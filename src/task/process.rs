extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::TaskId;

pub type Pid = u64;

/// PID of the shell process (first task spawned by the executor).
pub const SHELL_PID: Pid = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Sleeping,
    Blocked,
    Terminated,
}

impl core::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProcessState::Ready => write!(f, "Ready"),
            ProcessState::Running => write!(f, "Running"),
            ProcessState::Sleeping => write!(f, "Sleeping"),
            ProcessState::Blocked => write!(f, "Blocked"),
            ProcessState::Terminated => write!(f, "Terminated"),
        }
    }
}

pub struct Process {
    pub task_id: TaskId,
    pub name: String,
    pub state: ProcessState,
    pub parent_pid: Option<Pid>,
    pub exit_code: Option<i32>,
    pub is_thread: bool,
}

pub struct ProcessTable {
    processes: BTreeMap<Pid, Process>,
}

impl ProcessTable {
    fn new() -> Self {
        ProcessTable {
            processes: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, task_id: TaskId, name: String, parent_pid: Option<Pid>, is_thread: bool) {
        let pid = task_id.as_u64();
        self.processes.insert(
            pid,
            Process {
                task_id,
                name,
                state: ProcessState::Ready,
                parent_pid,
                exit_code: None,
                is_thread,
            },
        );
    }

    pub fn terminate(&mut self, pid: Pid, exit_code: i32) {
        if let Some(proc) = self.processes.get_mut(&pid) {
            proc.state = ProcessState::Terminated;
            proc.exit_code = Some(exit_code);
        }
    }

    pub fn set_state(&mut self, pid: Pid, state: ProcessState) {
        if let Some(proc) = self.processes.get_mut(&pid) {
            if proc.state != ProcessState::Terminated {
                proc.state = state;
            }
        }
    }

    pub fn list(&self) -> Vec<(Pid, &Process)> {
        self.processes.iter().map(|(&pid, proc)| (pid, proc)).collect()
    }

    pub fn get(&self, pid: Pid) -> Option<&Process> {
        self.processes.get(&pid)
    }

    pub fn is_alive(&self, pid: Pid) -> bool {
        self.processes
            .get(&pid)
            .map(|p| p.state != ProcessState::Terminated)
            .unwrap_or(false)
    }
}

pub static PROCESS_TABLE: Mutex<Option<ProcessTable>> = Mutex::new(None);

/// Unified kill: dispatches to thread scheduler or async executor based on process type.
pub fn kill_process(pid: Pid) {
    let is_thread = x86_64::instructions::interrupts::without_interrupts(|| {
        let table = PROCESS_TABLE.lock();
        table.as_ref().and_then(|t| t.get(pid)).map(|p| p.is_thread)
    });
    match is_thread {
        Some(true) => { super::scheduler::kill_thread(pid); }
        Some(false) => { super::executor::kill_request(pid); }
        None => {}
    }
}

pub fn init() {
    *PROCESS_TABLE.lock() = Some(ProcessTable::new());
}

/// Demo async task: prints `count` ticks, yielding between each.
pub async fn demo_counter(name: String, count: u32) {
    for i in 1..=count {
        crate::println!("[{}] tick {}/{}", name, i, count);
        super::yield_now().await;
    }
    crate::println!("[{}] finished", name);
}
