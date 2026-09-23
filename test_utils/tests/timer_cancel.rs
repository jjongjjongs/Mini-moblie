//! `Timer.cancel` and `TimerTask.cancel`, which the runtime lacks and
//! `wie_jvm_support` fills in. 디지몬RPGII cancels a timer when a screen
//! effect is over and died on the missing method.

use std::sync::atomic::{AtomicUsize, Ordering};

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use test_utils::run_jvm_test;
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

static TIMER_RUNS: AtomicUsize = AtomicUsize::new(0);
static TASK_RUNS: AtomicUsize = AtomicUsize::new(0);

/// A `TimerTask` that counts its runs, in `TIMER_RUNS` or `TASK_RUNS`.
struct CountingTask<const TIMER: bool>;

impl<const TIMER: bool> CountingTask<TIMER> {
    fn proto(name: &'static str) -> WieJavaClassProto {
        WieJavaClassProto {
            name,
            parent_class: Some("java/util/TimerTask"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("run", "()V", Self::run, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        jvm.invoke_special(&this, "java/util/TimerTask", "<init>", "()V", ()).await
    }

    async fn run(_: &Jvm, _: &mut WieJvmContext, _: ClassInstanceRef<Self>) -> JvmResult<()> {
        if TIMER { &TIMER_RUNS } else { &TASK_RUNS }.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

async fn sleep(jvm: &Jvm, millis: i64) -> JvmResult<()> {
    jvm.invoke_static("java/lang/Thread", "sleep", "(J)V", (millis,)).await
}

#[test]
fn a_cancelled_timer_runs_nothing_more() -> wie_util::Result<()> {
    let protos = vec![CountingTask::<true>::proto("test/TimerCounted")];

    run_jvm_test(Box::new([protos.into_boxed_slice()]), |jvm| async move {
        let timer = jvm.new_class("java/util/Timer", "()V", ()).await?;
        let task = jvm.new_class("test/TimerCounted", "()V", ()).await?;
        let _: () = jvm
            .invoke_virtual(&timer, "scheduleAtFixedRate", "(Ljava/util/TimerTask;JJ)V", (task, 0i64, 10i64))
            .await?;

        sleep(&jvm, 300).await?;
        let before = TIMER_RUNS.load(Ordering::SeqCst);
        assert!(before > 0, "the task never ran");

        let _: () = jvm.invoke_virtual(&timer, "cancel", "()V", ()).await?;
        let after_cancel = TIMER_RUNS.load(Ordering::SeqCst);

        sleep(&jvm, 300).await?;
        assert_eq!(TIMER_RUNS.load(Ordering::SeqCst), after_cancel);

        Ok(())
    })
}

#[test]
fn a_cancelled_task_runs_no_more_and_says_so_once() -> wie_util::Result<()> {
    let protos = vec![CountingTask::<false>::proto("test/TaskCounted")];

    run_jvm_test(Box::new([protos.into_boxed_slice()]), |jvm| async move {
        let timer = jvm.new_class("java/util/Timer", "()V", ()).await?;
        let task = jvm.new_class("test/TaskCounted", "()V", ()).await?;
        let _: () = jvm
            .invoke_virtual(&timer, "scheduleAtFixedRate", "(Ljava/util/TimerTask;JJ)V", (task.clone(), 0i64, 10i64))
            .await?;

        sleep(&jvm, 300).await?;
        assert!(TASK_RUNS.load(Ordering::SeqCst) > 0, "the task never ran");

        let first: bool = jvm.invoke_virtual(&task, "cancel", "()Z", ()).await?;
        let second: bool = jvm.invoke_virtual(&task, "cancel", "()Z", ()).await?;
        assert!(first);
        assert!(!second);

        let after_cancel = TASK_RUNS.load(Ordering::SeqCst);
        sleep(&jvm, 300).await?;
        assert_eq!(TASK_RUNS.load(Ordering::SeqCst), after_cancel);

        Ok(())
    })
}
