/// Text output driver.
///
/// The bootloader sets up a graphical framebuffer (not VGA text mode),
/// so we route all text output through the serial port. The print!/println!
/// macros write to serial, which QEMU redirects to stdio.
///
/// A future enhancement could render text directly to the framebuffer
/// using a bitmap font.

use core::fmt;
use spin::Mutex;

const BUFFER_WIDTH: usize = 80;

pub struct Writer {
    pub column_position: usize,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        // Write to serial port (COM1 at 0x3F8)
        unsafe {
            x86_64::instructions::port::Port::new(0x3F8).write(byte);
        }
        if byte == b'\n' {
            self.column_position = 0;
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
        // Send ANSI clear sequence to serial terminal
        self.write_string("\x1b[2J\x1b[H");
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
