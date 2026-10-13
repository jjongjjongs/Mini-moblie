//! `cancel` for `java.util.Timer` and `java.util.TimerTask`, which the
//! runtime does not carry.
//!
//! 디지몬RPGII runs its screen effects on a `Timer` and cancels it when the
//! effect is over. The first cancel - a few minutes into the first field - died
//! on `NoSuchMethodError: java/util/Timer.cancel:()V`, and with the thread that
//! called it went the game.
//!
//! The runtime's timer thread loops for ever over a task vector and knows
//! nothing about cancelling, so its loop is replaced here by one that stops
//! when its timer is cancelled and drops a task that has been cancelled. The
//! rest of the loop is the runtime's: the same 16ms poll, the same
//! run-then-requeue for a periodic task.

use alloc::{boxed::Box, vec::Vec};
use core::time::Duration;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use java_runtime::{RuntimeClassProto, RuntimeContext};
use jvm::{ClassInstance, ClassInstanceRef, Jvm, Result as JvmResult};

/// Stand-ins for the classes the bodies below belong to.
struct Timer;
struct TimerTask;
struct TimerThread;

/// The flag `cancel` sets, on the timer's thread and on a task. Named so it
/// cannot collide with a field of a title's `TimerTask` subclass.
const CANCELLED: &str = "wieCancelled";

pub(super) fn fill_in_timer(mut proto: RuntimeClassProto) -> RuntimeClassProto {
    match proto.name {
        "java/util/Timer" => {
            add_method(
                &mut proto,
                JavaMethodProto::new("cancel", "()V", timer_cancel, MethodAccessFlags::empty()),
            );
        }
        "java/util/TimerTask" => {
            proto.fields.push(JavaFieldProto::new(CANCELLED, "Z", Default::default()));
            add_method(
                &mut proto,
                JavaMethodProto::new("cancel", "()Z", timer_task_cancel, MethodAccessFlags::empty()),
            );
        }
        "java/util/Timer$TimerThread" => {
            proto.fields.push(JavaFieldProto::new(CANCELLED, "Z", Default::default()));
            proto.methods.retain(|x| !(x.name == "run" && x.descriptor == "()V"));
            proto
                .methods
                .push(JavaMethodProto::new("run", "()V", timer_thread_run, MethodAccessFlags::empty()));
        }
        _ => {}
    }

    proto
}

/// Adds `method` unless the runtime has grown its own, which is the one to keep.
fn add_method(proto: &mut RuntimeClassProto, method: JavaMethodProto<dyn java_runtime::Runtime>) {
    if !proto.methods.iter().any(|x| x.name == method.name && x.descriptor == method.descriptor) {
        proto.methods.push(method);
    }
}

/// `Timer.cancel()`: no task of this timer runs again, including one that is
/// due, and the timer's thread ends.
async fn timer_cancel(jvm: &Jvm, _: &mut RuntimeContext, this: ClassInstanceRef<Timer>) -> JvmResult<()> {
    tracing::debug!("java.util.Timer::cancel({this:?})");

    let mut thread: Box<dyn ClassInstance> = jvm.get_field(&this, "thread", "Ljava/lang/Thread;").await?;
    jvm.put_field(&mut thread, CANCELLED, "Z", true).await?;

    let tasks: Box<dyn ClassInstance> = jvm.get_field(&this, "tasks", "Ljava/util/Vector;").await?;
    let _: () = jvm.invoke_virtual(&tasks, "removeAllElements", "()V", ()).await?;

    Ok(())
}

/// `TimerTask.cancel()`: the task does not run again.
///
/// The answer is whether this call is the one that cancelled it. CLDC's also
/// answers false for a one-shot that has already run, which this timer cannot
/// tell from one still waiting; no title seen reads the answer.
async fn timer_task_cancel(jvm: &Jvm, _: &mut RuntimeContext, mut this: ClassInstanceRef<TimerTask>) -> JvmResult<bool> {
    tracing::debug!("java.util.TimerTask::cancel({this:?})");

    let already: bool = jvm.get_field(&this, CANCELLED, "Z").await?;
    jvm.put_field(&mut this, CANCELLED, "Z", true).await?;

    Ok(!already)
}

async fn timer_thread_run(jvm: &Jvm, context: &mut RuntimeContext, this: ClassInstanceRef<TimerThread>) -> JvmResult<()> {
    tracing::debug!("java.util.Timer$TimerThread::run({this:?})");

    let java_tasks: Box<dyn ClassInstance> = jvm.get_field(&this, "tasks", "Ljava/util/Vector;").await?;

    loop {
        context.sleep(Duration::from_millis(16)).await;

        if timer_cancelled(jvm, &this).await? {
            return Ok(());
        }

        let tasks_size: i32 = jvm.invoke_virtual(&java_tasks, "size", "()I", ()).await?;
        if tasks_size == 0 {
            continue;
        }

        // Taken out of the vector while they run, so a task that schedules
        // another does not change the vector under this loop.
        let mut tasks: Vec<Box<dyn ClassInstance>> = Vec::with_capacity(tasks_size as _);
        for _ in 0..tasks_size {
            tasks.push(jvm.invoke_virtual(&java_tasks, "remove", "(I)Ljava/lang/Object;", (0,)).await?);
        }

        let now = context.now() as i64;
        let mut next_tasks = Vec::new();
        for mut task in tasks {
            let cancelled: bool = jvm.get_field(&task, CANCELLED, "Z").await?;
            if cancelled {
                continue;
            }

            let next_execution_time: i64 = jvm.get_field(&task, "nextExecutionTime", "J").await?;
            if next_execution_time >= now {
                next_tasks.push(task);
                continue;
            }

            let _: () = jvm.invoke_virtual(&task, "run", "()V", ()).await?;

            // A task may cancel its own timer, or itself, from `run`.
            if timer_cancelled(jvm, &this).await? {
                return Ok(());
            }
            let cancelled: bool = jvm.get_field(&task, CANCELLED, "Z").await?;
            let period: i64 = jvm.get_field(&task, "period", "J").await?;
            if period > 0 && !cancelled {
                jvm.put_field(&mut task, "nextExecutionTime", "J", now + period).await?;
                next_tasks.push(task);
            }
        }

        for task in next_tasks {
            let _: () = jvm.invoke_virtual(&java_tasks, "addElement", "(Ljava/lang/Object;)V", (task,)).await?;
        }
    }
}

async fn timer_cancelled(jvm: &Jvm, this: &ClassInstanceRef<TimerThread>) -> JvmResult<bool> {
    jvm.get_field(this, CANCELLED, "Z").await
}
