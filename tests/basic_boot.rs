/// Integration test: verify the kernel boots and basic printing works.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_entry = "test_main"]

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use kernel::{println, serial_println};

entry_point!(main);

fn main(_boot_info: &'static mut BootInfo) -> ! {
    kernel::init();
    test_main();
    kernel::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}

#[test_case]
fn test_serial_println() {
    serial_println!("test_serial_println output");
}
