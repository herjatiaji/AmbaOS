use core::sync::atomic::{AtomicBool, Ordering};
use linked_list_allocator::LockedHeap;
use crate::mm::{pmm, vmm};

pub const HEAP_SIZE: usize = 256 * 1024 * 1024; // 256 MiB
const HEAP_PAGES: usize = HEAP_SIZE / pmm::PAGE_SIZE;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let phys = pmm::alloc_contiguous(HEAP_PAGES)
        .expect("Failed to allocate contiguous physical frames for 32 MiB Kernel Heap");
    let virt = vmm::phys_to_virt(phys);
    unsafe {
        ALLOCATOR.lock().init(virt, HEAP_SIZE);
    }
    INITIALIZED.store(true, Ordering::Relaxed);
}

pub fn stats() -> (usize, usize) {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return (0, HEAP_SIZE);
    }
    let guard = ALLOCATOR.lock();
    (guard.used(), guard.size())
}
