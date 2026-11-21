use x86_64::instructions::port::Port;

#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

unsafe fn cmos_read(reg: u8) -> u8 {
    unsafe {
        Port::<u8>::new(0x70).write(reg);
        Port::<u8>::new(0x71).read()
    }
}

fn is_updating() -> bool {
    unsafe { cmos_read(0x0A) & 0x80 != 0 }
}

fn bcd_to_bin(val: u8) -> u8 {
    (val & 0x0F) + ((val / 16) * 10)
}

pub fn read_rtc() -> DateTime {
    for _ in 0..10_000 {
        if !is_updating() {
            break;
        }
        core::hint::spin_loop();
    }

    let mut sec = unsafe { cmos_read(0x00) };
    let mut min = unsafe { cmos_read(0x02) };
    let mut hour = unsafe { cmos_read(0x04) };
    let mut day = unsafe { cmos_read(0x07) };
    let mut month = unsafe { cmos_read(0x08) };
    let mut year = unsafe { cmos_read(0x09) };
    let reg_b = unsafe { cmos_read(0x0B) };

    if (reg_b & 0x04) == 0 {
        sec = bcd_to_bin(sec);
        min = bcd_to_bin(min);
        hour = bcd_to_bin(hour & 0x7F) | (hour & 0x80);
        day = bcd_to_bin(day);
        month = bcd_to_bin(month);
        year = bcd_to_bin(year);
    }

    if (reg_b & 0x02) == 0 && (hour & 0x80) != 0 {
        hour = ((hour & 0x7F) + 12) % 24;
    }

    DateTime {
        year: 2000 + year as u16,
        month,
        day,
        hour,
        minute: min,
        second: sec,
    }
}
