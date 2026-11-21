use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::instructions::port::Port;

const PIT_BASE_HZ: u32 = 1_193_180;
static MUTED: AtomicBool = AtomicBool::new(false);

pub fn is_muted() -> bool {
    MUTED.load(Ordering::Relaxed)
}

pub fn set_muted(muted: bool) {
    MUTED.store(muted, Ordering::Relaxed);
    if muted {
        stop();
    }
}

pub fn play_freq(hz: u32) {
    if hz < 20 || is_muted() {
        stop();
        return;
    }
    let divisor = (PIT_BASE_HZ / hz).clamp(1, 65535) as u16;
    unsafe {
        // PIT Channel 2, lobyte/hibyte, square wave mode 3
        Port::<u8>::new(0x43).write(0xB6);
        Port::<u8>::new(0x42).write((divisor & 0xFF) as u8);
        Port::<u8>::new(0x42).write((divisor >> 8) as u8);

        let mut ppi = Port::<u8>::new(0x61);
        let prev = ppi.read();
        if (prev & 0x03) != 0x03 {
            ppi.write(prev | 0x03);
        }
    }
}

pub fn stop() {
    unsafe {
        let mut ppi = Port::<u8>::new(0x61);
        let prev = ppi.read();
        if (prev & 0x03) != 0 {
            ppi.write(prev & !0x03);
        }
    }
}
