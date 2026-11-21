use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

const ATA_DATA: u16 = 0x1F0;
const ATA_ERROR: u16 = 0x1F1;
const ATA_SECCNT: u16 = 0x1F2;
const ATA_LBA0: u16 = 0x1F3;
const ATA_LBA1: u16 = 0x1F4;
const ATA_LBA2: u16 = 0x1F5;
const ATA_DRIVE: u16 = 0x1F6;
const ATA_STATUS: u16 = 0x1F7;
const ATA_CMD: u16 = 0x1F7;
const ATA_ALT_STATUS: u16 = 0x3F6;
const ATA_DEV_CTRL: u16 = 0x3F6;

const SR_ERR: u8 = 0x01;
const SR_DRQ: u8 = 0x08;
const SR_BSY: u8 = 0x80;

const CMD_READ_PIO: u8 = 0x20;
const CMD_WRITE_PIO: u8 = 0x30;
const CMD_CACHE_FLUSH: u8 = 0xE7;
const CMD_IDENTIFY: u8 = 0xEC;

static ATA_LOCK: Mutex<()> = Mutex::new(());
static DISK_PRESENT: AtomicBool = AtomicBool::new(false);
static DISK_SECTORS: AtomicU32 = AtomicU32::new(0);

#[inline]
unsafe fn io_wait() {
    unsafe {
        let mut alt = Port::<u8>::new(ATA_ALT_STATUS);
        for _ in 0..4 {
            let _ = alt.read();
        }
    }
}

unsafe fn wait_not_busy() -> Option<u8> {
    unsafe {
        let mut st_port = Port::<u8>::new(ATA_STATUS);
        for _ in 0..200_000 {
            let st = st_port.read();
            if (st & SR_BSY) == 0 {
                return Some(st);
            }
            core::hint::spin_loop();
        }
        None
    }
}

unsafe fn wait_drq() -> bool {
    unsafe {
        let mut st_port = Port::<u8>::new(ATA_STATUS);
        for _ in 0..200_000 {
            let st = st_port.read();
            if (st & SR_ERR) != 0 {
                return false;
            }
            if (st & SR_BSY) == 0 && (st & SR_DRQ) != 0 {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }
}

pub fn init() -> Option<u32> {
    let _guard = ATA_LOCK.lock();
    unsafe {
        // Disable IDE IRQs (nIEN) on Primary controller
        Port::<u8>::new(ATA_DEV_CTRL).write(0x02);
        // Select Primary Master
        Port::<u8>::new(ATA_DRIVE).write(0xA0);
        io_wait();

        Port::<u8>::new(ATA_SECCNT).write(0);
        Port::<u8>::new(ATA_LBA0).write(0);
        Port::<u8>::new(ATA_LBA1).write(0);
        Port::<u8>::new(ATA_LBA2).write(0);
        Port::<u8>::new(ATA_CMD).write(CMD_IDENTIFY);
        io_wait();

        let status = Port::<u8>::new(ATA_STATUS).read();
        if status == 0 || status == 0xFF {
            return None;
        }

        let st = wait_not_busy()?;
        if (st & SR_ERR) != 0 {
            return None;
        }
        // Check LBA1/LBA2 == 0 so we don't mistake ATAPI/SATA bridge for ATA HD
        let lba1 = Port::<u8>::new(ATA_LBA1).read();
        let lba2 = Port::<u8>::new(ATA_LBA2).read();
        if lba1 != 0 || lba2 != 0 {
            return None;
        }
        if !wait_drq() {
            return None;
        }

        let mut ident = [0u16; 256];
        let mut data_port = Port::<u16>::new(ATA_DATA);
        for w in ident.iter_mut() {
            *w = data_port.read();
        }

        let lba28 = (ident[60] as u32) | ((ident[61] as u32) << 16);
        if lba28 == 0 {
            return None;
        }

        DISK_SECTORS.store(lba28, Ordering::Relaxed);
        DISK_PRESENT.store(true, Ordering::Relaxed);
        Some(lba28)
    }
}

pub fn is_present() -> bool {
    DISK_PRESENT.load(Ordering::Relaxed)
}

pub fn total_sectors() -> u32 {
    DISK_SECTORS.load(Ordering::Relaxed)
}

pub fn read_sector(lba: u32, buf: &mut [u8; 512]) -> bool {
    if !is_present() || lba >= total_sectors() {
        return false;
    }
    let _guard = ATA_LOCK.lock();
    unsafe {
        Port::<u8>::new(ATA_DRIVE).write(0xE0 | (((lba >> 24) & 0x0F) as u8));
        io_wait();
        let _ = Port::<u8>::new(ATA_ERROR).read();
        Port::<u8>::new(ATA_SECCNT).write(1);
        Port::<u8>::new(ATA_LBA0).write((lba & 0xFF) as u8);
        Port::<u8>::new(ATA_LBA1).write(((lba >> 8) & 0xFF) as u8);
        Port::<u8>::new(ATA_LBA2).write(((lba >> 16) & 0xFF) as u8);
        Port::<u8>::new(ATA_CMD).write(CMD_READ_PIO);
        io_wait();

        if !wait_drq() {
            return false;
        }

        let mut data_port = Port::<u16>::new(ATA_DATA);
        for i in 0..256 {
            let w = data_port.read();
            let b = w.to_le_bytes();
            buf[i * 2] = b[0];
            buf[i * 2 + 1] = b[1];
        }
        true
    }
}

pub fn write_sector(lba: u32, buf: &[u8; 512]) -> bool {
    if !is_present() || lba >= total_sectors() {
        return false;
    }
    let _guard = ATA_LOCK.lock();
    unsafe {
        Port::<u8>::new(ATA_DRIVE).write(0xE0 | (((lba >> 24) & 0x0F) as u8));
        io_wait();
        Port::<u8>::new(ATA_SECCNT).write(1);
        Port::<u8>::new(ATA_LBA0).write((lba & 0xFF) as u8);
        Port::<u8>::new(ATA_LBA1).write(((lba >> 8) & 0xFF) as u8);
        Port::<u8>::new(ATA_LBA2).write(((lba >> 16) & 0xFF) as u8);
        Port::<u8>::new(ATA_CMD).write(CMD_WRITE_PIO);
        io_wait();

        if !wait_drq() {
            return false;
        }

        let mut data_port = Port::<u16>::new(ATA_DATA);
        for i in 0..256 {
            let w = u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]);
            data_port.write(w);
        }
        let _ = wait_not_busy();
        true
    }
}

pub fn flush() {
    if !is_present() {
        return;
    }
    let _guard = ATA_LOCK.lock();
    unsafe {
        Port::<u8>::new(ATA_DRIVE).write(0xE0);
        io_wait();
        Port::<u8>::new(ATA_CMD).write(CMD_CACHE_FLUSH);
        io_wait();
        let _ = wait_not_busy();
    }
}
