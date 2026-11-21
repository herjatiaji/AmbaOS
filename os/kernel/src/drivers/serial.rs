use core::fmt;
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

const COM1_BASE: u16 = 0x3F8;

pub struct SerialPort {
    base: u16,
    initialized: bool,
}

impl SerialPort {
    const fn new(base: u16) -> Self {
        Self {
            base,
            initialized: false,
        }
    }

    pub fn init(&mut self) {
        unsafe {
            let mut ier = Port::<u8>::new(self.base + 1);
            let mut lcr = Port::<u8>::new(self.base + 3);
            let mut dll = Port::<u8>::new(self.base);
            let mut dlm = Port::<u8>::new(self.base + 1);
            let mut fcr = Port::<u8>::new(self.base + 2);
            let mut mcr = Port::<u8>::new(self.base + 4);

            ier.write(0x00); // Disable interrupts
            lcr.write(0x80); // Enable DLAB
            dll.write(0x03); // 38400 baud (lo)
            dlm.write(0x00); // (hi)
            lcr.write(0x03); // 8N1
            fcr.write(0xC7); // Enable FIFO, clear, 14-byte threshold
            mcr.write(0x0B); // IRQs enabled, RTS/DSR set
        }
        self.initialized = true;
    }

    #[inline]
    fn is_transmit_empty(&mut self) -> bool {
        unsafe { Port::<u8>::new(self.base + 5).read() & 0x20 != 0 }
    }

    pub fn write_byte(&mut self, byte: u8) {
        if !self.initialized {
            self.init();
        }
        if byte == b'\n' {
            self.write_raw(b'\r');
        }
        self.write_raw(byte);
    }

    fn write_raw(&mut self, byte: u8) {
        for _ in 0..100_000 {
            if self.is_transmit_empty() {
                break;
            }
            core::hint::spin_loop();
        }
        unsafe {
            Port::<u8>::new(self.base).write(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            self.write_byte(b);
        }
        Ok(())
    }
}

pub static SERIAL1: Mutex<SerialPort> = Mutex::new(SerialPort::new(COM1_BASE));

pub fn init() {
    interrupts::without_interrupts(|| {
        SERIAL1.lock().init();
    });
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    interrupts::without_interrupts(|| {
        let _ = SERIAL1.lock().write_fmt(args);
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::drivers::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
