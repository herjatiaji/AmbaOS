use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

pub const KEY_W: u32 = 1 << 0;
pub const KEY_S: u32 = 1 << 1;
pub const KEY_A: u32 = 1 << 2;
pub const KEY_D: u32 = 1 << 3;
pub const KEY_Q: u32 = 1 << 4;
pub const KEY_E: u32 = 1 << 5;
pub const KEY_UP: u32 = 1 << 6;
pub const KEY_DOWN: u32 = 1 << 7;
pub const KEY_LEFT: u32 = 1 << 8;
pub const KEY_RIGHT: u32 = 1 << 9;
pub const KEY_SPACE: u32 = 1 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    Backspace,
    Enter,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
}

const QUEUE_CAP: usize = 128;

struct KeyQueue {
    buf: [Option<KeyEvent>; QUEUE_CAP],
    head: usize,
    tail: usize,
}

impl KeyQueue {
    const fn new() -> Self {
        Self {
            buf: [None; QUEUE_CAP],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, ev: KeyEvent) {
        let next = (self.tail + 1) % QUEUE_CAP;
        if next != self.head {
            self.buf[self.tail] = Some(ev);
            self.tail = next;
        }
    }

    fn pop(&mut self) -> Option<KeyEvent> {
        if self.head == self.tail {
            None
        } else {
            let ev = self.buf[self.head].take();
            self.head = (self.head + 1) % QUEUE_CAP;
            ev
        }
    }
}

static KEY_QUEUE: Mutex<KeyQueue> = Mutex::new(KeyQueue::new());
static HELD_KEYS: AtomicU32 = AtomicU32::new(0);
static LEFT_SHIFT: AtomicBool = AtomicBool::new(false);
static RIGHT_SHIFT: AtomicBool = AtomicBool::new(false);
static CAPS_LOCK: AtomicBool = AtomicBool::new(false);
static CTRL_DOWN: AtomicBool = AtomicBool::new(false);
static EXTENDED_PREFIX: AtomicBool = AtomicBool::new(false);

pub fn held_mask() -> u32 {
    HELD_KEYS.load(Ordering::Relaxed)
}

#[inline]
fn set_held(bit: u32, down: bool) {
    if down {
        HELD_KEYS.fetch_or(bit, Ordering::Relaxed);
    } else {
        HELD_KEYS.fetch_and(!bit, Ordering::Relaxed);
    }
}

pub fn on_irq() {
    let scancode = unsafe { Port::<u8>::new(0x60).read() };
    handle_scancode(scancode);
}

fn handle_scancode(sc: u8) {
    if sc == 0xE0 {
        EXTENDED_PREFIX.store(true, Ordering::Relaxed);
        return;
    }

    let is_ext = EXTENDED_PREFIX.swap(false, Ordering::Relaxed);
    if is_ext {
        match sc {
            0x48 => {
                set_held(KEY_UP, true);
                push_event(KeyEvent::Up);
            }
            0xC8 => set_held(KEY_UP, false),
            0x50 => {
                set_held(KEY_DOWN, true);
                push_event(KeyEvent::Down);
            }
            0xD0 => set_held(KEY_DOWN, false),
            0x4B => {
                set_held(KEY_LEFT, true);
                push_event(KeyEvent::Left);
            }
            0xCB => set_held(KEY_LEFT, false),
            0x4D => {
                set_held(KEY_RIGHT, true);
                push_event(KeyEvent::Right);
            }
            0xCD => set_held(KEY_RIGHT, false),
            _ => {}
        }
        return;
    }

    match sc {
        0x11 => set_held(KEY_W, true),
        0x91 => set_held(KEY_W, false),
        0x1F => set_held(KEY_S, true),
        0x9F => set_held(KEY_S, false),
        0x1E => set_held(KEY_A, true),
        0x9E => set_held(KEY_A, false),
        0x20 => set_held(KEY_D, true),
        0xA0 => set_held(KEY_D, false),
        0x10 => set_held(KEY_Q, true),
        0x90 => set_held(KEY_Q, false),
        0x12 => set_held(KEY_E, true),
        0x92 => set_held(KEY_E, false),
        0x39 => set_held(KEY_SPACE, true),
        0xB9 => set_held(KEY_SPACE, false),
        _ => {}
    }

    match sc {
        0x2A => LEFT_SHIFT.store(true, Ordering::Relaxed),
        0xAA => LEFT_SHIFT.store(false, Ordering::Relaxed),
        0x36 => RIGHT_SHIFT.store(true, Ordering::Relaxed),
        0xB6 => RIGHT_SHIFT.store(false, Ordering::Relaxed),
        0x1D => CTRL_DOWN.store(true, Ordering::Relaxed),
        0x9D => CTRL_DOWN.store(false, Ordering::Relaxed),
        0x3A => {
            let prev = CAPS_LOCK.load(Ordering::Relaxed);
            CAPS_LOCK.store(!prev, Ordering::Relaxed);
        }
        0x01 => push_event(KeyEvent::Escape),
        0x0E => push_event(KeyEvent::Backspace),
        0x0F => push_event(KeyEvent::Tab),
        0x1C => push_event(KeyEvent::Enter),
        code if code < 0x80 => {
            let shift = LEFT_SHIFT.load(Ordering::Relaxed) || RIGHT_SHIFT.load(Ordering::Relaxed);
            let caps = CAPS_LOCK.load(Ordering::Relaxed);
            if let Some(ch) = scancode_to_ascii(code, shift, caps) {
                push_event(KeyEvent::Char(ch));
            }
        }
        _ => {}
    }
}

fn push_event(ev: KeyEvent) {
    KEY_QUEUE.lock().push(ev);
}

pub fn pop_event() -> Option<KeyEvent> {
    interrupts::without_interrupts(|| KEY_QUEUE.lock().pop())
}

fn scancode_to_ascii(code: u8, shift: bool, caps: bool) -> Option<char> {
    let normal = match code {
        0x02 => ('1', '!'),
        0x03 => ('2', '@'),
        0x04 => ('3', '#'),
        0x05 => ('4', '$'),
        0x06 => ('5', '%'),
        0x07 => ('6', '^'),
        0x08 => ('7', '&'),
        0x09 => ('8', '*'),
        0x0A => ('9', '('),
        0x0B => ('0', ')'),
        0x0C => ('-', '_'),
        0x0D => ('=', '+'),
        0x10 => ('q', 'Q'),
        0x11 => ('w', 'W'),
        0x12 => ('e', 'E'),
        0x13 => ('r', 'R'),
        0x14 => ('t', 'T'),
        0x15 => ('y', 'Y'),
        0x16 => ('u', 'U'),
        0x17 => ('i', 'I'),
        0x18 => ('o', 'O'),
        0x19 => ('p', 'P'),
        0x1A => ('[', '{'),
        0x1B => (']', '}'),
        0x1E => ('a', 'A'),
        0x1F => ('s', 'S'),
        0x20 => ('d', 'D'),
        0x21 => ('f', 'F'),
        0x22 => ('g', 'G'),
        0x23 => ('h', 'H'),
        0x24 => ('j', 'J'),
        0x25 => ('k', 'K'),
        0x26 => ('l', 'L'),
        0x27 => (';', ':'),
        0x28 => ('\'', '"'),
        0x29 => ('`', '~'),
        0x2B => ('\\', '|'),
        0x2C => ('z', 'Z'),
        0x2D => ('x', 'X'),
        0x2E => ('c', 'C'),
        0x2F => ('v', 'V'),
        0x30 => ('b', 'B'),
        0x31 => ('n', 'N'),
        0x32 => ('m', 'M'),
        0x33 => (',', '<'),
        0x34 => ('.', '>'),
        0x35 => ('/', '?'),
        0x39 => (' ', ' '),
        _ => return None,
    };

    let is_alpha = normal.0.is_ascii_lowercase();
    let upper = if is_alpha { shift ^ caps } else { shift };
    Some(if upper { normal.1 } else { normal.0 })
}
