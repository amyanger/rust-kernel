/// Serial port driver using UART 16550.
///
/// The UART chip converts parallel data to serial for communication.
/// COM1 is at I/O port 0x3F8. QEMU redirects serial to stdio,
/// so we use this for test output and debug logging.

use spin::Mutex;
use uart_16550::SerialPort;

pub static SERIAL1: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(0x3F8) });

pub fn init() {
    SERIAL1.lock().init();
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(::core::format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", ::core::format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        SERIAL1.lock().write_fmt(args).unwrap();
    });
}
