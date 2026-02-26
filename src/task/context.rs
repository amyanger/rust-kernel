/// CPU state saved/restored on timer interrupt for preemptive context switching.
///
/// The raw ISR stub pushes all 15 GP registers onto the current stack,
/// then calls the Rust handler with RSP as the argument. The handler
/// returns a (possibly different) RSP, and the stub pops registers from
/// the new frame and does `iretq`.

/// Saved CPU state on the stack during a timer interrupt.
/// Field order matches the push/pop order in the assembly ISR.
#[repr(C)]
pub struct InterruptFrame {
    // Pushed by our ISR stub (first pushed = highest address, but in memory
    // the struct starts at RSP which is the lowest address after all pushes)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Pushed by CPU on interrupt entry
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Returns the address of the raw timer ISR assembly stub for IDT registration.
pub fn timer_isr_addr() -> u64 {
    extern "C" {
        fn timer_isr();
    }
    timer_isr as *const () as u64
}

// Raw timer ISR: save all GP registers, call Rust handler, restore (possibly different) frame.
core::arch::global_asm!(
    ".global timer_isr",
    "timer_isr:",
    "push rax",
    "push rbx",
    "push rcx",
    "push rdx",
    "push rsi",
    "push rdi",
    "push rbp",
    "push r8",
    "push r9",
    "push r10",
    "push r11",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    // Pass pointer to InterruptFrame as first argument
    "mov rdi, rsp",
    // Clear direction flag (SysV ABI requires DF=0 on function entry)
    "cld",
    // Call Rust handler — returns new RSP in rax
    "call timer_tick_handler",
    // Switch to returned stack frame (may be a different thread's stack)
    "mov rsp, rax",
    // Restore registers from the (possibly new) frame
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop r11",
    "pop r10",
    "pop r9",
    "pop r8",
    "pop rbp",
    "pop rdi",
    "pop rsi",
    "pop rdx",
    "pop rcx",
    "pop rbx",
    "pop rax",
    "iretq",
);
