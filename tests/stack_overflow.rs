/// Integration test: verify stack overflow triggers a double fault
/// (handled on a separate IST stack) instead of a triple fault.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use kernel::{exit_qemu, serial_print, serial_println, QemuExitCode};

entry_point!(main);

fn main(_boot_info: &'static mut BootInfo) -> ! {
    serial_print!("stack_overflow::stack_overflow...\t");

    kernel::gdt::init();
    init_test_idt();

    stack_overflow();

    panic!("Execution continued after stack overflow");
}

#[allow(unconditional_recursion)]
fn stack_overflow() {
    stack_overflow();
    core::hint::black_box(0); // prevent tail-call optimization
}

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

static mut TEST_IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();

fn init_test_idt() {
    unsafe {
        TEST_IDT
            .double_fault
            .set_handler_fn(test_double_fault_handler)
            .set_stack_index(kernel::gdt::DOUBLE_FAULT_IST_INDEX);
        TEST_IDT.load();
    }
}

extern "x86-interrupt" fn test_double_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    serial_println!("[ok]");
    exit_qemu(QemuExitCode::Success);
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}
