/// Preemptive thread scheduler.
///
/// Manages kernel threads that each have their own stack and are
/// preempted by the timer interrupt. The executor's main loop is the
/// "idle context" — when no threads are ready, control returns there
/// to poll async futures as before.

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

use super::context::InterruptFrame;
use super::process::PROCESS_TABLE;
use super::TaskId;

const THREAD_STACK_SIZE: usize = 16 * 1024; // 16 KiB per thread

// Kernel segment selectors (must match gdt.rs init order)
const KERNEL_CS: u64 = 0x08;
const KERNEL_SS: u64 = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Sleeping(u64), // absolute tick count at which to wake
    Terminated,
}

pub struct Thread {
    pub pid: u64,
    pub name: String,
    pub state: ThreadState,
    pub parent_pid: Option<u64>,
    stack_bottom: *mut u8,
    stack_size: usize,
    saved_frame: *mut InterruptFrame,
}

// Thread contains raw pointers but is only accessed with the scheduler lock held.
unsafe impl Send for Thread {}

pub struct Scheduler {
    threads: VecDeque<Thread>,
    current: Option<Thread>,
    idle_frame: *mut InterruptFrame,
    // Deferred stack deallocation: we can't free a thread's stack while the
    // ISR is still running on it, so we defer it to the next schedule() call.
    deferred_dealloc: Option<(*mut u8, usize)>,
}

unsafe impl Send for Scheduler {}

pub static SCHEDULER: Mutex<Option<Scheduler>> = Mutex::new(None);
static SCHEDULER_ENABLED: AtomicBool = AtomicBool::new(false);

fn alloc_thread_id() -> u64 {
    // Use the same TaskId counter so PIDs don't collide with async tasks
    TaskId::new().as_u64()
}

/// Initialize the scheduler. Call after process table init.
pub fn init() {
    *SCHEDULER.lock() = Some(Scheduler {
        threads: VecDeque::new(),
        current: None,
        idle_frame: core::ptr::null_mut(),
        deferred_dealloc: None,
    });
    SCHEDULER_ENABLED.store(true, Ordering::Release);
    crate::serial_println!("Preemptive scheduler initialized");
}

pub fn is_enabled() -> bool {
    SCHEDULER_ENABLED.load(Ordering::Acquire)
}

/// Called from the timer ISR. Uses try_lock to avoid deadlock if the
/// scheduler lock is already held by the preempted code.
pub fn try_schedule(current_frame: *mut InterruptFrame) -> Option<*mut InterruptFrame> {
    let mut guard = SCHEDULER.try_lock()?;
    let sched = guard.as_mut()?;
    Some(sched.schedule(current_frame))
}

impl Scheduler {
    fn schedule(&mut self, current_frame: *mut InterruptFrame) -> *mut InterruptFrame {
        // Free any previously-deferred stack (safe: we're now on a different stack)
        if let Some((ptr, size)) = self.deferred_dealloc.take() {
            dealloc_stack(ptr, size);
        }

        // Save context of whoever was running
        match self.current.take() {
            Some(mut thread) => {
                thread.saved_frame = current_frame;
                match thread.state {
                    ThreadState::Terminated => {
                        // Defer deallocation — the ISR is still running on this stack
                        self.deferred_dealloc = Some((thread.stack_bottom, thread.stack_size));
                    }
                    ThreadState::Sleeping(_) => {
                        // Preserve sleep state — don't overwrite to Ready
                        self.threads.push_back(thread);
                    }
                    _ => {
                        thread.state = ThreadState::Ready;
                        self.threads.push_back(thread);
                    }
                }
            }
            None => {
                // Was in idle/executor context
                self.idle_frame = current_frame;
            }
        }

        // Clean out terminated threads, wake expired sleepers, find next ready one
        let current_tick = crate::interrupts::TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed);
        let len = self.threads.len();
        for _ in 0..len {
            if let Some(mut thread) = self.threads.pop_front() {
                if thread.state == ThreadState::Terminated {
                    // Threads in the ready queue are not currently executing,
                    // so their stacks can be freed immediately.
                    dealloc_stack(thread.stack_bottom, thread.stack_size);
                    continue;
                }
                // Wake sleeping threads whose time has come
                if let ThreadState::Sleeping(wake_tick) = thread.state {
                    if current_tick >= wake_tick {
                        thread.state = ThreadState::Ready;
                    }
                }
                // Pick first Ready thread
                if thread.state == ThreadState::Ready {
                    let frame = thread.saved_frame;
                    self.current = Some(thread);
                    self.current.as_mut().unwrap().state = ThreadState::Running;
                    return frame;
                }
                // Still sleeping — put back
                self.threads.push_back(thread);
            }
        }

        // No ready threads — return to idle context
        self.idle_frame
    }
}

fn dealloc_stack(stack_bottom: *mut u8, stack_size: usize) {
    if !stack_bottom.is_null() {
        unsafe {
            let layout = alloc::alloc::Layout::from_size_align(stack_size, 16).unwrap();
            alloc::alloc::dealloc(stack_bottom, layout);
        }
    }
}

/// Spawn a new preemptible thread. Returns the thread's PID.
pub fn spawn_thread(
    name: String,
    entry_fn: fn(u64),
    arg: u64,
    parent_pid: Option<u64>,
) -> u64 {
    let pid = alloc_thread_id();

    // Allocate stack
    let layout = alloc::alloc::Layout::from_size_align(THREAD_STACK_SIZE, 16).unwrap();
    let stack_bottom = unsafe { alloc::alloc::alloc_zeroed(layout) };
    if stack_bottom.is_null() {
        panic!("Failed to allocate thread stack");
    }
    let stack_top = unsafe { stack_bottom.add(THREAD_STACK_SIZE) } as u64;

    // Build a synthetic InterruptFrame at the top of the stack.
    // When the scheduler switches to this thread, the ISR will pop these
    // registers and iretq will jump to thread_entry_wrapper.
    let frame_ptr = unsafe {
        let ptr = (stack_top as *mut InterruptFrame).sub(1);
        core::ptr::write(ptr, InterruptFrame {
            r15: 0, r14: 0, r13: 0, r12: 0,
            r11: 0, r10: 0, r9: 0, r8: 0,
            rbp: 0,
            rdi: arg,
            rsi: entry_fn as u64,
            rdx: 0, rcx: 0, rbx: 0, rax: 0,
            rip: thread_entry_wrapper as *const () as u64,
            cs: KERNEL_CS,
            rflags: 0x202, // IF (interrupts enabled) + reserved bit 1
            rsp: stack_top, // thread starts with empty stack
            ss: KERNEL_SS,
        });
        ptr
    };

    let thread = Thread {
        pid,
        name: name.clone(),
        state: ThreadState::Ready,
        parent_pid,
        stack_bottom,
        stack_size: THREAD_STACK_SIZE,
        saved_frame: frame_ptr,
    };

    // Register in process table (with interrupts disabled to prevent
    // preemption while holding the lock)
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut table = PROCESS_TABLE.lock();
        if let Some(table) = table.as_mut() {
            table.register_thread(pid, name, parent_pid);
        }
    });

    // Add to scheduler ready queue
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(sched) = sched.as_mut() {
            sched.threads.push_back(thread);
        }
    });

    pid
}

/// Entry point for all threads. Called via iretq from the synthetic frame.
/// rdi = arg, rsi = actual entry function pointer (set up in the synthetic frame).
extern "C" fn thread_entry_wrapper(arg: u64, entry_fn: u64) {
    let f: fn(u64) = unsafe { core::mem::transmute(entry_fn) };
    f(arg);
    exit_current_thread();
}

/// Mark the current thread as terminated and halt until preempted.
pub fn exit_current_thread() {
    // Acquire SCHEDULER lock with interrupts disabled to prevent preemption
    // while holding the lock. Release it before touching PROCESS_TABLE to
    // avoid nested lock deadlocks.
    let pid = x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(sched) = sched.as_mut() {
            if let Some(thread) = sched.current.as_mut() {
                thread.state = ThreadState::Terminated;
                return Some(thread.pid);
            }
        }
        None
    });

    if let Some(pid) = pid {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut table = PROCESS_TABLE.lock();
            if let Some(table) = table.as_mut() {
                table.terminate(pid, 0);
            }
        });
    }

    // Halt until the next timer tick preempts us and cleans up
    loop {
        x86_64::instructions::hlt();
    }
}

/// Kill a thread by PID. Marks it terminated; cleanup happens on next schedule.
pub fn kill_thread(pid: u64) -> bool {
    // Acquire SCHEDULER with interrupts disabled, release before touching
    // PROCESS_TABLE to avoid nested lock deadlocks.
    let found = x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(sched) = sched.as_mut() {
            if let Some(ref mut current) = sched.current {
                if current.pid == pid {
                    current.state = ThreadState::Terminated;
                    return true;
                }
            }
            for thread in sched.threads.iter_mut() {
                if thread.pid == pid {
                    thread.state = ThreadState::Terminated;
                    return true;
                }
            }
        }
        false
    });

    if found {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut table = PROCESS_TABLE.lock();
            if let Some(table) = table.as_mut() {
                table.terminate(pid, 1);
            }
        });
    }

    found
}

/// Check if a PID belongs to a preemptible thread.
pub fn is_thread(pid: u64) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let table = PROCESS_TABLE.lock();
        if let Some(table) = table.as_ref() {
            if let Some(proc) = table.get(pid) {
                return proc.is_thread;
            }
        }
        false
    })
}

// --- Sleep support ---

/// Put the current thread to sleep for approximately `ms` milliseconds.
/// Rounds up to 10ms granularity (PIT runs at 100 Hz).
pub fn sleep_ms(ms: u64) {
    let ticks = (ms + 9) / 10; // round up to 10ms granularity
    if ticks == 0 {
        return;
    }

    let current_tick = crate::interrupts::TICK_COUNT.load(Ordering::Relaxed);
    let wake_tick = current_tick.saturating_add(ticks);

    // Mark current thread as sleeping (interrupts disabled to prevent preemption
    // while holding lock)
    let pid = x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if let Some(sched) = sched.as_mut() {
            if let Some(thread) = sched.current.as_mut() {
                thread.state = ThreadState::Sleeping(wake_tick);
                return Some(thread.pid);
            }
        }
        None
    });

    if pid.is_none() {
        crate::serial_println!("WARNING: sleep_ms called from non-thread context");
        return;
    }

    if let Some(pid) = pid {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut table = PROCESS_TABLE.lock();
            if let Some(table) = table.as_mut() {
                table.set_state(pid, crate::task::process::ProcessState::Sleeping);
            }
        });
    }

    // Halt until wake tick reached
    loop {
        x86_64::instructions::hlt();
        if crate::interrupts::TICK_COUNT.load(Ordering::Relaxed) >= wake_tick {
            break;
        }
    }

    // Restore process table state
    if let Some(pid) = pid {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut table = PROCESS_TABLE.lock();
            if let Some(table) = table.as_mut() {
                table.set_state(pid, crate::task::process::ProcessState::Ready);
            }
        });
    }
}

// --- Demo thread entry functions ---

/// A thread that prints messages with sleep pauses. Used by `tspawn`.
pub fn demo_thread_entry(arg: u64) {
    let count = arg as u32;

    // Read pid and name with interrupts disabled to prevent preemption
    // while holding spin locks.
    let (_pid, name) = x86_64::instructions::interrupts::without_interrupts(|| {
        let pid = {
            let sched = SCHEDULER.lock();
            sched.as_ref().and_then(|s| s.current.as_ref()).map(|t| t.pid).unwrap_or(0)
        };
        let name = {
            let table = PROCESS_TABLE.lock();
            table.as_ref()
                .and_then(|t| t.get(pid))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| String::from("?"))
        };
        (pid, name)
    });

    for i in 1..=count {
        crate::serial_println!("[T:{}] tick {}/{}", name, i, count);
        crate::println!("[T:{}] tick {}/{}", name, i, count);
        sleep_ms(500);
    }
    crate::serial_println!("[T:{}] finished", name);
    crate::println!("[T:{}] finished", name);
}

/// A thread that sleeps for a given duration. Used by the `sleep` shell command.
pub fn sleep_thread_entry(arg: u64) {
    let ms = arg;
    crate::serial_println!("[sleep] sleeping for {}ms", ms);
    crate::println!("[sleep] sleeping for {}ms", ms);
    sleep_ms(ms);
    crate::serial_println!("[sleep] woke up after {}ms", ms);
    crate::println!("[sleep] woke up after {}ms", ms);
}
