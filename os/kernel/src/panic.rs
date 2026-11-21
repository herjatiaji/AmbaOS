use core::fmt::Write;
use core::panic::PanicInfo;
use spin::Mutex;
use x86_64::instructions::{hlt, interrupts};

use crate::drivers::framebuffer::FramebufferInfo;
use crate::drivers::serial::SERIAL1;

static PANIC_FB: Mutex<Option<FramebufferInfo>> = Mutex::new(None);

pub fn set_framebuffer(fb: FramebufferInfo) {
    *PANIC_FB.lock() = Some(fb);
}

pub fn halt_forever() -> ! {
    interrupts::disable();
    loop {
        hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    interrupts::disable();

    unsafe {
        SERIAL1.force_unlock();
    }
    {
        let mut serial = SERIAL1.lock();
        let _ = writeln!(serial, "\n=== AMBAOS KERNEL PANIC ===");
        let _ = writeln!(serial, "{}", info);
    }

    let fb_opt = PANIC_FB.try_lock().and_then(|g| *g);
    if let Some(fb) = fb_opt {
        fb.fill_solid(0x18090C);
        fb.draw_banner_text(48, 48, "AmbaOS KERNEL PANIC", 0xFF5F56, 3);
        fb.draw_banner_text(48, 100, "Fatal error in Ring 0. CPU halted cleanly.", 0xF1F5F9, 2);
        fb.draw_banner_text(
            48,
            126,
            "Restart to open AmbaOS Atomic Rollback Manager and restore a healthy snapshot.",
            0x4ADE80,
            1,
        );

        let mut buf = StackFmt::<512>::new();
        let _ = write!(buf, "{}", info);
        fb.draw_banner_text(48, 150, buf.as_str(), 0xCBD5E1, 1);
    }

    halt_forever();
}

struct StackFmt<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> StackFmt<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Write for StackFmt<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if self.len >= N {
                break;
            }
            self.buf[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}
