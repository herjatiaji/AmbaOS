use pic8259::ChainedPics;
use spin::{Lazy, Mutex};
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

use crate::arch::x86_64::gdt;
use crate::drivers::{keyboard, mouse, timer};
use crate::serial_println;

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
    Mouse = PIC_2_OFFSET + 4,
}

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.divide_error.set_handler_fn(divide_error_handler);
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.general_protection_fault.set_handler_fn(gpf_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }

    idt[InterruptIndex::Timer as u8].set_handler_fn(timer_interrupt_handler);
    idt[InterruptIndex::Keyboard as u8].set_handler_fn(keyboard_interrupt_handler);
    idt[InterruptIndex::Mouse as u8].set_handler_fn(mouse_interrupt_handler);
    idt
});

pub fn init() {
    IDT.load();
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        // Unmask PIC1: IRQ0 (timer), IRQ1 (keyboard), IRQ2 (cascade) -> 0b1111_1000
        // Unmask PIC2: IRQ12 (bit 4 -> 40+4=44) -> 0b1110_1111
        pics.write_masks(0b1111_1000, 0b1110_1111);
    }
}

extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    panic!("CPU EXCEPTION: DIVIDE BY ZERO\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("[EXCEPTION] Breakpoint\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic!("CPU EXCEPTION: INVALID OPCODE\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn gpf_handler(stack_frame: InterruptStackFrame, error_code: u64) {
    panic!(
        "CPU EXCEPTION: GENERAL PROTECTION FAULT (err={:#x})\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let addr = Cr2::read();
    panic!(
        "CPU EXCEPTION: PAGE FAULT at {:?} ({:?})\n{:#?}",
        addr, error_code, stack_frame
    );
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!(
        "CPU EXCEPTION: DOUBLE FAULT (err={:#x})\n{:#?}",
        error_code, stack_frame
    );
}

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    timer::on_tick();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer as u8);
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    keyboard::on_irq();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard as u8);
    }
}

extern "x86-interrupt" fn mouse_interrupt_handler(_stack_frame: InterruptStackFrame) {
    mouse::on_irq();
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Mouse as u8);
    }
}
