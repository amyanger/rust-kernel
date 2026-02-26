// Integration test: verify async executor runs tasks correctly.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use kernel::{allocator, memory, task};

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(main, config = &BOOTLOADER_CONFIG);

fn main(boot_info: &'static mut BootInfo) -> ! {
    kernel::init();

    let phys_mem_offset = x86_64::VirtAddr::new(
        boot_info
            .physical_memory_offset
            .into_option()
            .expect("physical_memory_offset not available"),
    );
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_regions) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    test_main();
    kernel::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}

#[test_case]
fn test_async_task_completes() {
    static COMPLETED: AtomicBool = AtomicBool::new(false);

    let mut executor = task::executor::Executor::new();
    executor.spawn(task::Task::new(async {
        COMPLETED.store(true, Ordering::SeqCst);
    }));
    executor.run_until_idle();

    assert!(COMPLETED.load(Ordering::SeqCst));
}

#[test_case]
fn test_multiple_tasks() {
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let mut executor = task::executor::Executor::new();
    executor.spawn(task::Task::new(async {
        COUNTER.fetch_add(1, Ordering::SeqCst);
    }));
    executor.spawn(task::Task::new(async {
        COUNTER.fetch_add(1, Ordering::SeqCst);
    }));
    executor.spawn(task::Task::new(async {
        COUNTER.fetch_add(1, Ordering::SeqCst);
    }));
    executor.run_until_idle();

    assert_eq!(COUNTER.load(Ordering::SeqCst), 3);
}

#[test_case]
fn test_yield_now() {
    static COMPLETED: AtomicBool = AtomicBool::new(false);

    let mut executor = task::executor::Executor::new();
    executor.spawn(task::Task::new(async {
        task::yield_now().await;
        COMPLETED.store(true, Ordering::SeqCst);
    }));

    // First poll: yield_now returns Pending and wakes itself
    executor.run_until_idle();
    assert!(!COMPLETED.load(Ordering::SeqCst));

    // Second poll: yield_now returns Ready, then COMPLETED is set
    executor.run_until_idle();
    assert!(COMPLETED.load(Ordering::SeqCst));
}
