extern crate alloc;

use super::{Task, TaskId};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::task::Wake;
use core::task::{Context, Poll, Waker};
use spin::Mutex;

static WAKE_QUEUE: Mutex<VecDeque<TaskId>> = Mutex::new(VecDeque::new());

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

    pub fn run(&mut self) -> ! {
        loop {
            self.drain_wake_queue();
            self.poll_ready_tasks();
            self.sleep_if_idle();
        }
    }

    pub fn run_until_idle(&mut self) {
        self.drain_wake_queue();
        self.poll_ready_tasks();
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
            let waker = self.waker_cache.entry(task_id).or_insert_with(|| {
                Waker::from(Arc::new(TaskWaker { task_id }))
            });
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    fn sleep_if_idle(&self) {
        x86_64::instructions::interrupts::disable();
        if self.ready_queue.is_empty() {
            let wake_queue_empty = WAKE_QUEUE.lock().is_empty();
            if wake_queue_empty {
                x86_64::instructions::interrupts::enable_and_hlt();
            } else {
                x86_64::instructions::interrupts::enable();
            }
        } else {
            x86_64::instructions::interrupts::enable();
        }
    }
}
