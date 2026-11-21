use spin::Lazy;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

const STACK_SIZE: usize = 4096 * 5;
static mut DOUBLE_FAULT_STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
    let stack_end = stack_start + (STACK_SIZE as u64);
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack_end;
    tss
});

struct Selectors {
    code: SegmentSelector,
    data: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (gdt, Selectors { code, data, tss })
});

pub fn init() {
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code);
        DS::set_reg(GDT.1.data);
        ES::set_reg(GDT.1.data);
        SS::set_reg(GDT.1.data);
        load_tss(GDT.1.tss);
    }
}
