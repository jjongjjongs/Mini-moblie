use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::executor::Executor;

#[derive(Default)]
pub struct YieldFuture {
    polled: bool,
}

impl YieldFuture {
    /// Hands the CPU to the executor's other tasks.
    ///
    /// This is the emulator's own handoff - a point where it knows another task
    /// should get a turn, such as the moment a monitor is released. It says
    /// nothing about what the title is doing, so it does not count as waiting.
    pub fn new() -> Self {
        Self { polled: false }
    }

    /// The same handoff, from a title that is waiting by spinning.
    ///
    /// A title that waits this way is otherwise indistinguishable from one
    /// doing work, and holds a host CPU for as long as it waits.
    /// 판타지포에버2's game thread is a bare `while (...) yield();` - fifty
    /// thousand turns a second in a capture, with nothing else between them.
    /// Turning them as fast as the host can is no more progress than turning
    /// one, so the executor is told, and a step that does nothing else ends the
    /// tick. See [`Executor::note_yield`].
    pub fn waiting(executor: &Executor) -> Self {
        executor.note_yield();

        Self { polled: false }
    }
}

impl Future for YieldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.polled {
            self.polled = true;
            cx.waker().wake_by_ref(); // signal executor to poll again

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

impl Unpin for YieldFuture {}

pub struct SleepFuture {
    polled: bool,
}

impl SleepFuture {
    pub fn new(timeout: u64, executor: &Executor) -> Self {
        // we need executor from outside before rust `context_ext` stabilization
        executor.sleep(timeout);

        Self { polled: false }
    }
}

impl Future for SleepFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.polled {
            self.polled = true;

            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

impl Unpin for SleepFuture {}
