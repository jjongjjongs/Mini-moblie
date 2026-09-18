use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use hashbrown::HashMap;
use spin::Mutex;

use wie_util::{Result, WieError};

use crate::time::Instant;

type Task = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

pub struct ExecutorInner {
    current_task_id: Option<usize>,
    tasks: HashMap<usize, Task>,
    sleeping_tasks: HashMap<usize, Instant>,
    /// Scratch space for [`Executor::step`]'s poll order, kept here so a step
    /// does not have to allocate one.
    poll_order: Vec<usize>,
    last_task_id: usize,
    last_now: Instant,
}

pub trait AsyncCallable<R>: Send
where
    R: Send,
{
    fn call(self) -> impl Future<Output = R> + Send;
}

impl<F, R, Fut> AsyncCallable<R> for F
where
    F: FnOnce() -> Fut + 'static + Send,
    R: AsyncCallableResult,
    Fut: Future<Output = R> + 'static + Send,
{
    async fn call(self) -> R {
        self().await
    }
}

pub trait AsyncCallableResult: Send {
    fn err(self) -> Option<WieError>;
}

impl<R> AsyncCallableResult for core::result::Result<R, WieError>
where
    R: Send,
{
    fn err(self) -> Option<WieError> {
        self.err()
    }
}

impl AsyncCallableResult for () {
    fn err(self) -> Option<WieError> {
        None
    }
}

#[derive(Clone)]
pub struct Executor {
    inner: Arc<Mutex<ExecutorInner>>,
}

impl Executor {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(ExecutorInner {
            current_task_id: None,
            tasks: HashMap::new(),
            sleeping_tasks: HashMap::new(),
            poll_order: Vec::new(),
            last_task_id: 0,
            last_now: Instant::from_epoch_millis(0),
        }));

        Self { inner }
    }

    pub fn spawn<C, R>(&self, callable: C) -> usize
    where
        C: AsyncCallable<R> + 'static,
        R: AsyncCallableResult,
    {
        let fut = async move {
            let result = callable.call().await;
            if let Some(err) = result.err() {
                return Err(err);
            }

            Ok(())
        };

        let task_id = {
            let mut inner = self.inner.lock();
            inner.last_task_id += 1;
            inner.last_task_id
        };

        self.inner.lock().tasks.insert(task_id, Box::pin(fut));

        task_id
    }

    // TODO we need to remove error handling from here. we need to JoinHandle like on spawn..
    pub fn tick<T>(&mut self, now: T) -> Result<()>
    where
        T: Fn() -> Instant,
    {
        let end = now() + 8; // TODO hardcoded
        loop {
            let now = now();

            if now > end {
                break;
            }

            {
                let inner = self.inner.lock();
                let running_task_count = inner.tasks.len() - inner.sleeping_tasks.len();
                if running_task_count == 0 && !inner.sleeping_tasks.is_empty() {
                    let next_wakeup = *inner.sleeping_tasks.values().min().unwrap();
                    if now < next_wakeup {
                        break;
                    }
                }
            }

            self.step(now)?;
        }

        Ok(())
    }

    pub fn current_task_id(&self) -> u64 {
        self.inner.lock().current_task_id.unwrap() as _
    }

    /// Whether every task is asleep with its wake-up still in the future — i.e.
    /// there is nothing to run until a timer fires. This is exactly the
    /// condition [`tick`](Self::tick) breaks its inner loop on; exposing it lets
    /// the host stop spinning a budget out and sleep until real work is due,
    /// instead of busy-waiting through the idle remainder of every tick.
    pub fn is_idle(&self) -> bool {
        let inner = self.inner.lock();
        let running = inner.tasks.len() - inner.sleeping_tasks.len();
        running == 0 && inner.sleeping_tasks.values().min().is_some_and(|&wakeup| inner.last_now < wakeup)
    }

    fn step(&mut self, now: Instant) -> Result<()> {
        // Polling a task re-enters the executor - `sleep` and `spawn` both take the
        // lock - so a task has to be out of the map while it is polled. It is taken
        // out one at a time: draining the whole map and rebuilding it would tear down
        // and rehash every task on every step, and a step happens many times per tick.
        let mut poll_order = {
            let mut inner = self.inner.lock();
            inner.last_now = now;

            let mut poll_order = core::mem::take(&mut inner.poll_order);
            poll_order.extend(inner.tasks.keys().copied());

            poll_order
        };

        let mut first_error = None;
        let waker = self.create_waker();

        for &task_id in poll_order.iter() {
            // Everything the poll needs is settled under one lock: whether the
            // task is still asleep, the task itself, and whose task it is.
            let mut task = {
                let mut inner = self.inner.lock();

                match inner.sleeping_tasks.get(&task_id) {
                    Some(&until) if until > now => continue,
                    Some(_) => {
                        inner.sleeping_tasks.remove(&task_id);
                    }
                    None => {}
                }

                match inner.tasks.remove(&task_id) {
                    Some(task) => {
                        inner.current_task_id = Some(task_id);
                        task
                    }
                    // Gone since the order was taken - a task another one finished off.
                    None => continue,
                }
            };

            let mut context = Context::from_waker(&waker);

            let poll = task.as_mut().poll(&mut context);

            // `inner` is declared after `task`, so the lock is released before a
            // finished task is dropped - a drop that reached back into the executor
            // would deadlock on it otherwise.
            let mut inner = self.inner.lock();
            inner.current_task_id = None;

            match poll {
                Poll::Ready(result) => {
                    // A task that finished while asleep takes its wake-up with it.
                    // Left behind, it would count as a sleeper with no task forever.
                    inner.sleeping_tasks.remove(&task_id);

                    if let Err(err) = result
                        && first_error.is_none()
                    {
                        first_error = Some(err);
                    }
                }
                Poll::Pending => {
                    inner.tasks.insert(task_id, task);
                }
            }
        }

        poll_order.clear();
        self.inner.lock().poll_order = poll_order;

        if let Some(err) = first_error { Err(err) } else { Ok(()) }
    }

    pub(crate) fn sleep(&self, timeout: u64) {
        let task_id = self.inner.lock().current_task_id.unwrap();

        let until = self.inner.lock().last_now + timeout;
        self.inner.lock().sleeping_tasks.insert(task_id, until);
    }

    fn create_waker(&self) -> Waker {
        unsafe fn noop_clone(_data: *const ()) -> RawWaker {
            noop_raw_waker()
        }

        unsafe fn noop(_data: *const ()) {}

        const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);

        const fn noop_raw_waker() -> RawWaker {
            RawWaker::new(core::ptr::null(), &NOOP_WAKER_VTABLE)
        }

        unsafe { Waker::from_raw(noop_raw_waker()) }
    }
}

#[cfg(test)]
mod tests {
    use alloc::sync::Arc;
    use core::{
        cell::Cell,
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicBool, Ordering},
        task::{Context, Poll},
    };

    use wie_util::WieError;

    use super::Executor;
    use crate::time::Instant;

    struct YieldOnce(bool);

    impl Future for YieldOnce {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                Poll::Pending
            }
        }
    }

    fn advancing_clock(start: u64) -> impl Fn() -> Instant {
        let time = Cell::new(start);
        move || {
            let now = time.get();
            time.set(now + 1);
            Instant::from_epoch_millis(now)
        }
    }

    #[test]
    fn test_failed_task_preserves_others() {
        let mut executor = Executor::new();

        executor.spawn(|| async { Err::<(), _>(WieError::FatalError("test error".into())) });

        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = completed.clone();
        executor.spawn(move || async move {
            YieldOnce(false).await;
            completed_clone.store(true, Ordering::Relaxed);
        });

        assert!(executor.tick(advancing_clock(0)).is_err());
        assert!(!completed.load(Ordering::Relaxed));

        executor.tick(advancing_clock(100)).unwrap();
        assert!(completed.load(Ordering::Relaxed));
    }

    #[test]
    fn test_failed_task_preserves_sleeping_tasks() {
        let mut executor = Executor::new();

        let completed = Arc::new(AtomicBool::new(false));
        let completed_clone = completed.clone();
        let executor_clone = executor.clone();
        executor.spawn(move || async move {
            executor_clone.sleep(100);
            YieldOnce(false).await;
            completed_clone.store(true, Ordering::Relaxed);
        });

        executor.spawn(|| async { Err::<(), _>(WieError::FatalError("test error".into())) });

        assert!(executor.tick(advancing_clock(0)).is_err());
        assert!(!completed.load(Ordering::Relaxed));

        executor.tick(advancing_clock(50)).unwrap();
        assert!(!completed.load(Ordering::Relaxed));

        executor.tick(advancing_clock(200)).unwrap();
        assert!(completed.load(Ordering::Relaxed));
    }

    #[test]
    fn a_finished_task_leaves_no_wakeup_behind() {
        let mut executor = Executor::new();

        // A task is free to finish while it still has a sleep pending. The wake-up
        // has to go with it: `tick` counts the runnable tasks by taking the sleepers
        // off the total, and a sleeper with no task makes that count nonsense.
        let executor_clone = executor.clone();
        executor.spawn(move || async move {
            executor_clone.sleep(100);
        });

        executor.tick(advancing_clock(0)).unwrap();
        executor.tick(advancing_clock(200)).unwrap();

        assert!(!executor.is_idle());
    }

    #[test]
    fn test_all_ok_tasks_complete() {
        let mut executor = Executor::new();

        let completed_a = Arc::new(AtomicBool::new(false));
        let completed_a_clone = completed_a.clone();
        executor.spawn(move || async move {
            completed_a_clone.store(true, Ordering::Relaxed);
        });

        let completed_b = Arc::new(AtomicBool::new(false));
        let completed_b_clone = completed_b.clone();
        executor.spawn(move || async move {
            YieldOnce(false).await;
            completed_b_clone.store(true, Ordering::Relaxed);
        });

        executor.tick(advancing_clock(0)).unwrap();

        assert!(completed_a.load(Ordering::Relaxed));
        assert!(completed_b.load(Ordering::Relaxed));
    }
}
