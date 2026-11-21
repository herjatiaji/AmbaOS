use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{Ordering, compiler_fence};

use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

use crate::drivers::pci;
use crate::mm::{pmm, vmm};

const INTEL_VENDOR: u16 = 0x8086;
const E1000_DEV_82540EM: u16 = 0x100E;

const REG_CTRL: usize = 0x0000;
const REG_STATUS: usize = 0x0008;
const REG_EERD: usize = 0x0014;
const REG_IMC: usize = 0x00D8;
const REG_RCTL: usize = 0x0100;
const REG_TCTL: usize = 0x0400;
const REG_TIPG: usize = 0x0410;
const REG_RDBAL: usize = 0x2800;
const REG_RDBAH: usize = 0x2804;
const REG_RDLEN: usize = 0x2808;
const REG_RDH: usize = 0x2810;
const REG_RDT: usize = 0x2818;
const REG_TDBAL: usize = 0x3800;
const REG_TDBAH: usize = 0x3804;
const REG_TDLEN: usize = 0x3808;
const REG_TDH: usize = 0x3810;
const REG_TDT: usize = 0x3818;
const REG_MTA: usize = 0x5200;
const REG_RAL: usize = 0x5400;
const REG_RAH: usize = 0x5404;

const CTRL_ASDE: u32 = 1 << 5;
const CTRL_SLU: u32 = 1 << 6;
const CTRL_RST: u32 = 1 << 26;

const RCTL_EN: u32 = 1 << 1;
const RCTL_UPE: u32 = 1 << 3;
const RCTL_MPE: u32 = 1 << 4;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;

const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;

const RX_DESC_COUNT: usize = 256;
const TX_DESC_COUNT: usize = 64;
const PKT_BUF_SIZE: usize = 2048;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct RxDesc {
    addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

pub struct E1000Device {
    mmio: *mut u8,
    mac: [u8; 6],
    rx_descs: *mut RxDesc,
    rx_bufs_virt: *mut u8,
    rx_cur: usize,
    tx_descs: *mut TxDesc,
    tx_bufs_virt: *mut u8,
    tx_cur: usize,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

unsafe impl Send for E1000Device {}

impl E1000Device {
    #[inline]
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        unsafe { core::ptr::read_volatile(self.mmio.add(offset) as *const u32) }
    }

    #[inline]
    unsafe fn write_reg(&self, offset: usize, val: u32) {
        unsafe { core::ptr::write_volatile(self.mmio.add(offset) as *mut u32, val) }
    }

    unsafe fn read_eeprom(&self, addr: u8) -> Option<u16> {
        unsafe {
            self.write_reg(REG_EERD, 1 | ((addr as u32) << 8));
            for _ in 0..50_000 {
                let v = self.read_reg(REG_EERD);
                if (v & (1 << 4)) != 0 {
                    return Some(((v >> 16) & 0xFFFF) as u16);
                }
                core::hint::spin_loop();
            }
            None
        }
    }

    pub fn init() -> Option<Self> {
        let dev = pci::find_device(INTEL_VENDOR, E1000_DEV_82540EM)?;
        if dev.bar0 == 0 {
            return None;
        }
        dev.enable_bus_mastering();

        let mmio = vmm::map_mmio_region(dev.bar0, 0x20000);
        let mut nic = Self {
            mmio,
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            rx_descs: core::ptr::null_mut(),
            rx_bufs_virt: core::ptr::null_mut(),
            rx_cur: 0,
            tx_descs: core::ptr::null_mut(),
            tx_bufs_virt: core::ptr::null_mut(),
            tx_cur: 0,
            rx_packets: 0,
            tx_packets: 0,
        };

        unsafe {
            // Read MAC before reset as fallback
            let ral0 = nic.read_reg(REG_RAL);
            let rah0 = nic.read_reg(REG_RAH);

            // Disable interrupts & reset NIC
            nic.write_reg(REG_IMC, 0xFFFF_FFFF);
            let ctrl = nic.read_reg(REG_CTRL);
            nic.write_reg(REG_CTRL, ctrl | CTRL_RST);
            for _ in 0..100_000 {
                if (nic.read_reg(REG_CTRL) & CTRL_RST) == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
            nic.write_reg(REG_IMC, 0xFFFF_FFFF);

            // Set link up
            let ctrl2 = nic.read_reg(REG_CTRL);
            nic.write_reg(REG_CTRL, (ctrl2 | CTRL_SLU | CTRL_ASDE) & !(1 << 31));

            // Read MAC from EEPROM or pre-reset RAL/RAH
            if let (Some(w0), Some(w1), Some(w2)) = (nic.read_eeprom(0), nic.read_eeprom(1), nic.read_eeprom(2)) {
                nic.mac = [
                    (w0 & 0xFF) as u8,
                    (w0 >> 8) as u8,
                    (w1 & 0xFF) as u8,
                    (w1 >> 8) as u8,
                    (w2 & 0xFF) as u8,
                    (w2 >> 8) as u8,
                ];
            } else if ral0 != 0 || rah0 != 0 {
                nic.mac = [
                    (ral0 & 0xFF) as u8,
                    ((ral0 >> 8) & 0xFF) as u8,
                    ((ral0 >> 16) & 0xFF) as u8,
                    ((ral0 >> 24) & 0xFF) as u8,
                    (rah0 & 0xFF) as u8,
                    ((rah0 >> 8) & 0xFF) as u8,
                ];
            }

            // Program MAC filter
            let ral = (nic.mac[0] as u32)
                | ((nic.mac[1] as u32) << 8)
                | ((nic.mac[2] as u32) << 16)
                | ((nic.mac[3] as u32) << 24);
            let rah = (nic.mac[4] as u32) | ((nic.mac[5] as u32) << 8) | (1 << 31);
            nic.write_reg(REG_RAL, ral);
            nic.write_reg(REG_RAH, rah);

            // Clear multicast table array
            for i in 0..128 {
                nic.write_reg(REG_MTA + i * 4, 0);
            }

            // Allocate RX ring & buffers (1 page descriptors + 16 pages buffers)
            let rx_desc_phys = pmm::alloc_contiguous(1)?;
            let rx_desc_virt = vmm::phys_to_virt(rx_desc_phys) as *mut RxDesc;
            core::ptr::write_bytes(rx_desc_virt, 0, pmm::PAGE_SIZE);

            let rx_buf_pages = (RX_DESC_COUNT * PKT_BUF_SIZE) / pmm::PAGE_SIZE;
            let rx_buf_phys = pmm::alloc_contiguous(rx_buf_pages)?;
            let rx_buf_virt = vmm::phys_to_virt(rx_buf_phys);
            core::ptr::write_bytes(rx_buf_virt, 0, rx_buf_pages * pmm::PAGE_SIZE);

            for i in 0..RX_DESC_COUNT {
                let desc = &mut *rx_desc_virt.add(i);
                desc.addr = rx_buf_phys + (i * PKT_BUF_SIZE) as u64;
                desc.status = 0;
            }
            nic.rx_descs = rx_desc_virt;
            nic.rx_bufs_virt = rx_buf_virt;

            nic.write_reg(REG_RDBAL, (rx_desc_phys & 0xFFFF_FFFF) as u32);
            nic.write_reg(REG_RDBAH, (rx_desc_phys >> 32) as u32);
            nic.write_reg(REG_RDLEN, (RX_DESC_COUNT * core::mem::size_of::<RxDesc>()) as u32);
            nic.write_reg(REG_RDH, 0);
            nic.write_reg(REG_RDT, (RX_DESC_COUNT - 1) as u32);
            nic.write_reg(REG_RCTL, RCTL_EN | RCTL_UPE | RCTL_MPE | RCTL_BAM | RCTL_SECRC);

            // Allocate TX ring & buffers
            let tx_desc_phys = pmm::alloc_contiguous(1)?;
            let tx_desc_virt = vmm::phys_to_virt(tx_desc_phys) as *mut TxDesc;
            core::ptr::write_bytes(tx_desc_virt, 0, pmm::PAGE_SIZE);

            let tx_buf_pages = (TX_DESC_COUNT * PKT_BUF_SIZE) / pmm::PAGE_SIZE;
            let tx_buf_phys = pmm::alloc_contiguous(tx_buf_pages)?;
            let tx_buf_virt = vmm::phys_to_virt(tx_buf_phys);
            core::ptr::write_bytes(tx_buf_virt, 0, tx_buf_pages * pmm::PAGE_SIZE);

            for i in 0..TX_DESC_COUNT {
                let desc = &mut *tx_desc_virt.add(i);
                desc.addr = tx_buf_phys + (i * PKT_BUF_SIZE) as u64;
                desc.status = 1; // TSTA_DD
            }
            nic.tx_descs = tx_desc_virt;
            nic.tx_bufs_virt = tx_buf_virt;

            nic.write_reg(REG_TDBAL, (tx_desc_phys & 0xFFFF_FFFF) as u32);
            nic.write_reg(REG_TDBAH, (tx_desc_phys >> 32) as u32);
            nic.write_reg(REG_TDLEN, (TX_DESC_COUNT * core::mem::size_of::<TxDesc>()) as u32);
            nic.write_reg(REG_TDH, 0);
            nic.write_reg(REG_TDT, 0);
            nic.write_reg(REG_TIPG, 10 | (8 << 10) | (6 << 20));
            nic.write_reg(REG_TCTL, TCTL_EN | TCTL_PSP | (0x10 << 4) | (0x40 << 12));
        }

        Some(nic)
    }

    #[inline]
    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    #[inline]
    pub fn link_up(&self) -> bool {
        unsafe { (self.read_reg(REG_STATUS) & (1 << 1)) != 0 }
    }

    pub fn recv_packet(&mut self) -> Option<Vec<u8>> {
        unsafe {
            let desc_ptr = self.rx_descs.add(self.rx_cur);
            compiler_fence(Ordering::Acquire);
            let status = core::ptr::read_volatile(&raw const (*desc_ptr).status);
            if (status & 0x01) == 0 {
                return None;
            }
            let len = (core::ptr::read_volatile(&raw const (*desc_ptr).length) as usize).min(PKT_BUF_SIZE);
            let src = self.rx_bufs_virt.add(self.rx_cur * PKT_BUF_SIZE);
            let mut pkt = vec![0u8; len];
            if len > 0 {
                core::ptr::copy_nonoverlapping(src, pkt.as_mut_ptr(), len);
            }

            core::ptr::write_volatile(&raw mut (*desc_ptr).status, 0);
            compiler_fence(Ordering::Release);

            let old = self.rx_cur;
            self.rx_cur = (self.rx_cur + 1) % RX_DESC_COUNT;
            self.write_reg(REG_RDT, old as u32);
            self.rx_packets += 1;
            Some(pkt)
        }
    }

    pub fn send_packet(&mut self, data: &[u8]) {
        if data.is_empty() || data.len() > PKT_BUF_SIZE {
            return;
        }
        unsafe {
            let desc_ptr = self.tx_descs.add(self.tx_cur);
            for _ in 0..100_000 {
                compiler_fence(Ordering::Acquire);
                let status = core::ptr::read_volatile(&raw const (*desc_ptr).status);
                if (status & 0x01) != 0 {
                    break;
                }
                core::hint::spin_loop();
            }

            let dst = self.tx_bufs_virt.add(self.tx_cur * PKT_BUF_SIZE);
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());

            core::ptr::write_volatile(&raw mut (*desc_ptr).length, data.len() as u16);
            core::ptr::write_volatile(&raw mut (*desc_ptr).cmd, 0x0B); // EOP | IFCS | RS
            core::ptr::write_volatile(&raw mut (*desc_ptr).status, 0);
            compiler_fence(Ordering::SeqCst);

            self.tx_cur = (self.tx_cur + 1) % TX_DESC_COUNT;
            self.write_reg(REG_TDT, self.tx_cur as u32);
            self.tx_packets += 1;
        }
    }
}

pub struct E1000RxToken(Vec<u8>);

impl RxToken for E1000RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}

pub struct E1000TxToken<'a>(&'a mut E1000Device);

impl<'a> TxToken for E1000TxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let res = f(&mut buf);
        self.0.send_packet(&buf);
        res
    }
}

impl Device for E1000Device {
    type RxToken<'a> = E1000RxToken where Self: 'a;
    type TxToken<'a> = E1000TxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let pkt = self.recv_packet()?;
        Some((E1000RxToken(pkt), E1000TxToken(self)))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(E1000TxToken(self))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}
