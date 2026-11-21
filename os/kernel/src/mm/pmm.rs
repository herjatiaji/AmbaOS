use core::sync::atomic::{AtomicU64, Ordering};
use limine::memory_map::{Entry, EntryType};
use spin::Mutex;

pub const PAGE_SIZE: usize = 4096;
const MAX_FRAMES: usize = 4 * 1024 * 1024; // Supports up to 16 GiB physical RAM (512 KiB bitmap)
const BITMAP_WORDS: usize = MAX_FRAMES / 64;

#[derive(Clone, Copy)]
pub struct MemStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_frames: usize,
}

struct PmmState {
    bitmap: [u64; BITMAP_WORDS],
    managed_frames: usize,
    total_usable_frames: usize,
    used_frames: usize,
}

impl PmmState {
    const fn new() -> Self {
        Self {
            bitmap: [u64::MAX; BITMAP_WORDS],
            managed_frames: 0,
            total_usable_frames: 0,
            used_frames: 0,
        }
    }

    #[inline]
    fn is_used(&self, frame: usize) -> bool {
        let word = frame / 64;
        let bit = frame % 64;
        (self.bitmap[word] >> bit) & 1 != 0
    }

    #[inline]
    fn mark_free(&mut self, frame: usize) {
        if frame < self.managed_frames && self.is_used(frame) {
            let word = frame / 64;
            let bit = frame % 64;
            self.bitmap[word] &= !(1u64 << bit);
            self.used_frames = self.used_frames.saturating_sub(1);
        }
    }

    #[inline]
    fn mark_used(&mut self, frame: usize) {
        if frame < self.managed_frames && !self.is_used(frame) {
            let word = frame / 64;
            let bit = frame % 64;
            self.bitmap[word] |= 1u64 << bit;
            self.used_frames += 1;
        }
    }
}

static PMM: Mutex<PmmState> = Mutex::new(PmmState::new());
static HHDM_OFFSET: AtomicU64 = AtomicU64::new(0);

pub fn init(entries: &[&Entry], hhdm_offset: u64) -> MemStats {
    HHDM_OFFSET.store(hhdm_offset, Ordering::Relaxed);

    let mut max_phys: u64 = 0;
    for entry in entries {
        if entry.entry_type == EntryType::USABLE {
            let end = entry.base.saturating_add(entry.length);
            if end > max_phys {
                max_phys = end;
            }
        }
    }

    let managed_frames = ((max_phys as usize) / PAGE_SIZE).min(MAX_FRAMES);

    let mut pmm = PMM.lock();
    pmm.bitmap.fill(u64::MAX);
    pmm.managed_frames = managed_frames;
    pmm.total_usable_frames = 0;
    pmm.used_frames = 0;

    for entry in entries {
        if entry.entry_type == EntryType::USABLE {
            let start_frame = ((entry.base as usize + PAGE_SIZE - 1) / PAGE_SIZE).min(managed_frames);
            let end_frame = ((entry.base.saturating_add(entry.length) as usize) / PAGE_SIZE).min(managed_frames);
            for frame in start_frame..end_frame {
                // Keep bottom 1 MiB reserved for legacy/BIOS safety
                if frame >= 256 {
                    let word = frame / 64;
                    let bit = frame % 64;
                    if (pmm.bitmap[word] >> bit) & 1 != 0 {
                        pmm.bitmap[word] &= !(1u64 << bit);
                        pmm.total_usable_frames += 1;
                    }
                }
            }
        }
    }

    MemStats {
        total_bytes: (pmm.total_usable_frames * PAGE_SIZE) as u64,
        used_bytes: (pmm.used_frames * PAGE_SIZE) as u64,
        free_bytes: ((pmm.total_usable_frames - pmm.used_frames) * PAGE_SIZE) as u64,
        total_frames: pmm.total_usable_frames,
    }
}

pub fn hhdm_offset() -> u64 {
    HHDM_OFFSET.load(Ordering::Relaxed)
}

pub fn alloc_contiguous(pages: usize) -> Option<u64> {
    if pages == 0 {
        return None;
    }
    let mut pmm = PMM.lock();
    let limit = pmm.managed_frames;
    let mut run = 0usize;
    let mut run_start = 256usize;
    let mut frame = 256usize;

    while frame < limit {
        if (frame % 64 == 0) && run == 0 && pmm.bitmap[frame / 64] == u64::MAX {
            frame += 64;
            run_start = frame;
            continue;
        }
        if !pmm.is_used(frame) {
            if run == 0 {
                run_start = frame;
            }
            run += 1;
            if run == pages {
                for f in run_start..run_start + pages {
                    pmm.mark_used(f);
                }
                return Some((run_start * PAGE_SIZE) as u64);
            }
        } else {
            run = 0;
        }
        frame += 1;
    }
    None
}

#[allow(dead_code)]
pub fn free_frame(phys: u64) {
    let frame = (phys as usize) / PAGE_SIZE;
    PMM.lock().mark_free(frame);
}

pub fn stats() -> MemStats {
    let pmm = PMM.lock();
    let free = pmm.total_usable_frames.saturating_sub(pmm.used_frames);
    MemStats {
        total_bytes: (pmm.total_usable_frames * PAGE_SIZE) as u64,
        used_bytes: (pmm.used_frames * PAGE_SIZE) as u64,
        free_bytes: (free * PAGE_SIZE) as u64,
        total_frames: pmm.total_usable_frames,
    }
}
