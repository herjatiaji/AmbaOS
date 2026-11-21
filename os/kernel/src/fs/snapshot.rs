use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use crate::drivers::{ata, rtc};
use crate::fs::vfs;
use crate::serial_println;

const SB_MAGIC: [u8; 8] = *b"AMBASNP1";
const PAYLOAD_MAGIC: [u8; 4] = *b"APAY";
const SLOT_A_SB_LBA: u32 = 0;
const SLOT_B_SB_LBA: u32 = 1;
const SLOT_A_DATA_LBA: u32 = 2;
const SLOT_B_DATA_LBA: u32 = 2048;
const MAX_SLOT_SECTORS: u32 = 1024;
const MAX_GENERATIONS: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GenHealth {
    Verified = 0,
    Unverified = 1,
    Corrupt = 2,
}

impl GenHealth {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => GenHealth::Verified,
            2 => GenHealth::Corrupt,
            _ => GenHealth::Unverified,
        }
    }
}

#[derive(Clone)]
pub struct Generation {
    pub id: u32,
    pub timestamp: String,
    pub label: String,
    pub theme_idx: u8,
    pub show_watermark: bool,
    pub core_overrides: Vec<(String, Vec<u8>)>,
    pub health: GenHealth,
    pub hash: u64,
}

#[derive(Clone)]
pub struct BootDecision {
    pub show_boot_menu: bool,
    pub crash_detected: bool,
    pub recommended_gen: u32,
    pub crash_reason: String,
}

#[derive(Clone)]
pub struct SnapshotSummary {
    pub active_gen: u32,
    pub last_good_gen: u32,
    pub disk_backed: bool,
    pub disk_mib: u32,
    pub commit_seq: u64,
    pub active_slot: char,
    pub crash_count: u32,
    pub generations: Vec<Generation>,
}

struct SnapshotStore {
    next_gen_id: u32,
    active_gen: u32,
    last_good_gen: u32,
    boot_in_progress: u8,
    force_boot_menu: u8,
    crash_count: u32,
    last_crash_gen: u32,
    commit_seq: u64,
    active_slot: char,
    generations: Vec<Generation>,
}

#[derive(Clone, Copy)]
struct DiskSuperblock {
    seq: u64,
    active_gen: u32,
    last_good_gen: u32,
    boot_in_progress: u8,
    force_boot_menu: u8,
    crash_count: u32,
    last_crash_gen: u32,
    payload_lba: u32,
    payload_len: u32,
    payload_crc: u64,
}

static SNAP_STATE: Mutex<Option<SnapshotStore>> = Mutex::new(None);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn compute_gen_hash(g: &Generation) -> u64 {
    let mut buf = Vec::with_capacity(96);
    buf.extend_from_slice(&g.id.to_le_bytes());
    buf.push(g.theme_idx);
    buf.push(if g.show_watermark { 1 } else { 0 });
    buf.push(g.health as u8);
    buf.extend_from_slice(g.label.as_bytes());
    for (p, d) in &g.core_overrides {
        buf.extend_from_slice(p.as_bytes());
        buf.extend_from_slice(&(d.len() as u32).to_le_bytes());
        buf.extend_from_slice(d);
    }
    fnv1a64(&buf)
}

fn current_timestamp() -> String {
    let dt = rtc::read_rtc();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
    )
}

fn push_str(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let len = b.len().min(u16::MAX as usize) as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&b[..len as usize]);
}

fn read_u16(src: &[u8], off: &mut usize) -> Option<u16> {
    if *off + 2 > src.len() {
        return None;
    }
    let v = u16::from_le_bytes([src[*off], src[*off + 1]]);
    *off += 2;
    Some(v)
}

fn read_u32(src: &[u8], off: &mut usize) -> Option<u32> {
    if *off + 4 > src.len() {
        return None;
    }
    let v = u32::from_le_bytes([src[*off], src[*off + 1], src[*off + 2], src[*off + 3]]);
    *off += 4;
    Some(v)
}

fn read_u64(src: &[u8], off: &mut usize) -> Option<u64> {
    if *off + 8 > src.len() {
        return None;
    }
    let v = u64::from_le_bytes([
        src[*off],
        src[*off + 1],
        src[*off + 2],
        src[*off + 3],
        src[*off + 4],
        src[*off + 5],
        src[*off + 6],
        src[*off + 7],
    ]);
    *off += 8;
    Some(v)
}

fn read_str(src: &[u8], off: &mut usize) -> Option<String> {
    let len = read_u16(src, off)? as usize;
    if *off + len > src.len() {
        return None;
    }
    let s = core::str::from_utf8(&src[*off..*off + len]).ok()?;
    *off += len;
    Some(String::from(s))
}

fn serialize_payload(st: &SnapshotStore, user_files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    out.extend_from_slice(&PAYLOAD_MAGIC);
    out.extend_from_slice(&st.next_gen_id.to_le_bytes());
    out.extend_from_slice(&(st.generations.len() as u32).to_le_bytes());

    for g in &st.generations {
        out.extend_from_slice(&g.id.to_le_bytes());
        push_str(&mut out, &g.timestamp);
        push_str(&mut out, &g.label);
        out.push(g.theme_idx);
        out.push(if g.show_watermark { 1 } else { 0 });
        out.push(g.health as u8);
        out.extend_from_slice(&g.hash.to_le_bytes());
        out.extend_from_slice(&(g.core_overrides.len() as u16).to_le_bytes());
        for (p, d) in &g.core_overrides {
            push_str(&mut out, p);
            out.extend_from_slice(&(d.len() as u32).to_le_bytes());
            out.extend_from_slice(d);
        }
    }

    out.extend_from_slice(&(user_files.len() as u32).to_le_bytes());
    for (p, d) in user_files {
        push_str(&mut out, p);
        out.extend_from_slice(&(d.len() as u32).to_le_bytes());
        out.extend_from_slice(d);
    }

    out
}

fn deserialize_payload(
    src: &[u8],
) -> Option<(u32, Vec<Generation>, Vec<(String, Vec<u8>)>)> {
    if src.len() < 12 || src[0..4] != PAYLOAD_MAGIC {
        return None;
    }
    let mut off = 4usize;
    let next_gen_id = read_u32(src, &mut off)?;
    let g_count = read_u32(src, &mut off)? as usize;
    if g_count > 64 {
        return None;
    }

    let mut gens = Vec::with_capacity(g_count);
    for _ in 0..g_count {
        let id = read_u32(src, &mut off)?;
        let timestamp = read_str(src, &mut off)?;
        let label = read_str(src, &mut off)?;
        if off + 3 > src.len() {
            return None;
        }
        let theme_idx = src[off];
        let show_watermark = src[off + 1] != 0;
        let health = GenHealth::from_u8(src[off + 2]);
        off += 3;
        let hash = read_u64(src, &mut off)?;
        let ov_count = read_u16(src, &mut off)? as usize;
        if ov_count > 128 {
            return None;
        }
        let mut core_overrides = Vec::with_capacity(ov_count);
        for _ in 0..ov_count {
            let p = read_str(src, &mut off)?;
            let dlen = read_u32(src, &mut off)? as usize;
            if off + dlen > src.len() {
                return None;
            }
            let data = src[off..off + dlen].to_vec();
            off += dlen;
            core_overrides.push((p, data));
        }
        gens.push(Generation {
            id,
            timestamp,
            label,
            theme_idx,
            show_watermark,
            core_overrides,
            health,
            hash,
        });
    }

    let u_count = read_u32(src, &mut off)? as usize;
    if u_count > 512 {
        return None;
    }
    let mut ufiles = Vec::with_capacity(u_count);
    for _ in 0..u_count {
        let p = read_str(src, &mut off)?;
        let dlen = read_u32(src, &mut off)? as usize;
        if off + dlen > src.len() {
            return None;
        }
        let data = src[off..off + dlen].to_vec();
        off += dlen;
        ufiles.push((p, data));
    }

    Some((next_gen_id, gens, ufiles))
}

fn read_superblock(lba: u32) -> Option<DiskSuperblock> {
    let mut sec = [0u8; 512];
    if !ata::read_sector(lba, &mut sec) {
        return None;
    }
    if sec[0..8] != SB_MAGIC {
        return None;
    }
    let header_crc = u64::from_le_bytes(sec[50..58].try_into().ok()?);
    if fnv1a64(&sec[0..50]) != header_crc {
        return None;
    }
    let seq = u64::from_le_bytes(sec[8..16].try_into().ok()?);
    let active_gen = u32::from_le_bytes(sec[16..20].try_into().ok()?);
    let last_good_gen = u32::from_le_bytes(sec[20..24].try_into().ok()?);
    let boot_in_progress = sec[24];
    let force_boot_menu = sec[25];
    let crash_count = u32::from_le_bytes(sec[26..30].try_into().ok()?);
    let last_crash_gen = u32::from_le_bytes(sec[30..34].try_into().ok()?);
    let payload_lba = u32::from_le_bytes(sec[34..38].try_into().ok()?);
    let payload_len = u32::from_le_bytes(sec[38..42].try_into().ok()?);
    let payload_crc = u64::from_le_bytes(sec[42..50].try_into().ok()?);
    if payload_len == 0 || payload_len > MAX_SLOT_SECTORS * 512 {
        return None;
    }
    Some(DiskSuperblock {
        seq,
        active_gen,
        last_good_gen,
        boot_in_progress,
        force_boot_menu,
        crash_count,
        last_crash_gen,
        payload_lba,
        payload_len,
        payload_crc,
    })
}

fn load_slot_from_disk(sb: &DiskSuperblock) -> Option<(u32, Vec<Generation>, Vec<(String, Vec<u8>)>)> {
    let sectors = (sb.payload_len + 511) / 512;
    if sectors == 0 || sectors > MAX_SLOT_SECTORS {
        return None;
    }
    let mut raw = vec![0u8; (sectors as usize) * 512];
    for i in 0..sectors {
        let mut sec = [0u8; 512];
        if !ata::read_sector(sb.payload_lba + i, &mut sec) {
            return None;
        }
        let off = (i as usize) * 512;
        raw[off..off + 512].copy_from_slice(&sec);
    }
    let slice = &raw[..sb.payload_len as usize];
    if fnv1a64(slice) != sb.payload_crc {
        return None;
    }
    deserialize_payload(slice)
}

fn prune_generations(st: &mut SnapshotStore) {
    while st.generations.len() > MAX_GENERATIONS {
        if let Some(pos) = st
            .generations
            .iter()
            .position(|g| g.id != 1 && g.id != st.active_gen && g.id != st.last_good_gen)
        {
            st.generations.remove(pos);
        } else {
            break;
        }
    }
}

fn commit_to_disk(st: &mut SnapshotStore) {
    prune_generations(st);
    if !ata::is_present() || ata::total_sectors() < 4096 {
        st.commit_seq += 1;
        return;
    }

    let user_files = vfs::export_user_files();
    let payload = serialize_payload(st, &user_files);
    let sectors = ((payload.len() + 511) / 512) as u32;
    if sectors == 0 || sectors > MAX_SLOT_SECTORS {
        return;
    }

    let (next_slot, sb_lba, data_lba) = if st.active_slot == 'A' {
        ('B', SLOT_B_SB_LBA, SLOT_B_DATA_LBA)
    } else {
        ('A', SLOT_A_SB_LBA, SLOT_A_DATA_LBA)
    };

    for i in 0..sectors {
        let mut sec = [0u8; 512];
        let start = (i as usize) * 512;
        let end = (start + 512).min(payload.len());
        let len = end - start;
        sec[..len].copy_from_slice(&payload[start..end]);
        if !ata::write_sector(data_lba + i, &sec) {
            return;
        }
    }
    ata::flush();

    let next_seq = st.commit_seq + 1;
    let payload_crc = fnv1a64(&payload);
    let mut sb = [0u8; 512];
    sb[0..8].copy_from_slice(&SB_MAGIC);
    sb[8..16].copy_from_slice(&next_seq.to_le_bytes());
    sb[16..20].copy_from_slice(&st.active_gen.to_le_bytes());
    sb[20..24].copy_from_slice(&st.last_good_gen.to_le_bytes());
    sb[24] = st.boot_in_progress;
    sb[25] = st.force_boot_menu;
    sb[26..30].copy_from_slice(&st.crash_count.to_le_bytes());
    sb[30..34].copy_from_slice(&st.last_crash_gen.to_le_bytes());
    sb[34..38].copy_from_slice(&data_lba.to_le_bytes());
    sb[38..42].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    sb[42..50].copy_from_slice(&payload_crc.to_le_bytes());
    let hcrc = fnv1a64(&sb[0..50]);
    sb[50..58].copy_from_slice(&hcrc.to_le_bytes());

    if ata::write_sector(sb_lba, &sb) {
        ata::flush();
        st.commit_seq = next_seq;
        st.active_slot = next_slot;
    }
}

pub fn init() -> BootDecision {
    let mut loaded: Option<(DiskSuperblock, char, u32, Vec<Generation>, Vec<(String, Vec<u8>)>)> = None;

    if ata::is_present() && ata::total_sectors() >= 4096 {
        let mut candidates: Vec<(DiskSuperblock, char)> = Vec::new();
        if let Some(sb_a) = read_superblock(SLOT_A_SB_LBA) {
            candidates.push((sb_a, 'A'));
        }
        if let Some(sb_b) = read_superblock(SLOT_B_SB_LBA) {
            candidates.push((sb_b, 'B'));
        }
        candidates.sort_by(|x, y| y.0.seq.cmp(&x.0.seq));

        for (sb, slot) in candidates {
            if let Some((next_id, gens, ufiles)) = load_slot_from_disk(&sb) {
                loaded = Some((sb, slot, next_id, gens, ufiles));
                break;
            }
        }
    }

    let (store, crash_detected, recommended_gen, crash_reason) = if let Some((sb, slot, next_id, mut gens, ufiles)) = loaded {
        if !ufiles.is_empty() {
            vfs::restore_user_files(ufiles);
        }

        let mut crashed = false;
        let mut reason = String::new();
        let active_is_corrupt = gens
            .iter()
            .find(|g| g.id == sb.active_gen)
            .map(|g| g.health == GenHealth::Corrupt)
            .unwrap_or(false);

        if active_is_corrupt {
            crashed = true;
            reason = format!("Staged Gen #{} failed integrity verification (CORRUPT).", sb.active_gen);
        } else if sb.boot_in_progress != 0 {
            crashed = true;
            if let Some(g) = gens.iter_mut().find(|g| g.id == sb.active_gen) {
                if g.id != 1 {
                    g.health = GenHealth::Corrupt;
                    g.hash = compute_gen_hash(g);
                }
            }
            reason = format!(
                "Previous boot of Gen #{} crashed or reset before reaching desktop.",
                sb.active_gen
            );
        }

        let mut rec = if crashed { sb.last_good_gen } else { sb.active_gen };
        let rec_ok = gens
            .iter()
            .any(|g| g.id == rec && g.health != GenHealth::Corrupt);
        if !rec_ok {
            rec = gens
                .iter()
                .rev()
                .find(|g| g.health != GenHealth::Corrupt)
                .map(|g| g.id)
                .unwrap_or(1);
        }

        let crash_count = if crashed { sb.crash_count + 1 } else { sb.crash_count };
        let last_crash_gen = if crashed { sb.active_gen } else { sb.last_crash_gen };

        serial_println!(
            "[OK] Immutable Snapshot Store loaded (Slot {}, seq {}, {} gen(s), active=#{}, last_good=#{})",
            slot,
            sb.seq,
            gens.len(),
            sb.active_gen,
            rec
        );

        let st = SnapshotStore {
            next_gen_id: next_id.max(gens.iter().map(|g| g.id + 1).max().unwrap_or(2)),
            active_gen: sb.active_gen,
            last_good_gen: rec,
            boot_in_progress: sb.boot_in_progress,
            force_boot_menu: sb.force_boot_menu,
            crash_count,
            last_crash_gen,
            commit_seq: sb.seq,
            active_slot: slot,
            generations: gens,
        };
        (st, crashed, rec, reason)
    } else {
        let mut g1 = Generation {
            id: 1,
            timestamp: current_timestamp(),
            label: String::from("Factory Base Image (AmbaOS v0.5.0)"),
            theme_idx: 0,
            show_watermark: true,
            core_overrides: Vec::new(),
            health: GenHealth::Verified,
            hash: 0,
        };
        g1.hash = compute_gen_hash(&g1);

        let mut st = SnapshotStore {
            next_gen_id: 2,
            active_gen: 1,
            last_good_gen: 1,
            boot_in_progress: 0,
            force_boot_menu: 0,
            crash_count: 0,
            last_crash_gen: 0,
            commit_seq: 0,
            active_slot: 'B',
            generations: vec![g1],
        };
        commit_to_disk(&mut st);
        serial_println!(
            "[OK] Immutable Factory Gen #1 initialized (slot {}, seq {})",
            st.active_slot,
            st.commit_seq
        );
        (st, false, 1, String::new())
    };

    let show_menu =
        crash_detected || store.force_boot_menu != 0 || store.generations.len() > 1;
    let rec_final = recommended_gen;

    *SNAP_STATE.lock() = Some(store);
    INITIALIZED.store(true, Ordering::Relaxed);

    BootDecision {
        show_boot_menu: show_menu,
        crash_detected,
        recommended_gen: rec_final,
        crash_reason,
    }
}

pub fn select_boot_generation(gen_id: u32) -> Result<(), &'static str> {
    let mut guard = SNAP_STATE.lock();
    let st = guard.as_mut().ok_or("Snapshot engine uninitialized")?;
    let g = st
        .generations
        .iter()
        .find(|g| g.id == gen_id)
        .ok_or("Generation not found")?
        .clone();
    if g.health == GenHealth::Corrupt {
        return Err("Selected generation is marked CORRUPT. Choose a VERIFIED snapshot.");
    }
    st.active_gen = g.id;
    st.boot_in_progress = 1;
    st.force_boot_menu = 0;
    commit_to_disk(st);
    vfs::apply_core_overrides(&g.core_overrides);
    serial_println!("[OK] Mounted Immutable Core Gen #{} ({})", g.id, g.label);
    Ok(())
}

pub fn mark_boot_healthy() {
    let mut guard = SNAP_STATE.lock();
    let Some(st) = guard.as_mut() else {
        return;
    };
    st.boot_in_progress = 0;
    st.force_boot_menu = 0;
    let active_id = st.active_gen;
    if let Some(g) = st.generations.iter_mut().find(|g| g.id == active_id) {
        g.health = GenHealth::Verified;
        g.hash = compute_gen_hash(g);
    }
    st.last_good_gen = active_id;
    commit_to_disk(st);
}

pub fn active_config() -> (u8, bool) {
    let guard = SNAP_STATE.lock();
    if let Some(st) = guard.as_ref() {
        if let Some(g) = st.generations.iter().find(|g| g.id == st.active_gen) {
            return (g.theme_idx, g.show_watermark);
        }
    }
    (0, true)
}

pub fn summary() -> SnapshotSummary {
    let guard = SNAP_STATE.lock();
    let disk_backed = ata::is_present();
    let disk_mib = (ata::total_sectors() / 2048).max(if disk_backed { 1 } else { 0 });
    match guard.as_ref() {
        Some(st) => SnapshotSummary {
            active_gen: st.active_gen,
            last_good_gen: st.last_good_gen,
            disk_backed,
            disk_mib,
            commit_seq: st.commit_seq,
            active_slot: st.active_slot,
            crash_count: st.crash_count,
            generations: st.generations.clone(),
        },
        None => SnapshotSummary {
            active_gen: 1,
            last_good_gen: 1,
            disk_backed,
            disk_mib,
            commit_seq: 0,
            active_slot: 'A',
            crash_count: 0,
            generations: Vec::new(),
        },
    }
}

pub fn sync_user_data() {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    let mut guard = SNAP_STATE.lock();
    if let Some(st) = guard.as_mut() {
        commit_to_disk(st);
    }
}

pub fn create_snapshot(label: &str, theme_idx: u8, show_watermark: bool) -> u32 {
    let mut guard = SNAP_STATE.lock();
    let Some(st) = guard.as_mut() else {
        return 1;
    };
    let cur_ov = st
        .generations
        .iter()
        .find(|g| g.id == st.active_gen)
        .map(|g| g.core_overrides.clone())
        .unwrap_or_default();

    let id = st.next_gen_id;
    st.next_gen_id += 1;

    let clean_lbl = label.trim();
    let lbl = if clean_lbl.is_empty() {
        format!("System Snapshot #{}", id)
    } else {
        String::from(clean_lbl)
    };

    let mut g = Generation {
        id,
        timestamp: current_timestamp(),
        label: lbl,
        theme_idx,
        show_watermark,
        core_overrides: cur_ov,
        health: GenHealth::Verified,
        hash: 0,
    };
    g.hash = compute_gen_hash(&g);
    st.generations.push(g);
    st.active_gen = id;
    st.last_good_gen = id;
    commit_to_disk(st);
    serial_println!("[snap] Created Gen #{} (seq {})", id, st.commit_seq);
    id
}

fn upsert_override(list: &mut Vec<(String, Vec<u8>)>, path: &str, data: Vec<u8>) {
    let norm = vfs::normalize_path(path);
    if let Some(item) = list.iter_mut().find(|(p, _)| *p == norm) {
        item.1 = data;
    } else {
        list.push((norm, data));
    }
}

pub fn apply_atomic_update(theme_idx: u8, show_watermark: bool) -> u32 {
    let mut guard = SNAP_STATE.lock();
    let Some(st) = guard.as_mut() else {
        return 1;
    };
    let mut ov = st
        .generations
        .iter()
        .find(|g| g.id == st.active_gen)
        .map(|g| g.core_overrides.clone())
        .unwrap_or_default();

    let id = st.next_gen_id;
    st.next_gen_id += 1;
    let ts = current_timestamp();

    let ver_str = format!("AmbaOS 0.5.{}-atomic\n", id);
    upsert_override(&mut ov, "/etc/version", ver_str.into_bytes());

    let mut notes = vfs::read_file("/etc/release-notes.txt")
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| String::from("AmbaOS Atomic Core Release History\n==================================\n"));
    notes.push_str(&format!("- Gen #{} [{}] Atomic OS Core Update 0.5.{}\n", id, ts, id));
    upsert_override(&mut ov, "/etc/release-notes.txt", notes.into_bytes());

    let mut g = Generation {
        id,
        timestamp: ts,
        label: format!("Atomic Core OS Update (0.5.{})", id),
        theme_idx,
        show_watermark,
        core_overrides: ov.clone(),
        health: GenHealth::Verified,
        hash: 0,
    };
    g.hash = compute_gen_hash(&g);
    st.generations.push(g);
    st.active_gen = id;
    st.last_good_gen = id;
    commit_to_disk(st);
    vfs::apply_core_overrides(&ov);
    serial_println!("[snap] Atomic Core Update applied -> Gen #{} (0.5.{})", id, id);
    id
}

pub fn stage_corrupt_update(theme_idx: u8, show_watermark: bool) -> u32 {
    let mut guard = SNAP_STATE.lock();
    let Some(st) = guard.as_mut() else {
        return 1;
    };
    let mut ov = st
        .generations
        .iter()
        .find(|g| g.id == st.active_gen)
        .map(|g| g.core_overrides.clone())
        .unwrap_or_default();

    let id = st.next_gen_id;
    st.next_gen_id += 1;
    upsert_override(&mut ov, "/etc/version", b"CORRUPT_IMAGE_BAD_CHECKSUM\n".to_vec());

    let mut g = Generation {
        id,
        timestamp: current_timestamp(),
        label: String::from("Broken Dependency Update (Corrupt Test)"),
        theme_idx,
        show_watermark,
        core_overrides: ov,
        health: GenHealth::Corrupt,
        hash: 0xDEAD_BEEF_BAD0_CAFE,
    };
    g.hash = compute_gen_hash(&g);
    st.generations.push(g);
    // Target this corrupt gen on next boot while keeping last_good_gen intact
    st.active_gen = id;
    st.force_boot_menu = 1;
    commit_to_disk(st);
    serial_println!(
        "[snap] Staged CORRUPT Gen #{} for next boot (last_good remains #{})",
        id,
        st.last_good_gen
    );
    id
}

pub fn rollback_live(gen_id: u32) -> Result<(u8, bool), &'static str> {
    let mut guard = SNAP_STATE.lock();
    let st = guard.as_mut().ok_or("Snapshot engine uninitialized")?;
    let g = st
        .generations
        .iter()
        .find(|g| g.id == gen_id)
        .ok_or("Generation not found")?
        .clone();
    if g.health == GenHealth::Corrupt {
        return Err("Cannot rollback to a CORRUPT generation");
    }
    st.active_gen = g.id;
    st.last_good_gen = g.id;
    commit_to_disk(st);
    vfs::apply_core_overrides(&g.core_overrides);
    serial_println!("[snap] Live rollback switched Core OS to Gen #{}", g.id);
    Ok((g.theme_idx, g.show_watermark))
}

pub fn delete_generation(gen_id: u32) -> Result<(), &'static str> {
    let mut guard = SNAP_STATE.lock();
    let st = guard.as_mut().ok_or("Snapshot engine uninitialized")?;
    if gen_id == 1 {
        return Err("Cannot delete Factory Base Image Gen #1");
    }
    if gen_id == st.active_gen {
        return Err("Cannot delete currently active generation");
    }
    let pos = st
        .generations
        .iter()
        .position(|g| g.id == gen_id)
        .ok_or("Generation not found")?;
    st.generations.remove(pos);
    if st.last_good_gen == gen_id {
        st.last_good_gen = st.active_gen;
    }
    commit_to_disk(st);
    Ok(())
}

pub fn request_boot_menu_and_reboot() -> ! {
    {
        let mut guard = SNAP_STATE.lock();
        if let Some(st) = guard.as_mut() {
            st.force_boot_menu = 1;
            commit_to_disk(st);
        }
    }
    serial_println!("[snap] Rebooting to Atomic Rollback Boot Manager...");
    crate::drivers::power::reboot()
}
