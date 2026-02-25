/// Text console rendered on the framebuffer.
///
/// Maintains a cursor position, foreground/background colors, and handles
/// newlines, carriage returns, line wrapping, and scrolling.

use core::fmt;
use spin::Mutex;

use crate::font::{self, CHAR_HEIGHT, CHAR_WIDTH};
use crate::framebuffer::FRAMEBUFFER;

pub struct Console {
    col: usize,
    row: usize,
    cols: usize,
    rows: usize,
    fg: (u8, u8, u8),
    bg: (u8, u8, u8),
}

pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);

impl Console {
    pub fn new(fb_width: usize, fb_height: usize) -> Self {
        Self {
            col: 0,
            row: 0,
            cols: fb_width / CHAR_WIDTH,
            rows: fb_height / CHAR_HEIGHT,
            fg: (255, 255, 255), // White text
            bg: (0, 0, 0),       // Black background
        }
    }

    pub fn set_fg(&mut self, r: u8, g: u8, b: u8) {
        self.fg = (r, g, b);
    }

    pub fn set_bg(&mut self, r: u8, g: u8, b: u8) {
        self.bg = (r, g, b);
    }

    pub fn fg(&self) -> (u8, u8, u8) {
        self.fg
    }

    pub fn bg(&self) -> (u8, u8, u8) {
        self.bg
    }

    pub fn column_position(&self) -> usize {
        self.col
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            0x08 => {
                // Backspace
                if self.col > 0 {
                    self.col -= 1;
                    self.render_char(b' ');
                }
            }
            byte => {
                if self.col >= self.cols {
                    self.newline();
                }
                self.render_char(byte);
                self.col += 1;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7E | b'\n' | b'\r' => self.write_byte(byte),
                _ => self.write_byte(b'?'),
            }
        }
    }

    pub fn clear(&mut self) {
        let mut fb_lock = FRAMEBUFFER.lock();
        if let Some(fb) = fb_lock.as_mut() {
            fb.clear(self.bg.0, self.bg.1, self.bg.2);
        }
        self.col = 0;
        self.row = 0;
    }

    fn render_char(&mut self, byte: u8) {
        let px = self.col * CHAR_WIDTH;
        let py = self.row * CHAR_HEIGHT;
        let mut fb_lock = FRAMEBUFFER.lock();
        if let Some(fb) = fb_lock.as_mut() {
            font::draw_char(fb, px, py, byte, self.fg, self.bg);
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.rows {
            self.scroll();
            self.row = self.rows - 1;
        }
    }

    fn scroll(&mut self) {
        let mut fb_lock = FRAMEBUFFER.lock();
        if let Some(fb) = fb_lock.as_mut() {
            fb.scroll_up(CHAR_HEIGHT, self.bg.0, self.bg.1, self.bg.2);
        }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub fn init(fb_width: usize, fb_height: usize) {
    *CONSOLE.lock() = Some(Console::new(fb_width, fb_height));
}
