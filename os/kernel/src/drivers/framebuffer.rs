use core::slice;

#[derive(Clone, Copy)]
pub struct FramebufferInfo {
    pub addr: *mut u32,
    pub width: usize,
    pub height: usize,
    pub pitch_bytes: usize,
    pub bpp: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

unsafe impl Send for FramebufferInfo {}
unsafe impl Sync for FramebufferInfo {}

impl FramebufferInfo {
    pub fn from_limine(fb: &limine::framebuffer::Framebuffer<'_>) -> Self {
        Self {
            addr: fb.addr() as *mut u32,
            width: fb.width() as usize,
            height: fb.height() as usize,
            pitch_bytes: fb.pitch() as usize,
            bpp: fb.bpp(),
            red_shift: fb.red_mask_shift(),
            green_shift: fb.green_mask_shift(),
            blue_shift: fb.blue_mask_shift(),
        }
    }

    #[inline]
    pub fn pack_rgb(&self, rgb: u32) -> u32 {
        let r = (rgb >> 16) & 0xFF;
        let g = (rgb >> 8) & 0xFF;
        let b = rgb & 0xFF;
        (r << self.red_shift) | (g << self.green_shift) | (b << self.blue_shift)
    }

    pub fn blit_backbuffer(&self, backbuffer: &[u32]) {
        let stride_u32 = self.pitch_bytes / 4;
        if backbuffer.len() < self.width * self.height {
            return;
        }
        unsafe {
            if stride_u32 == self.width {
                core::ptr::copy_nonoverlapping(backbuffer.as_ptr(), self.addr, self.width * self.height);
            } else {
                for y in 0..self.height {
                    let src = backbuffer.as_ptr().add(y * self.width);
                    let dst = self.addr.add(y * stride_u32);
                    core::ptr::copy_nonoverlapping(src, dst, self.width);
                }
            }
        }
    }

    pub fn blit_rect(&self, backbuffer: &[u32], x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 || backbuffer.len() < self.width * self.height {
            return;
        }
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x + w).min(self.width as i32).max(0) as usize;
        let y1 = (y + h).min(self.height as i32).max(0) as usize;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let count = x1 - x0;
        let stride_u32 = self.pitch_bytes / 4;
        unsafe {
            for py in y0..y1 {
                let src = backbuffer.as_ptr().add(py * self.width + x0);
                let dst = self.addr.add(py * stride_u32 + x0);
                core::ptr::copy_nonoverlapping(src, dst, count);
            }
        }
    }

    pub fn fill_solid(&self, rgb: u32) {
        let packed = self.pack_rgb(rgb);
        let stride_u32 = self.pitch_bytes / 4;
        unsafe {
            for y in 0..self.height {
                let row = slice::from_raw_parts_mut(self.addr.add(y * stride_u32), self.width);
                row.fill(packed);
            }
        }
    }

    pub fn draw_banner_text(&self, start_x: usize, start_y: usize, text: &str, fg_rgb: u32, scale: usize) {
        use font8x8::{BASIC_FONTS, UnicodeFonts};
        let packed = self.pack_rgb(fg_rgb);
        let stride_u32 = self.pitch_bytes / 4;
        let mut cursor_x = start_x;
        let mut cursor_y = start_y;

        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = start_x;
                cursor_y += 10 * scale;
                continue;
            }
            let glyph = BASIC_FONTS.get(ch).unwrap_or([0; 8]);
            for (row_idx, &bits) in glyph.iter().enumerate() {
                for col_idx in 0..8 {
                    if (bits >> col_idx) & 1 != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = cursor_x + col_idx * scale + sx;
                                let py = cursor_y + row_idx * scale + sy;
                                if px < self.width && py < self.height {
                                    unsafe {
                                        self.addr.add(py * stride_u32 + px).write_volatile(packed);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            cursor_x += 8 * scale;
        }
    }
}
