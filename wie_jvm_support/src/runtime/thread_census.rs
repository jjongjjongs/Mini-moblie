//! `Thread.activeCount()` as a title counts itself: its own running threads
//! plus the one its MIDlet was started on.
//!
//! The runtime answers with every thread attached to the JVM, which here takes
//! in the runtime's own - the thread that booted the JVM and the one the event
//! loop runs on. A handset counted neither.
//!
//! 드래곤아이즈 starts its music only when `activeCount() == 1`: its game loop
//! runs on the event thread through `callSerially`, so on the handset the only
//! thread it counts is that one, and a music thread still playing makes two.
//! The runtime's own threads made the answer three, so the music never started
//! and the game played in silence.
//!
//! The count here is the reference emulator's (wfeature, `builtins.go`): each
//! thread the title started and that has not finished, plus one for the main
//! thread. `start` is wrapped to note each thread it starts, and a thread
//! drops out of the count once its `alive` flag is cleared, which the runtime
//! does when `run` returns.

use alloc::boxed::Box;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use java_runtime::{RuntimeClassProto, RuntimeContext};
use jvm::{ClassInstance, ClassInstanceRef, Jvm, Result as JvmResult};

/// Stand-in for the class the bodies below belong to.
struct Thread;

/// The runtime's own `start`, renamed so the wrapper can call it.
const RUNTIME_START: &str = "wieRuntimeStart";

/// The threads `start` has started, as a `java.util.Vector`.
const STARTED: &str = "wieStartedThreads";

pub(super) fn fill_in_thread_census(mut proto: RuntimeClassProto) -> RuntimeClassProto {
    if proto.name != "java/lang/Thread" {
        return proto;
    }

    let Some(start) = proto.methods.iter_mut().find(|x| x.name == "start" && x.descriptor == "()V") else {
        return proto;
    };
    start.name = RUNTIME_START.into();

    proto
        .methods
        .push(JavaMethodProto::new("start", "()V", start_and_count, MethodAccessFlags::empty()));

    proto.methods.retain(|x| !(x.name == "activeCount" && x.descriptor == "()I"));
    proto
        .methods
        .push(JavaMethodProto::new("activeCount", "()I", active_count, MethodAccessFlags::STATIC));

    proto
        .fields
        .push(JavaFieldProto::new(STARTED, "Ljava/util/Vector;", FieldAccessFlags::STATIC));

    proto
}

async fn start_and_count(jvm: &Jvm, _: &mut RuntimeContext, this: ClassInstanceRef<Thread>) -> JvmResult<()> {
    let _: () = jvm.invoke_special(&this, "java/lang/Thread", RUNTIME_START, "()V", ()).await?;

    let started = started_threads(jvm).await?;
    let _: () = jvm.invoke_virtual(&started, "addElement", "(Ljava/lang/Object;)V", (this,)).await?;

    Ok(())
}

async fn active_count(jvm: &Jvm, _: &mut RuntimeContext) -> JvmResult<i32> {
    tracing::debug!("java.lang.Thread::activeCount()");

    let started = started_threads(jvm).await?;

    // Finished threads are dropped as they are found, so the vector holds no
    // more than the title has running.
    let mut running = 0;
    let mut index = 0;
    loop {
        let size: i32 = jvm.invoke_virtual(&started, "size", "()I", ()).await?;
        if index >= size {
            break;
        }

        let thread: Box<dyn ClassInstance> = jvm.invoke_virtual(&started, "elementAt", "(I)Ljava/lang/Object;", (index,)).await?;
        let alive: bool = jvm.get_field(&thread, "alive", "Z").await?;
        if alive {
            running += 1;
            index += 1;
        } else {
            let _: () = jvm.invoke_virtual(&started, "removeElementAt", "(I)V", (index,)).await?;
        }
    }

    Ok(running + 1)
}

async fn started_threads(jvm: &Jvm) -> JvmResult<Box<dyn ClassInstance>> {
    let started: ClassInstanceRef<()> = jvm.get_static_field("java/lang/Thread", STARTED, "Ljava/util/Vector;").await?;
    if let Some(started) = started.instance {
        return Ok(started);
    }

    let started = jvm.new_class("java/util/Vector", "()V", ()).await?;
    jvm.put_static_field("java/lang/Thread", STARTED, "Ljava/util/Vector;", started.clone())
        .await?;

    Ok(started)
}
