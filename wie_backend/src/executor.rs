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
    /// Tasks that have yielded during the step now running. See
    /// [`Executor::note_yield`].
    yielded_tasks: Vec<usize>,
    /// Whether the last step did nothing but pass the CPU around. See
    /// [`Executor::note_yield`].
    last_step_only_yielded: bool,
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
            yielded_tasks: Vec::new(),
            last_step_only_yielded: false,
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

            // A step that only passed the CPU around will do the same again for
            // the rest of the budget. See `note_yield`.
            if self.inner.lock().last_step_only_yielded {
                break;
            }
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

        // Or every task that is awake is only spinning, which is idle in every
        // way that matters to a host deciding whether to sleep. See
        // `note_yield`.
        if inner.last_step_only_yielded {
            return true;
        }

        let running = inner.tasks.len() - inner.sleeping_tasks.len();
        running == 0 && inner.sleeping_tasks.values().min().is_some_and(|&wakeup| inner.last_now < wakeup)
    }

    /// How long until there is work again, in milliseconds, when there is none
    /// now.
    ///
    /// [`is_idle`](Self::is_idle) says only *that* the host may stop; this says
    /// how long it may stop for. A host that polls a fixed interval instead has
    /// to pay one poll's overhead for every interval a title's timer spans, and
    /// a title that asks for the next frame in 15ms was waiting 33 for it -
    /// several poll cycles, each draining audio and asking the emulator its
    /// state again, before one of them happened to land past the wake-up.
    ///
    /// `None` when the host should keep to its own interval: something is
    /// runnable, nothing is scheduled at all, or the tasks are spinning rather
    /// than sleeping - a spin has no wake-up to wait for, and the host's
    /// interval is what keeps it from pegging the thread.
    pub fn idle_for(&self, now: Instant) -> Option<u64> {
        let inner = self.inner.lock();

        if inner.last_step_only_yielded {
            return None;
        }

        let running = inner.tasks.len() - inner.sleeping_tasks.len();
        if running != 0 {
            return None;
        }

        let wakeup = *inner.sleeping_tasks.values().min()?;

        (wakeup > now).then(|| wakeup - now)
    }

    fn step(&mut self, now: Instant) -> Result<()> {
        // Polling a task re-enters the executor - `sleep` and `spawn` both take the
        // lock - so a task has to be out of the map while it is polled. It is taken
        // out one at a time: draining the whole map and rebuilding it would tear down
        // and rehash every task on every step, and a step happens many times per tick.
        let (mut poll_order, task_count) = {
            let mut inner = self.inner.lock();
            inner.last_now = now;
            inner.yielded_tasks.clear();
            inner.last_step_only_yielded = false;

            let mut poll_order = core::mem::take(&mut inner.poll_order);
            poll_order.extend(inner.tasks.keys().copied());

            // In spawn order, which is what a task id counts. The map's own
            // order would do as well for fairness - every task is polled once
            // either way, and only who goes first in a step differs - but it is
            // a hash order over a randomly seeded hasher, so it differs between
            // runs of the same binary over the same archive.
            //
            // A guest thread's turn then lands wherever that seed put it, and a
            // title whose threads race gets a different answer each launch.
            // 서울타이쿤2 does: `startApp` pushes its card, the card's
            // `showNotify` queues a repaint, and the tick's budget runs out
            // right there. Whether the next step resumes the title's own thread
            // - which goes on to create the image its `paint` draws - or the
            // event thread that drains that repaint first decided whether the
            // title started or died on `NullPointerException: image is null`,
            // about half the launches either way.
            //
            // Spawn order also puts the main thread ahead of anything it
            // started, which is the order a title is written expecting.
            poll_order.sort_unstable();

            let task_count = inner.tasks.len();

            (poll_order, task_count)
        };
        let mut polled = 0usize;

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

            polled += 1;
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
        {
            let mut inner = self.inner.lock();
            inner.poll_order = poll_order;

            // Only when every task that ran did nothing but yield, and no task
            // appeared while they did - one that has not been polled yet is
            // work waiting to happen.
            inner.last_step_only_yielded = polled > 0 && inner.yielded_tasks.len() == polled && inner.tasks.len() <= task_count;
        }

        if let Some(err) = first_error { Err(err) } else { Ok(()) }
    }

    /// Records that the task now running is yielding - handing the CPU on
    /// rather than waiting for anything this executor can deliver.
    ///
    /// A step in which every task polled did only this made no progress, and
    /// stepping again cannot change that: what those tasks are waiting for is a
    /// sleeping task's timer, or something the host will bring in. So such a
    /// step ends the tick and reads as idle, and the host sleeps its budget
    /// instead of spinning it out.
    ///
    /// A title that waits by spinning is otherwise indistinguishable from one
    /// doing work. 판타지포에버2 waits that way - a bare `while (...) yield();`
    /// game thread, fifty thousand turns a second - and held a Y700's CPU at
    /// its top clock for as long as it ran.
    pub(crate) fn note_yield(&self) {
        let mut inner = self.inner.lock();
        if let Some(task_id) = inner.current_task_id
            && !inner.yielded_tasks.contains(&task_id)
        {
            inner.yielded_tasks.push(task_id);
        }
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
    use alloc::{sync::Arc, vec::Vec};
    use core::{
        cell::Cell,
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        task::{Context, Poll},
    };

    use wie_util::WieError;

    use spin::Mutex;

    use super::Executor;
    use crate::{task::YieldFuture, time::Instant};

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

    /// A title that waits by spinning does not hold the host's CPU.
    ///
    /// 판타지포에버2's game thread is a bare `while (...) yield();` - fifty
    /// thousand turns a second in a capture, with nothing else between them.
    /// Turning them as fast as the host can is no more progress than turning
    /// one, so a step that only passes the CPU around ends the tick and reads
    /// as idle, and the host sleeps the rest of its budget.
    #[test]
    fn a_tick_stops_once_every_task_is_only_spinning() {
        let mut executor = Executor::new();

        let turns = Arc::new(AtomicUsize::new(0));
        let spinner = executor.clone();
        let counted = turns.clone();
        executor.spawn(move || async move {
            for _ in 0..10_000 {
                counted.fetch_add(1, Ordering::Relaxed);
                YieldFuture::waiting(&spinner).await;
            }
        });

        // The clock advances a millisecond per read, so a tick that does not
        // stop early runs its whole 8ms budget out.
        executor.tick(advancing_clock(0)).unwrap();

        assert_eq!(turns.load(Ordering::Relaxed), 1, "spinning is not a reason to keep stepping");
        assert!(executor.is_idle(), "a task that is only spinning leaves the host free to sleep");
    }

    /// And a task that is doing something keeps the tick going, however much
    /// another one spins beside it.
    #[test]
    fn a_tick_keeps_going_while_a_task_is_working() {
        let mut executor = Executor::new();

        let spinner = executor.clone();
        executor.spawn(move || async move {
            for _ in 0..10_000 {
                YieldFuture::waiting(&spinner).await;
            }
        });

        let turns = Arc::new(AtomicUsize::new(0));
        let counted = turns.clone();
        executor.spawn(move || async move {
            for _ in 0..10_000 {
                counted.fetch_add(1, Ordering::Relaxed);
                YieldOnce(false).await;
            }
        });

        executor.tick(advancing_clock(0)).unwrap();

        assert!(turns.load(Ordering::Relaxed) > 1, "the working task has to keep being polled");
        assert!(!executor.is_idle());
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

    /// The host is told how long the wait is, not just that there is one.
    ///
    /// A fixed poll interval pays one poll per interval the wait spans, and the
    /// wake-up lands late by whatever is left over. 제노니아2 asks for its next
    /// frame in 15ms and was waiting 33 for it.
    #[test]
    fn an_idle_executor_says_how_long_it_is_idle_for() {
        let mut executor = Executor::new();

        // The sleep registers the wake-up; the yield after it is what leaves the
        // task in the map to be woken, rather than finishing and taking its
        // wake-up with it.
        let executor_clone = executor.clone();
        executor.spawn(move || async move {
            executor_clone.sleep(100);
            YieldOnce(false).await;
        });

        executor.tick(advancing_clock(0)).unwrap();

        // The test clock advances a millisecond per read, so the sleep was
        // asked for at 1 and runs to 101: at 60 there are 41 left to wait.
        assert!(executor.is_idle());
        assert_eq!(executor.idle_for(Instant::from_epoch_millis(60)), Some(41));

        // At the wake-up and past it there is nothing left to wait for, and the
        // host is told to come back on its own terms rather than to sleep zero.
        assert_eq!(executor.idle_for(Instant::from_epoch_millis(101)), None);
        assert_eq!(executor.idle_for(Instant::from_epoch_millis(140)), None);
    }

    /// A task that is spinning rather than sleeping has no wake-up to wait for,
    /// so the host keeps to its own interval - which is what stops a spin from
    /// pegging the thread. It is still idle, which is a different question.
    #[test]
    fn a_spinning_task_offers_no_wait() {
        let mut executor = Executor::new();

        let spinner = executor.clone();
        executor.spawn(move || async move {
            for _ in 0..10_000 {
                YieldFuture::waiting(&spinner).await;
            }
        });

        executor.tick(advancing_clock(0)).unwrap();

        assert!(executor.is_idle());
        assert_eq!(executor.idle_for(Instant::from_epoch_millis(0)), None);
    }

    /// Threads take their turn in the order they were spawned, every step.
    ///
    /// The poll order used to be a `HashMap`'s, over a randomly seeded hasher,
    /// so which guest thread went first was drawn afresh on every launch. A
    /// title whose threads race got a different answer each time: 서울타이쿤2
    /// started or died on `image is null` about half the launches, depending on
    /// whether its own thread or the event thread resumed after the tick that
    /// queued its first repaint.
    #[test]
    fn threads_take_their_turn_in_the_order_they_were_spawned() {
        let mut executor = Executor::new();

        let turns = Arc::new(Mutex::new(Vec::new()));

        // Enough of them that a hash order matching this one by chance is not
        // what a passing run means.
        for id in 0..8 {
            let seen = turns.clone();
            executor.spawn(move || async move {
                seen.lock().push(id);
                YieldOnce(false).await;

                Ok::<_, WieError>(())
            });
        }

        executor.tick(advancing_clock(0)).unwrap();

        assert_eq!(*turns.lock(), (0..8).collect::<Vec<_>>());
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
