extern crate alloc;

use super::process::{Pid, ProcessState, PROCESS_TABLE};
use super::{Task, TaskId};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

static WAKE_QUEUE: Mutex<VecDeque<TaskId>> = Mutex::new(VecDeque::new());
static KILL_QUEUE: Mutex<VecDeque<Pid>> = Mutex::new(VecDeque::new());

fn wake_task(task_id: TaskId) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        WAKE_QUEUE.lock().push_back(task_id);
    });
}

struct TaskWaker {
    task_id: TaskId,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        wake_task(self.task_id);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        wake_task(self.task_id);
    }
}

/// Request to spawn a process. Queued and processed by the executor loop.
pub struct TaskSpawnRequest {
    pub task: Task,
    pub name: String,
    pub parent_pid: Option<Pid>,
}

// Safety: single-core kernel with no real threads. The spawn queue is only
// accessed with interrupts disabled, ensuring no concurrent mutation.
unsafe impl Send for TaskSpawnRequest {}

static TASK_SPAWN_QUEUE: Mutex<VecDeque<TaskSpawnRequest>> = Mutex::new(VecDeque::new());

/// Called from async context (e.g. shell) to request spawning a new process.
/// Returns the PID that will be assigned.
pub fn spawn_request(
    name: String,
    future: impl Future<Output = ()> + 'static,
    parent_pid: Option<Pid>,
) -> Pid {
    let task = Task::new(future);
    let pid = task.id.as_u64();
    x86_64::instructions::interrupts::without_interrupts(|| {
        TASK_SPAWN_QUEUE.lock().push_back(TaskSpawnRequest {
            task,
            name,
            parent_pid,
        });
    });
    pid
}

/// Called from async context to request killing a process by PID.
pub fn kill_request(pid: Pid) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        KILL_QUEUE.lock().push_back(pid);
    });
}

pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    ready_queue: VecDeque<TaskId>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Self {
        Executor {
            tasks: BTreeMap::new(),
            ready_queue: VecDeque::new(),
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, task: Task) {
        let task_id = task.id;
        if self.tasks.insert(task_id, task).is_some() {
            panic!("task with same ID already in tasks");
        }
        self.ready_queue.push_back(task_id);
    }

    /// Spawn a task and register it in the process table.
    pub fn spawn_process(&mut self, name: String, future: impl Future<Output = ()> + 'static, parent_pid: Option<Pid>) {
        let task = Task::new(future);
        let task_id = task.id;
        self.spawn(task);

        let mut table = PROCESS_TABLE.lock();
        if let Some(table) = table.as_mut() {
            table.register(task_id, name, parent_pid, false);
        }
    }

    pub fn run(&mut self) -> ! {
        loop {
            self.drain_spawn_queue();
            self.drain_kill_queue();
            self.drain_wake_queue();
            self.poll_ready_tasks();
            self.sleep_if_idle();
        }
    }

    pub fn run_until_idle(&mut self) {
        self.drain_spawn_queue();
        self.drain_kill_queue();
        self.drain_wake_queue();
        self.poll_ready_tasks();
    }

    fn drain_spawn_queue(&mut self) {
        loop {
            let req = x86_64::instructions::interrupts::without_interrupts(|| {
                TASK_SPAWN_QUEUE.lock().pop_front()
            });
            match req {
                Some(req) => {
                    let task_id = req.task.id;
                    self.spawn(req.task);

                    let mut table = PROCESS_TABLE.lock();
                    if let Some(table) = table.as_mut() {
                        table.register(task_id, req.name, req.parent_pid, false);
                    }
                }
                None => break,
            }
        }
    }

    fn drain_kill_queue(&mut self) {
        loop {
            let pid = x86_64::instructions::interrupts::without_interrupts(|| {
                KILL_QUEUE.lock().pop_front()
            });
            match pid {
                Some(pid) => {
                    let task_id = TaskId::from_u64(pid);
                    // Remove the task (dropping its future)
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);

                    // Mark terminated in process table
                    let mut table = PROCESS_TABLE.lock();
                    if let Some(table) = table.as_mut() {
                        table.terminate(pid, 1); // exit code 1 = killed
                    }
                }
                None => break,
            }
        }
    }

    fn drain_wake_queue(&mut self) {
        loop {
            let task_id = x86_64::instructions::interrupts::without_interrupts(|| {
                WAKE_QUEUE.lock().pop_front()
            });
            match task_id {
                Some(id) => {
                    if self.tasks.contains_key(&id) {
                        self.ready_queue.push_back(id);

                        let mut table = PROCESS_TABLE.lock();
                        if let Some(table) = table.as_mut() {
                            table.set_state(id.as_u64(), ProcessState::Ready);
                        }
                    }
                }
                None => break,
            }
        }
    }

    fn poll_ready_tasks(&mut self) {
        while let Some(task_id) = self.ready_queue.pop_front() {
            let task = match self.tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue,
            };

            // Mark as Running before polling
            {
                let mut table = PROCESS_TABLE.lock();
                if let Some(table) = table.as_mut() {
                    table.set_state(task_id.as_u64(), ProcessState::Running);
                }
            }

            let waker = self.waker_cache.entry(task_id).or_insert_with(|| {
                Waker::from(Arc::new(TaskWaker { task_id }))
            });
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);

                    let mut table = PROCESS_TABLE.lock();
                    if let Some(table) = table.as_mut() {
                        table.terminate(task_id.as_u64(), 0); // clean exit
                    }
                }
                Poll::Pending => {
                    let mut table = PROCESS_TABLE.lock();
                    if let Some(table) = table.as_mut() {
                        table.set_state(task_id.as_u64(), ProcessState::Blocked);
                    }
                }
            }
        }
    }

    fn sleep_if_idle(&self) {
        x86_64::instructions::interrupts::disable();
        if self.ready_queue.is_empty() {
            let wake_queue_empty = WAKE_QUEUE.lock().is_empty();
            let spawn_queue_empty = TASK_SPAWN_QUEUE.lock().is_empty();
            let kill_queue_empty = KILL_QUEUE.lock().is_empty();
            if wake_queue_empty && spawn_queue_empty && kill_queue_empty {
                x86_64::instructions::interrupts::enable_and_hlt();
            } else {
                x86_64::instructions::interrupts::enable();
            }
        } else {
            x86_64::instructions::interrupts::enable();
        }
    }
}
