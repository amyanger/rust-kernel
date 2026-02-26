/// Interrupt Descriptor Table (IDT) and interrupt handlers.
///
/// The IDT tells the CPU which function to call for each interrupt:
///   - 0-31: CPU exceptions (divide by zero, page fault, double fault, etc.)
///   - 32-47: Hardware interrupts (remapped from PIC: timer, keyboard, etc.)
///
/// The PIC 8259 manages hardware interrupts. We remap IRQs 0-7 from
/// IDT entries 8-15 to 32-47 to avoid colliding with CPU exceptions.

use crate::gdt;
use crate::hlt_loop;
use core::sync::atomic::{AtomicU64, Ordering};
use pic8259::ChainedPics;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;

pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Configure the 8254 PIT to fire at ~100 Hz (10ms timeslice).
pub fn init_pit() {
    let divisor: u16 = (1_193_182u32 / 100) as u16;
    unsafe {
        Port::new(0x43).write(0x36u8);
        Port::new(0x40).write((divisor & 0xFF) as u8);
        Port::new(0x40).write((divisor >> 8) as u8);
    }
}

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

// --- Scancode queue for keyboard input ---

pub static SCANCODE_QUEUE: Mutex<ScancodeQueue> = Mutex::new(ScancodeQueue::new());

pub struct ScancodeQueue {
    buf: [u8; 128],
    read: usize,
    write: usize,
    count: usize,
}

impl ScancodeQueue {
    const fn new() -> Self {
        Self {
            buf: [0; 128],
            read: 0,
            write: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, scancode: u8) {
        if self.count < self.buf.len() {
            self.buf[self.write] = scancode;
            self.write = (self.write + 1) % self.buf.len();
            self.count += 1;
        }
    }

    pub fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let val = self.buf[self.read];
        self.read = (self.read + 1) % self.buf.len();
        self.count -= 1;
        Some(val)
    }
}

// --- IDT setup ---

static IDT: spin::Once<InterruptDescriptorTable> = spin::Once::new();

pub fn init_idt() {
    let idt = IDT.call_once(|| {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        unsafe {
            idt[InterruptIndex::Timer as u8]
                .set_handler_addr(VirtAddr::new(crate::task::context::timer_isr_addr()));
        }
        idt[InterruptIndex::Keyboard as u8].set_handler_fn(keyboard_interrupt_handler);
        idt
    });
    idt.load();
}

// --- CPU Exception Handlers ---

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    crate::println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    crate::println!("EXCEPTION: PAGE FAULT");
    crate::println!("Accessed Address: {:?}", Cr2::read());
    crate::println!("Error Code: {:?}", error_code);
    crate::println!("{:#?}", stack_frame);
    hlt_loop();
}

// --- Hardware Interrupt Handlers ---

/// Called from the raw timer ISR assembly stub.
/// Receives the current stack frame, returns the frame to resume (possibly different).
#[no_mangle]
extern "C" fn timer_tick_handler(frame: *mut crate::task::context::InterruptFrame) -> *mut crate::task::context::InterruptFrame {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer as u8);
    }

    // Try to schedule a context switch
    if crate::task::scheduler::is_enabled() {
        if let Some(new_frame) = crate::task::scheduler::try_schedule(frame) {
            return new_frame;
        }
    }

    frame
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    SCANCODE_QUEUE.lock().push(scancode);
    crate::task::keyboard::notify_keyboard_interrupt();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard as u8);
    }
}
