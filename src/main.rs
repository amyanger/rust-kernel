#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use core::panic::PanicInfo;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// Write a byte directly to the serial port (COM1 at 0x3F8).
/// No initialization needed for basic QEMU serial — just write.
fn serial_byte(b: u8) {
    unsafe {
        x86_64::instructions::port::Port::new(0x3F8).write(b);
    }
}

fn serial_str(s: &str) {
    for b in s.bytes() {
        serial_byte(b);
    }
}

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // Absolute first thing: write directly to serial port
    serial_str("KERNEL ENTRY\n");

    // Init serial properly
    kernel::serial::init();
    kernel::serial_println!("Serial initialized");

    // VGA output
    kernel::println!("Booting RustKernel...");
    kernel::serial_println!("VGA print done");

    // Init GDT, IDT, PICs
    kernel::init();
    kernel::serial_println!("GDT, IDT, PICs initialized");

    // Set up paging and heap
    let phys_mem_offset = x86_64::VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("physical_memory_offset not available"),
    );

    let mut mapper = unsafe { kernel::memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { kernel::memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };

    kernel::allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
    kernel::serial_println!("Heap initialized");

    kernel::println!("All subsystems initialized.");
    kernel::shell::run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::println!("{}", info);
    kernel::serial_println!("{}", info);
    kernel::hlt_loop()
}
