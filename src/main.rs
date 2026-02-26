#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::info::PixelFormat;
use core::panic::PanicInfo;

#[allow(deprecated)]
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config.kernel_stack_size = 512 * 1024; // 512 KiB (default 80 KiB is too small)
    config.frame_buffer.minimum_framebuffer_height = Some(720);
    config.frame_buffer.minimum_framebuffer_width = Some(1280);
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

    // Initialize framebuffer
    if let Some(fb) = boot_info.framebuffer.as_mut() {
        let info = fb.info();
        let fb_info = kernel::framebuffer::FramebufferInfo {
            width: info.width,
            height: info.height,
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            is_bgr: matches!(info.pixel_format, PixelFormat::Bgr),
        };
        let w = info.width;
        let h = info.height;
        kernel::serial_println!(
            "Framebuffer: {}x{}, {} bpp, {:?}",
            w, h, info.bytes_per_pixel, info.pixel_format
        );
        kernel::framebuffer::init(fb.buffer_mut(), fb_info);
        kernel::console::init(w, h);
        kernel::serial_println!("Framebuffer console initialized");
    } else {
        kernel::serial_println!("WARNING: No framebuffer available");
    }

    // VGA output (now goes to both framebuffer and serial)
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

    kernel::filesystem::init();
    kernel::serial_println!("Filesystem initialized");

    kernel::interrupts::init_pit();
    kernel::serial_println!("PIT configured at 100 Hz");

    kernel::task::process::init();
    kernel::serial_println!("Process table initialized");

    kernel::task::scheduler::init();

    kernel::println!("All subsystems initialized.");

    let mut executor = kernel::task::executor::Executor::new();
    executor.spawn_process(
        alloc::string::String::from("shell"),
        kernel::shell::run(),
        None,
    );
    executor.run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::println!("{}", info);
    kernel::serial_println!("{}", info);
    kernel::hlt_loop()
}
