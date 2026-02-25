/// Text output driver.
///
/// Routes all print!/println! output to both the framebuffer console
/// (for on-screen display) and the serial port (for QEMU stdio debug).

use core::fmt;
use spin::Mutex;

use crate::console::CONSOLE;

const BUFFER_WIDTH: usize = 80;

pub struct Writer {
    pub column_position: usize,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        // Write to serial port (COM1 at 0x3F8)
        if byte == 0x08 {
            // Backspace: erase character on serial terminal (back + space + back)
            unsafe {
                let mut port: x86_64::instructions::port::Port<u8> =
                    x86_64::instructions::port::Port::new(0x3F8);
                port.write(0x08);
                port.write(b' ');
                port.write(0x08);
            }
        } else {
            unsafe {
                x86_64::instructions::port::Port::new(0x3F8).write(byte);
            }
        }

        // Write to framebuffer console
        let mut console = CONSOLE.lock();
        if let Some(c) = console.as_mut() {
            c.write_byte(byte);
        }

        // Track column position
        if byte == b'\n' {
            self.column_position = 0;
        } else if byte == 0x08 {
            if self.column_position > 0 {
                self.column_position -= 1;
            }
        } else {
            self.column_position += 1;
            if self.column_position >= BUFFER_WIDTH {
                self.column_position = 0;
            }
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(b'?'),
            }
        }
    }

    pub fn clear_screen(&mut self) {
        // Clear framebuffer console
        let mut console = CONSOLE.lock();
        if let Some(c) = console.as_mut() {
            c.clear();
        }

        // Also send ANSI clear to serial terminal
        unsafe {
            for b in b"\x1b[2J\x1b[H" {
                x86_64::instructions::port::Port::new(0x3F8).write(*b);
            }
        }
        self.column_position = 0;
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

pub static WRITER: Mutex<Writer> = Mutex::new(Writer {
    column_position: 0,
});

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(::core::format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", ::core::format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}
