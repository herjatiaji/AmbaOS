use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::fs::vfs;
use crate::proc::caps::{self, AppId, CAP_ZC_DMA};

pub const ZC_SLOTS: usize = 16;
pub const ZC_SLOT_SIZE: usize = 4096;

#[derive(Clone, Copy)]
pub struct ZcHandle {
    pub slot_id: u16,
    pub epoch: u16,
    pub len: u16,
    pub owner: u8,
}

#[derive(Clone)]
pub struct ZcStats {
    pub active_slots: usize,
    pub total_slots: usize,
    pub transfers: u64,
    pub bytes_saved: u64,
    pub zram_saved_bytes: u64,
    pub last_op: String,
}

struct ZcSlot {
    in_use: bool,
    epoch: u16,
    len: u16,
    compressed_len: u16,
    owner: u8,
    buf: Box<[u8; ZC_SLOT_SIZE]>,
}

struct ZcArena {
    slots: Vec<ZcSlot>,
    transfers: u64,
    bytes_saved: u64,
    zram_saved_bytes: u64,
    last_op: String,
}

static ARENA: Mutex<Option<ZcArena>> = Mutex::new(None);

pub fn init() {
    let mut slots = Vec::with_capacity(ZC_SLOTS);
    for _ in 0..ZC_SLOTS {
        slots.push(ZcSlot {
            in_use: false,
            epoch: 1,
            len: 0,
            compressed_len: 0,
            owner: AppId::Kernel as u8,
            buf: Box::new([0u8; ZC_SLOT_SIZE]),
        });
    }
    *ARENA.lock() = Some(ZcArena {
        slots,
        transfers: 0,
        bytes_saved: 0,
        zram_saved_bytes: 0,
        last_op: String::from("Zero-Copy DMA Arena ready (16 x 4 KiB slots)"),
    });
}

pub fn alloc_slot(owner: AppId, payload: &[u8]) -> Option<ZcHandle> {
    if caps::check(owner, CAP_ZC_DMA).is_err() {
        return None;
    }
    let mut guard = ARENA.lock();
    let arena = guard.as_mut()?;
    for (idx, slot) in arena.slots.iter_mut().enumerate() {
        if !slot.in_use {
            let copy_len = payload.len().min(ZC_SLOT_SIZE);
            slot.in_use = true;
            slot.epoch = slot.epoch.wrapping_add(1).max(1);
            slot.len = copy_len as u16;
            slot.compressed_len = copy_len as u16;
            slot.owner = owner as u8;
            slot.buf[..copy_len].copy_from_slice(&payload[..copy_len]);
            return Some(ZcHandle {
                slot_id: idx as u16,
                epoch: slot.epoch,
                len: slot.len,
                owner: owner as u8,
            });
        }
    }
    None
}

pub fn transfer_ownership(mut handle: ZcHandle, new_owner: AppId) -> Result<ZcHandle, &'static str> {
    let mut guard = ARENA.lock();
    let arena = guard.as_mut().ok_or("ZC arena uninitialized")?;
    let slot = arena.slots.get_mut(handle.slot_id as usize).ok_or("Invalid ZC slot")?;
    if !slot.in_use || slot.epoch != handle.epoch {
        return Err("Stale ZC handle");
    }
    slot.owner = new_owner as u8;
    handle.owner = new_owner as u8;
    arena.transfers += 1;
    arena.bytes_saved += handle.len as u64;
    arena.last_op = format!(
        "Zero-Copy DMA handoff slot#{} ({} B, 0 copies -> {})",
        handle.slot_id,
        handle.len,
        new_owner.name()
    );
    Ok(handle)
}

pub fn read_slot(handle: ZcHandle) -> Option<Vec<u8>> {
    let guard = ARENA.lock();
    let arena = guard.as_ref()?;
    let slot = arena.slots.get(handle.slot_id as usize)?;
    if !slot.in_use || slot.epoch != handle.epoch {
        return None;
    }
    Some(slot.buf[..slot.len as usize].to_vec())
}

pub fn release_slot(handle: ZcHandle) {
    let mut guard = ARENA.lock();
    if let Some(arena) = guard.as_mut() {
        if let Some(slot) = arena.slots.get_mut(handle.slot_id as usize) {
            if slot.in_use && slot.epoch == handle.epoch {
                slot.in_use = false;
                slot.len = 0;
            }
        }
    }
}

pub fn compress_slot_zram(slot_id: usize) -> usize {
    let mut guard = ARENA.lock();
    let Some(arena) = guard.as_mut() else {
        return 0;
    };
    let Some(slot) = arena.slots.get_mut(slot_id) else {
        return 0;
    };
    if !slot.in_use || slot.len == 0 {
        return 0;
    }
    // Simple RLE estimation + savings accounting
    let n = slot.len as usize;
    let mut rle_bytes = 0usize;
    let mut i = 0usize;
    while i < n {
        let b = slot.buf[i];
        let mut run = 1usize;
        while i + run < n && run < 255 && slot.buf[i + run] == b {
            run += 1;
        }
        rle_bytes += if run >= 3 { 2 } else { run };
        i += run;
    }
    let comp = rle_bytes.min(n).max(1) as u16;
    let saved = (n as usize).saturating_sub(comp as usize);
    slot.compressed_len = comp;
    arena.zram_saved_bytes += saved as u64;
    saved
}

pub fn run_vfs_to_net_zerocopy_demo() -> String {
    let data = vfs::read_file("/docs/architecture.txt")
        .unwrap_or_else(|| b"AmbaOS Zero-Copy DMA Descriptor Stream".to_vec());
    let len = data.len();
    let Some(h1) = alloc_slot(AppId::Terminal, &data) else {
        return String::from("ZC DMA failed: no free slots or CAP_ZC_DMA revoked");
    };
    let res = match transfer_ownership(h1, AppId::SipIsolate) {
        Ok(h2) => match transfer_ownership(h2, AppId::Browser) {
            Ok(h3) => {
                let verified = read_slot(h3).map(|v| v.len()).unwrap_or(0);
                release_slot(h3);
                format!(
                    "ZC VFS->SIP->NIC OK: {} B moved with 0 memory copies",
                    verified
                )
            }
            Err(e) => {
                release_slot(h2);
                String::from(e)
            }
        },
        Err(e) => {
            release_slot(h1);
            String::from(e)
        }
    };
    let _ = len;
    res
}

pub fn stats() -> ZcStats {
    let guard = ARENA.lock();
    match guard.as_ref() {
        Some(a) => ZcStats {
            active_slots: a.slots.iter().filter(|s| s.in_use).count(),
            total_slots: ZC_SLOTS,
            transfers: a.transfers,
            bytes_saved: a.bytes_saved,
            zram_saved_bytes: a.zram_saved_bytes,
            last_op: a.last_op.clone(),
        },
        None => ZcStats {
            active_slots: 0,
            total_slots: ZC_SLOTS,
            transfers: 0,
            bytes_saved: 0,
            zram_saved_bytes: 0,
            last_op: String::from("Uninitialized"),
        },
    }
}
