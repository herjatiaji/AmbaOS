use x86_64::instructions::port::Port;

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    #[allow(dead_code)]
    pub vendor_id: u16,
    #[allow(dead_code)]
    pub device_id: u16,
    #[allow(dead_code)]
    pub class_code: u8,
    #[allow(dead_code)]
    pub subclass: u8,
    pub bar0: u64,
}

#[inline]
fn config_addr(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
}

pub fn read_u32(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    unsafe {
        Port::<u32>::new(PCI_CONFIG_ADDR).write(config_addr(bus, dev, func, offset));
        Port::<u32>::new(PCI_CONFIG_DATA).read()
    }
}

pub fn write_u32(bus: u8, dev: u8, func: u8, offset: u8, val: u32) {
    unsafe {
        Port::<u32>::new(PCI_CONFIG_ADDR).write(config_addr(bus, dev, func, offset));
        Port::<u32>::new(PCI_CONFIG_DATA).write(val);
    }
}

pub fn find_device(target_vendor: u16, target_device: u16) -> Option<PciDevice> {
    for bus in 0..8u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                let id = read_u32(bus, dev, func, 0x00);
                let vendor = (id & 0xFFFF) as u16;
                if vendor == 0xFFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let device = ((id >> 16) & 0xFFFF) as u16;
                if vendor == target_vendor && device == target_device {
                    let class_reg = read_u32(bus, dev, func, 0x08);
                    let bar0_lo = read_u32(bus, dev, func, 0x10);
                    let mut bar0 = (bar0_lo & !0xF) as u64;
                    if (bar0_lo & 0x06) == 0x04 {
                        let bar1_hi = read_u32(bus, dev, func, 0x14);
                        bar0 |= (bar1_hi as u64) << 32;
                    }
                    return Some(PciDevice {
                        bus,
                        dev,
                        func,
                        vendor_id: vendor,
                        device_id: device,
                        class_code: ((class_reg >> 24) & 0xFF) as u8,
                        subclass: ((class_reg >> 16) & 0xFF) as u8,
                        bar0,
                    });
                }
                if func == 0 {
                    let header_type = ((read_u32(bus, dev, func, 0x0C) >> 16) & 0xFF) as u8;
                    if (header_type & 0x80) == 0 {
                        break;
                    }
                }
            }
        }
    }
    None
}

impl PciDevice {
    pub fn enable_bus_mastering(&self) {
        let cmd = read_u32(self.bus, self.dev, self.func, 0x04);
        // Enable I/O Space (bit 0), Memory Space (bit 1), Bus Master (bit 2)
        write_u32(self.bus, self.dev, self.func, 0x04, cmd | 0x07);
    }
}
