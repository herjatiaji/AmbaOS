use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::instructions::port::Port;

const PIT_FREQUENCY_HZ: u32 = 1_193_182;
pub const TARGET_HZ: u32 = 1_000;

static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let divisor: u16 = (PIT_FREQUENCY_HZ / TARGET_HZ) as u16;
    unsafe {
        let mut cmd = Port::<u8>::new(0x43);
        let mut ch0 = Port::<u8>::new(0x40);
        // Channel 0, lobyte/hibyte, mode 3 (square wave), binary
        cmd.write(0x36);
        ch0.write((divisor & 0xFF) as u8);
        ch0.write(((divisor >> 8) & 0xFF) as u8);
    }
}

#[inline]
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

#[inline]
pub fn ticks_ms() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

#[inline]
pub fn uptime_secs() -> u64 {
    ticks_ms() / (TARGET_HZ as u64)
}
