use x86_64::instructions::{hlt, interrupts, port::Port};

pub fn shutdown() -> ! {
    interrupts::disable();
    unsafe {
        // QEMU PIIX4 / Q35 ACPI shutdown ports & Bochs fallback
        Port::<u16>::new(0x604).write(0x2000);
        Port::<u16>::new(0xB004).write(0x2000);
        Port::<u16>::new(0x4004).write(0x3400);
    }
    loop {
        hlt();
    }
}

pub fn reboot() -> ! {
    interrupts::disable();
    unsafe {
        let mut status = Port::<u8>::new(0x64);
        for _ in 0..10_000 {
            if status.read() & 0x02 == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        status.write(0xFE);
    }
    loop {
        hlt();
    }
}
