use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub left_pressed: bool,
    pub left_released: bool,
    #[allow(dead_code)]
    pub right: bool,
    #[allow(dead_code)]
    pub middle: bool,
    pub wheel: i32,
    pub dirty: bool,
}

static MAX_W: AtomicI32 = AtomicI32::new(1280);
static MAX_H: AtomicI32 = AtomicI32::new(800);
static MOUSE_X: AtomicI32 = AtomicI32::new(640);
static MOUSE_Y: AtomicI32 = AtomicI32::new(400);
static MOUSE_BUTTONS: AtomicU8 = AtomicU8::new(0);
static MOUSE_DIRTY: AtomicBool = AtomicBool::new(false);
static PENDING_LEFT_PRESS: AtomicBool = AtomicBool::new(false);
static PENDING_LEFT_RELEASE: AtomicBool = AtomicBool::new(false);
static PENDING_WHEEL: AtomicI32 = AtomicI32::new(0);
static VMMOUSE_ENABLED: AtomicBool = AtomicBool::new(false);

static PACKET_CYCLE: AtomicU8 = AtomicU8::new(0);
static PACKET_B0: AtomicU8 = AtomicU8::new(0);
static PACKET_B1: AtomicU8 = AtomicU8::new(0);

const VMMOUSE_MAGIC: u32 = 0x564D_5868;
const VMMOUSE_PORT: u16 = 0x5658;
const CMD_GETVERSION: u32 = 10;
const CMD_ABSPOINTER_DATA: u32 = 39;
const CMD_ABSPOINTER_STATUS: u32 = 40;
const CMD_ABSPOINTER_COMMAND: u32 = 41;
const VMMOUSE_CMD_READ_ID: u32 = 0x4541_4552;
const VMMOUSE_CMD_DISABLE: u32 = 0x0000_00F5;
const VMMOUSE_CMD_REQUEST_ABSOLUTE: u32 = 0x5342_4152;
const VMMOUSE_VERSION_ID: u32 = 0x3442_554A;

#[inline]
unsafe fn vmport_cmd(cmd: u32, param: u32) -> (u32, u32, u32, u32) {
    let mut eax: u32 = VMMOUSE_MAGIC;
    let mut b64: u64 = param as u64;
    let mut ecx: u32 = cmd;
    let mut edx: u32 = VMMOUSE_PORT as u32;
    unsafe {
        asm!(
            "xchg rbx, {b}",
            "in eax, dx",
            "xchg rbx, {b}",
            inout("eax") eax,
            b = inout(reg) b64,
            inout("ecx") ecx,
            inout("edx") edx,
            options(nomem, nostack, preserves_flags),
        );
    }
    (eax, b64 as u32, ecx, edx)
}

unsafe fn try_init_vmmouse() -> bool {
    unsafe {
        let (_, magic, _, _) = vmport_cmd(CMD_GETVERSION, 0);
        if magic != VMMOUSE_MAGIC {
            return false;
        }
        let _ = vmport_cmd(CMD_ABSPOINTER_COMMAND, VMMOUSE_CMD_READ_ID);
        let (st, _, _, _) = vmport_cmd(CMD_ABSPOINTER_STATUS, 0);
        if (st & 0xFFFF) == 0 {
            return false;
        }
        let (ver, _, _, _) = vmport_cmd(CMD_ABSPOINTER_DATA, 1);
        if ver != VMMOUSE_VERSION_ID {
            return false;
        }
        let _ = vmport_cmd(CMD_ABSPOINTER_COMMAND, VMMOUSE_CMD_DISABLE);
        let _ = vmport_cmd(CMD_ABSPOINTER_COMMAND, VMMOUSE_CMD_READ_ID);
        let _ = vmport_cmd(CMD_ABSPOINTER_DATA, 1);
        let _ = vmport_cmd(CMD_ABSPOINTER_COMMAND, VMMOUSE_CMD_REQUEST_ABSOLUTE);
        VMMOUSE_ENABLED.store(true, Ordering::Relaxed);
        true
    }
}

unsafe fn drain_vmmouse() {
    let max_x = (MAX_W.load(Ordering::Relaxed) - 1).max(1) as u64;
    let max_y = (MAX_H.load(Ordering::Relaxed) - 1).max(1) as u64;

    unsafe {
        for _ in 0..16 {
            let (status, _, _, _) = vmport_cmd(CMD_ABSPOINTER_STATUS, 0);
            if (status >> 16) == 0xFFFF {
                let _ = vmport_cmd(CMD_ABSPOINTER_COMMAND, VMMOUSE_CMD_REQUEST_ABSOLUTE);
                break;
            }
            if (status & 0xFFFF) < 4 {
                break;
            }
            let (btn_word, x_raw, y_raw, z_raw) = vmport_cmd(CMD_ABSPOINTER_DATA, 4);

            let mut new_btn: u8 = 0;
            if (btn_word & 0x20) != 0 {
                new_btn |= 0x01;
            }
            if (btn_word & 0x10) != 0 {
                new_btn |= 0x02;
            }
            if (btn_word & 0x08) != 0 {
                new_btn |= 0x04;
            }

            let old_btn = MOUSE_BUTTONS.swap(new_btn, Ordering::Relaxed);
            if (old_btn & 0x01) == 0 && (new_btn & 0x01) != 0 {
                PENDING_LEFT_PRESS.store(true, Ordering::Relaxed);
            } else if (old_btn & 0x01) != 0 && (new_btn & 0x01) == 0 {
                PENDING_LEFT_RELEASE.store(true, Ordering::Relaxed);
            }

            let nx = (((x_raw as u64 & 0xFFFF) * max_x + 32767) / 65535) as i32;
            let ny = (((y_raw as u64 & 0xFFFF) * max_y + 32767) / 65535) as i32;

            let dz = (z_raw as i8) as i32;
            if dz != 0 {
                PENDING_WHEEL.fetch_add(dz, Ordering::Relaxed);
            }

            MOUSE_X.store(nx, Ordering::Relaxed);
            MOUSE_Y.store(ny, Ordering::Relaxed);
            MOUSE_DIRTY.store(true, Ordering::Relaxed);
        }
    }
}

#[inline]
unsafe fn wait_write() {
    let mut status = Port::<u8>::new(0x64);
    for _ in 0..100_000 {
        if unsafe { status.read() } & 0x02 == 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

#[inline]
unsafe fn wait_read() {
    let mut status = Port::<u8>::new(0x64);
    for _ in 0..100_000 {
        if unsafe { status.read() } & 0x01 != 0 {
            return;
        }
        core::hint::spin_loop();
    }
}

unsafe fn mouse_write(byte: u8) {
    unsafe {
        wait_write();
        Port::<u8>::new(0x64).write(0xD4);
        wait_write();
        Port::<u8>::new(0x60).write(byte);
    }
}

unsafe fn mouse_read() -> u8 {
    unsafe {
        wait_read();
        Port::<u8>::new(0x60).read()
    }
}

unsafe fn flush_output() {
    let mut status = Port::<u8>::new(0x64);
    let mut data = Port::<u8>::new(0x60);
    for _ in 0..32 {
        if unsafe { status.read() } & 0x01 == 0 {
            break;
        }
        let _ = unsafe { data.read() };
    }
}

pub fn init(width: i32, height: i32) -> bool {
    let w = width.max(1);
    let h = height.max(1);
    MAX_W.store(w, Ordering::Relaxed);
    MAX_H.store(h, Ordering::Relaxed);
    MOUSE_X.store(w / 2, Ordering::Relaxed);
    MOUSE_Y.store(h / 2, Ordering::Relaxed);

    unsafe {
        flush_output();

        // Enable auxiliary mouse device
        wait_write();
        Port::<u8>::new(0x64).write(0xA8);

        // Read controller command byte
        wait_write();
        Port::<u8>::new(0x64).write(0x20);
        wait_read();
        let mut status_byte = Port::<u8>::new(0x60).read();

        // Enable IRQ12 (bit 1) and clear disable-mouse-clock (bit 5)
        status_byte |= 0x02;
        status_byte &= !0x20;

        wait_write();
        Port::<u8>::new(0x64).write(0x60);
        wait_write();
        Port::<u8>::new(0x60).write(status_byte);

        // Set default settings
        mouse_write(0xF6);
        let _ = mouse_read();

        // Enable data reporting
        mouse_write(0xF4);
        let _ = mouse_read();

        flush_output();
        try_init_vmmouse()
    }
}

pub fn on_irq() {
    if VMMOUSE_ENABLED.load(Ordering::Relaxed) {
        unsafe {
            let mut status_port = Port::<u8>::new(0x64);
            let mut data_port = Port::<u8>::new(0x60);
            for _ in 0..16 {
                if (status_port.read() & 0x21) != 0x21 {
                    break;
                }
                let _ = data_port.read();
            }
            drain_vmmouse();
        }
        return;
    }

    let status = unsafe { Port::<u8>::new(0x64).read() };
    if (status & 0x21) != 0x21 {
        return;
    }
    let byte = unsafe { Port::<u8>::new(0x60).read() };

    let cycle = PACKET_CYCLE.load(Ordering::Relaxed);
    match cycle {
        0 => {
            // Bit 3 must always be 1 in first PS/2 mouse packet byte
            if byte & 0x08 == 0 {
                return;
            }
            PACKET_B0.store(byte, Ordering::Relaxed);
            PACKET_CYCLE.store(1, Ordering::Relaxed);
        }
        1 => {
            PACKET_B1.store(byte, Ordering::Relaxed);
            PACKET_CYCLE.store(2, Ordering::Relaxed);
        }
        _ => {
            PACKET_CYCLE.store(0, Ordering::Relaxed);
            let b0 = PACKET_B0.load(Ordering::Relaxed);
            let b1 = PACKET_B1.load(Ordering::Relaxed);
            let b2 = byte;

            // Ignore overflow packets
            if (b0 & 0xC0) != 0 {
                return;
            }

            let dx = (b1 as i8) as i32;
            let dy = -((b2 as i8) as i32);

            let max_x = MAX_W.load(Ordering::Relaxed) - 1;
            let max_y = MAX_H.load(Ordering::Relaxed) - 1;

            let nx = (MOUSE_X.load(Ordering::Relaxed) + dx).clamp(0, max_x);
            let ny = (MOUSE_Y.load(Ordering::Relaxed) + dy).clamp(0, max_y);

            let new_btn = b0 & 0x07;
            let old_btn = MOUSE_BUTTONS.swap(new_btn, Ordering::Relaxed);
            if (old_btn & 0x01) == 0 && (new_btn & 0x01) != 0 {
                PENDING_LEFT_PRESS.store(true, Ordering::Relaxed);
            } else if (old_btn & 0x01) != 0 && (new_btn & 0x01) == 0 {
                PENDING_LEFT_RELEASE.store(true, Ordering::Relaxed);
            }

            MOUSE_X.store(nx, Ordering::Relaxed);
            MOUSE_Y.store(ny, Ordering::Relaxed);
            MOUSE_DIRTY.store(true, Ordering::Relaxed);
        }
    }
}

pub fn poll() {
    if VMMOUSE_ENABLED.load(Ordering::Relaxed) {
        x86_64::instructions::interrupts::without_interrupts(|| unsafe {
            drain_vmmouse();
        });
    }
}

pub fn take_state() -> MouseState {
    let dirty = MOUSE_DIRTY.swap(false, Ordering::Relaxed);
    let buttons = MOUSE_BUTTONS.load(Ordering::Relaxed);
    MouseState {
        x: MOUSE_X.load(Ordering::Relaxed),
        y: MOUSE_Y.load(Ordering::Relaxed),
        left: (buttons & 0x01) != 0,
        left_pressed: PENDING_LEFT_PRESS.swap(false, Ordering::Relaxed),
        left_released: PENDING_LEFT_RELEASE.swap(false, Ordering::Relaxed),
        right: (buttons & 0x02) != 0,
        middle: (buttons & 0x04) != 0,
        wheel: PENDING_WHEEL.swap(0, Ordering::Relaxed),
        dirty,
    }
}

pub fn peek_pos() -> (i32, i32) {
    (
        MOUSE_X.load(Ordering::Relaxed),
        MOUSE_Y.load(Ordering::Relaxed),
    )
}
