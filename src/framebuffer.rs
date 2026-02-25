/// Framebuffer graphics driver.
///
/// Safe wrapper around the raw framebuffer memory provided by the bootloader.
/// Supports pixel drawing, rectangles, lines (Bresenham), and circles (midpoint).

use spin::Mutex;

pub struct FramebufferInfo {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub bytes_per_pixel: usize,
    pub is_bgr: bool,
}

pub struct Framebuffer {
    buffer: &'static mut [u8],
    info: FramebufferInfo,
}

pub static FRAMEBUFFER: Mutex<Option<Framebuffer>> = Mutex::new(None);

impl Framebuffer {
    pub fn new(buffer: &'static mut [u8], info: FramebufferInfo) -> Self {
        Self { buffer, info }
    }

    pub fn info(&self) -> &FramebufferInfo {
        &self.info
    }

    pub fn width(&self) -> usize {
        self.info.width
    }

    pub fn height(&self) -> usize {
        self.info.height
    }

    #[inline]
    pub fn put_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let offset = y * self.info.stride * self.info.bytes_per_pixel
            + x * self.info.bytes_per_pixel;
        if self.info.is_bgr {
            self.buffer[offset] = b;
            self.buffer[offset + 1] = g;
            self.buffer[offset + 2] = r;
        } else {
            self.buffer[offset] = r;
            self.buffer[offset + 1] = g;
            self.buffer[offset + 2] = b;
        }
        if self.info.bytes_per_pixel == 4 {
            self.buffer[offset + 3] = 0xFF;
        }
    }

    pub fn clear(&mut self, r: u8, g: u8, b: u8) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.put_pixel(x, y, r, g, b);
            }
        }
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, r: u8, g: u8, b: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.put_pixel(x + dx, y + dy, r, g, b);
            }
        }
    }

    /// Bresenham's line algorithm.
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, r: u8, g: u8, b: u8) {
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x >= 0 && y >= 0 {
                self.put_pixel(x as usize, y as usize, r, g, b);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Midpoint circle algorithm (filled).
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i32, r: u8, g: u8, b: u8) {
        let mut x = 0i32;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            // Draw horizontal lines for each octant pair to fill the circle
            self.draw_hline(cx - x, cx + x, cy + y, r, g, b);
            self.draw_hline(cx - x, cx + x, cy - y, r, g, b);
            self.draw_hline(cx - y, cx + y, cy + x, r, g, b);
            self.draw_hline(cx - y, cx + y, cy - x, r, g, b);

            x += 1;
            if d < 0 {
                d += 2 * x + 1;
            } else {
                y -= 1;
                d += 2 * (x - y) + 1;
            }
        }
    }

    fn draw_hline(&mut self, x0: i32, x1: i32, y: i32, r: u8, g: u8, b: u8) {
        if y < 0 || y >= self.info.height as i32 {
            return;
        }
        let start = x0.max(0) as usize;
        let end = (x1 as usize).min(self.info.width.saturating_sub(1));
        for x in start..=end {
            self.put_pixel(x, y as usize, r, g, b);
        }
    }

    /// Scroll the framebuffer up by `rows` pixels. Clears the vacated bottom area.
    pub fn scroll_up(&mut self, rows: usize, bg_r: u8, bg_g: u8, bg_b: u8) {
        let bpp = self.info.bytes_per_pixel;
        let stride_bytes = self.info.stride * bpp;
        let src_start = rows * stride_bytes;
        let total = self.info.height * stride_bytes;

        if src_start < total {
            self.buffer.copy_within(src_start..total, 0);
        }

        // Clear the bottom rows
        let clear_start = (self.info.height - rows) * stride_bytes;
        for y in 0..rows {
            for x in 0..self.info.width {
                let offset = clear_start + y * stride_bytes + x * bpp;
                if self.info.is_bgr {
                    self.buffer[offset] = bg_b;
                    self.buffer[offset + 1] = bg_g;
                    self.buffer[offset + 2] = bg_r;
                } else {
                    self.buffer[offset] = bg_r;
                    self.buffer[offset + 1] = bg_g;
                    self.buffer[offset + 2] = bg_b;
                }
                if bpp == 4 {
                    self.buffer[offset + 3] = 0xFF;
                }
            }
        }
    }
}

pub fn init(buffer: &'static mut [u8], info: FramebufferInfo) {
    let mut fb = Framebuffer::new(buffer, info);
    fb.clear(0, 0, 0); // Black background
    *FRAMEBUFFER.lock() = Some(fb);
}
