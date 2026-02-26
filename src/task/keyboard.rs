use crate::interrupts::SCANCODE_QUEUE;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};
use spin::Mutex;

static KEYBOARD_WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// Called from the keyboard IRQ handler to wake the async scancode consumer.
/// Uses try_lock() to avoid deadlock in interrupt context.
pub fn notify_keyboard_interrupt() {
    if let Some(guard) = KEYBOARD_WAKER.try_lock() {
        if let Some(waker) = guard.as_ref() {
            waker.wake_by_ref();
        }
    }
}

static SCANCODE_STREAM_EXISTS: AtomicBool = AtomicBool::new(false);

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        assert!(
            !SCANCODE_STREAM_EXISTS.swap(true, Ordering::SeqCst),
            "ScancodeStream is a singleton — only one instance allowed"
        );
        ScancodeStream { _private: () }
    }

    pub fn next(&self) -> ScancodeStreamFuture {
        ScancodeStreamFuture { _private: () }
    }
}

pub struct ScancodeStreamFuture {
    _private: (),
}

impl Future for ScancodeStreamFuture {
    type Output = u8;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
        // First check: try to pop a scancode
        let result = x86_64::instructions::interrupts::without_interrupts(|| {
            SCANCODE_QUEUE.lock().pop()
        });
        if let Some(scancode) = result {
            return Poll::Ready(scancode);
        }

        // Register waker for notification
        *KEYBOARD_WAKER.lock() = Some(cx.waker().clone());

        // Double-check: a scancode may have arrived between the first check
        // and waker registration (race prevention)
        let result = x86_64::instructions::interrupts::without_interrupts(|| {
            SCANCODE_QUEUE.lock().pop()
        });
        if let Some(scancode) = result {
            Poll::Ready(scancode)
        } else {
            Poll::Pending
        }
    }
}
