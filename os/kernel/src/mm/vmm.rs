use x86_64::VirtAddr;
use x86_64::instructions::tlb;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTable, PageTableFlags};

use crate::mm::pmm;

#[inline]
pub fn active_pml4_phys() -> u64 {
    let (frame, _flags) = Cr3::read();
    frame.start_address().as_u64()
}

#[inline]
pub fn phys_to_virt(phys: u64) -> *mut u8 {
    (phys + pmm::hhdm_offset()) as *mut u8
}

unsafe fn ensure_next_table(entry: &mut x86_64::structures::paging::page_table::PageTableEntry) -> Option<*mut PageTable> {
    let flags = entry.flags();
    if flags.contains(PageTableFlags::PRESENT) {
        if flags.contains(PageTableFlags::HUGE_PAGE) {
            return None;
        }
        return Some(phys_to_virt(entry.addr().as_u64()) as *mut PageTable);
    }

    let frame = pmm::alloc_contiguous(1).expect("Out of frames allocating page table");
    let virt = phys_to_virt(frame);
    unsafe {
        core::ptr::write_bytes(virt, 0, pmm::PAGE_SIZE);
    }
    entry.set_addr(
        x86_64::PhysAddr::new(frame),
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
    );
    Some(virt as *mut PageTable)
}

pub fn map_mmio_region(phys_base: u64, size: usize) -> *mut u8 {
    let page_mask = !(pmm::PAGE_SIZE as u64 - 1);
    let start_phys = phys_base & page_mask;
    let end_phys = (phys_base + size as u64 + pmm::PAGE_SIZE as u64 - 1) & page_mask;
    let hhdm = pmm::hhdm_offset();
    let p4 = phys_to_virt(active_pml4_phys()) as *mut PageTable;
    let mmio_flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::WRITE_THROUGH
        | PageTableFlags::NO_CACHE;

    let mut phys = start_phys;
    while phys < end_phys {
        let virt = hhdm + phys;
        let p4_idx = ((virt >> 39) & 0x1FF) as usize;
        let p3_idx = ((virt >> 30) & 0x1FF) as usize;
        let p2_idx = ((virt >> 21) & 0x1FF) as usize;
        let p1_idx = ((virt >> 12) & 0x1FF) as usize;

        unsafe {
            if let Some(p3) = ensure_next_table(&mut (&mut *p4)[p4_idx]) {
                if let Some(p2) = ensure_next_table(&mut (&mut *p3)[p3_idx]) {
                    if let Some(p1) = ensure_next_table(&mut (&mut *p2)[p2_idx]) {
                        let leaf = &mut (&mut *p1)[p1_idx];
                        if !leaf.flags().contains(PageTableFlags::PRESENT) {
                            leaf.set_addr(x86_64::PhysAddr::new(phys), mmio_flags);
                            tlb::flush(VirtAddr::new(virt));
                        }
                    }
                }
            }
        }
        phys += pmm::PAGE_SIZE as u64;
    }

    (hhdm + phys_base) as *mut u8
}
