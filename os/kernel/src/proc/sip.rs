use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::proc::caps::{self, AppId, CAP_FS_WRITE, CAP_NPU_COMPUTE, CAP_ZC_DMA};
use crate::proc::uring::rdtsc;
use crate::proc::zerocopy;

const ISOLATE_MEM_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SipStatus {
    Running,
    TrappedFault,
}

#[derive(Clone, Copy)]
pub enum SipInstr {
    LoadImm(usize, u32),
    Add(usize, usize),
    BoundsStore(usize, u8),
    BoundsLoad(usize),
    HashMem(usize, usize),
}

#[derive(Clone)]
pub struct SipIsolate {
    pub id: u16,
    pub name: &'static str,
    pub profile: &'static str,
    pub linear_mem: Vec<u8>,
    pub regs: [u32; 4],
    pub boot_cycles: u64,
    pub ipc_count: u64,
    pub last_ipc_cycles: u64,
    pub status: SipStatus,
    pub caps: u32,
}

struct SipEngine {
    next_id: u16,
    isolates: Vec<SipIsolate>,
    last_event: String,
    tlb_flushes: u64,
}

static SIP: Mutex<Option<SipEngine>> = Mutex::new(None);

fn run_sandbox(iso: &mut SipIsolate, prog: &[SipInstr]) -> Result<u32, &'static str> {
    let mut acc = 0u32;
    for &ins in prog {
        match ins {
            SipInstr::LoadImm(r, v) => {
                if let Some(slot) = iso.regs.get_mut(r) {
                    *slot = v;
                }
            }
            SipInstr::Add(dst, src) => {
                let sv = iso.regs.get(src).copied().unwrap_or(0);
                if let Some(slot) = iso.regs.get_mut(dst) {
                    *slot = slot.wrapping_add(sv);
                    acc = *slot;
                }
            }
            SipInstr::BoundsStore(off, val) => {
                if off >= iso.linear_mem.len() {
                    iso.status = SipStatus::TrappedFault;
                    return Err("SIP Bounds Fault on STORE (>16KiB sandbox)");
                }
                iso.linear_mem[off] = val;
            }
            SipInstr::BoundsLoad(off) => {
                if off >= iso.linear_mem.len() {
                    iso.status = SipStatus::TrappedFault;
                    return Err("SIP Bounds Fault on LOAD (>16KiB sandbox)");
                }
                acc = iso.linear_mem[off] as u32;
            }
            SipInstr::HashMem(start, len) => {
                if start.saturating_add(len) > iso.linear_mem.len() {
                    iso.status = SipStatus::TrappedFault;
                    return Err("SIP Bounds Fault on HASH");
                }
                let mut h: u32 = 0x811C_9DC5;
                for &b in &iso.linear_mem[start..start + len] {
                    h ^= b as u32;
                    h = h.wrapping_mul(0x0100_0193);
                }
                acc = h;
            }
        }
    }
    Ok(acc)
}

fn spawn_internal(
    id: u16,
    name: &'static str,
    profile: &'static str,
    caps_mask: u32,
) -> SipIsolate {
    let t0 = rdtsc();
    let mut iso = SipIsolate {
        id,
        name,
        profile,
        linear_mem: vec![0u8; ISOLATE_MEM_BYTES],
        regs: [0; 4],
        boot_cycles: 0,
        ipc_count: 0,
        last_ipc_cycles: 0,
        status: SipStatus::Running,
        caps: caps_mask,
    };
    let _ = run_sandbox(
        &mut iso,
        &[
            SipInstr::LoadImm(0, 7),
            SipInstr::LoadImm(1, 35),
            SipInstr::Add(0, 1),
            SipInstr::BoundsStore(0, 0x42),
            SipInstr::HashMem(0, 64),
        ],
    );
    iso.boot_cycles = rdtsc().saturating_sub(t0);
    iso
}

pub fn init() {
    let isolates = vec![
        spawn_internal(1, "http_filter_uk", "Unikernel-Net", CAP_ZC_DMA),
        spawn_internal(2, "telemetry_sip", "SIP-Observer", CAP_ZC_DMA | CAP_NPU_COMPUTE),
        spawn_internal(3, "crypto_hash_uk", "Unikernel-SHA", CAP_NPU_COMPUTE),
    ];
    *SIP.lock() = Some(SipEngine {
        next_id: 4,
        isolates,
        last_event: String::from("Single-Address-Space SIP Engine ready (0 TLB flushes)"),
        tlb_flushes: 0,
    });
}

pub fn spawn_next_unikernel() -> String {
    let mut guard = SIP.lock();
    let Some(eng) = guard.as_mut() else {
        return String::from("SIP engine uninitialized");
    };
    let id = eng.next_id;
    eng.next_id += 1;
    let names = ["zram_worker_uk", "codec_dsp_uk", "pdf_sandbox_uk", "wasm_iso_uk"];
    let name = names[(id as usize) % names.len()];
    let iso = spawn_internal(id, name, "Micro-Unikernel", CAP_ZC_DMA | CAP_NPU_COMPUTE);
    let cyc = iso.boot_cycles;
    if eng.isolates.len() >= 7 {
        eng.isolates.remove(3);
    }
    eng.isolates.push(iso);
    let msg = format!(
        "Booted {} #{} (<1ms / {} cyc, 16 KiB footprint, 0 TLB flush)",
        name, id, cyc
    );
    eng.last_event = msg.clone();
    msg
}

pub fn run_nanosecond_ipc_demo() -> String {
    let payload = b"AmbaOS-SAS-ZeroCopy-IPC-Packet";
    let Some(h1) = zerocopy::alloc_slot(AppId::SipIsolate, payload) else {
        return String::from("IPC failed: CAP_ZC_DMA denied or arena full");
    };
    let t0 = rdtsc();
    let res = zerocopy::transfer_ownership(h1, AppId::Browser);
    let cyc = rdtsc().saturating_sub(t0);
    match res {
        Ok(h2) => {
            zerocopy::release_slot(h2);
            let mut guard = SIP.lock();
            if let Some(eng) = guard.as_mut() {
                if let Some(iso) = eng.isolates.first_mut() {
                    iso.ipc_count += 1;
                    iso.last_ipc_cycles = cyc;
                }
                let msg = format!("SAS Zero-Copy IPC completed in {} CPU cycles (0 TLB flush)", cyc);
                eng.last_event = msg.clone();
                msg
            } else {
                String::from("IPC OK")
            }
        }
        Err(e) => {
            zerocopy::release_slot(h1);
            String::from(e)
        }
    }
}

pub fn run_rogue_sip_trap_demo() -> String {
    let _cap_denied = caps::check(AppId::SipIsolate, CAP_FS_WRITE);
    let mut guard = SIP.lock();
    let Some(eng) = guard.as_mut() else {
        return String::from("SIP engine uninitialized");
    };
    let id = eng.next_id;
    eng.next_id += 1;
    let mut rogue = spawn_internal(id, "rogue_untrusted_uk", "Wasm/SIP-Rogue", 0);
    let fault = run_sandbox(
        &mut rogue,
        &[
            SipInstr::LoadImm(0, 99),
            SipInstr::BoundsLoad(0x9999),
        ],
    );
    if eng.isolates.len() >= 7 {
        eng.isolates.remove(3);
    }
    eng.isolates.push(rogue);
    let msg = match fault {
        Err(reason) => format!("TRAPPED Rogue SIP #{}: {} (Kernel safe!)", id, reason),
        Ok(_) => String::from("Unexpected pass"),
    };
    eng.last_event = msg.clone();
    msg
}

pub fn snapshot() -> (Vec<SipIsolate>, String, u64) {
    let guard = SIP.lock();
    match guard.as_ref() {
        Some(eng) => (eng.isolates.clone(), eng.last_event.clone(), eng.tlb_flushes),
        None => (Vec::new(), String::from("Uninitialized"), 0),
    }
}
