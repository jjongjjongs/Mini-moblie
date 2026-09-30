use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    future::Future,
    sync::atomic::{AtomicBool, Ordering},
};

use jvm::{Jvm, Result as JvmResult};

use wie_backend::{DefaultTaskRunner, System};
use wie_jvm_support::{JvmSupport, RustJavaJvmImplementation, WieJavaClassProto};
use wie_util::{Result, WieError};

use crate::TestPlatform;

// TODO macro?
pub fn run_jvm_test<T, F>(protos: Box<[Box<[WieJavaClassProto]>]>, func: T) -> Result<()>
where
    T: FnOnce(Jvm) -> F + Send + 'static,
    F: Future<Output = JvmResult<()>> + Send,
{
    run_jvm_test_with_files(protos, &[], func)
}

/// [`run_jvm_test`] with the archive's own files mounted before the JVM starts,
/// the way an emulator mounts the files that sit beside a title's `.jar`. A
/// test about what a title reads out of its archive needs them there first.
pub fn run_jvm_test_with_files<T, F>(protos: Box<[Box<[WieJavaClassProto]>]>, files: &[(&str, Vec<u8>)], func: T) -> Result<()>
where
    T: FnOnce(Jvm) -> F + Send + 'static,
    F: Future<Output = JvmResult<()>> + Send,
{
    run_jvm_test_inner("", protos, files, func)
}

/// [`run_jvm_test`] with the running title's application id set, the way an
/// emulator sets it from the archive's descriptor. A test about behaviour a
/// platform class gives only to a named title - a per-`aid` answer - needs it.
pub fn run_jvm_test_with_aid<T, F>(aid: &str, protos: Box<[Box<[WieJavaClassProto]>]>, func: T) -> Result<()>
where
    T: FnOnce(Jvm) -> F + Send + 'static,
    F: Future<Output = JvmResult<()>> + Send,
{
    run_jvm_test_inner(aid, protos, &[], func)
}

fn run_jvm_test_inner<T, F>(aid: &str, protos: Box<[Box<[WieJavaClassProto]>]>, files: &[(&str, Vec<u8>)], func: T) -> Result<()>
where
    T: FnOnce(Jvm) -> F + Send + 'static,
    F: Future<Output = JvmResult<()>> + Send,
{
    let mut system = System::new(Box::new(TestPlatform::new()), "", aid, DefaultTaskRunner);

    for (name, data) in files {
        system.filesystem().add_virtual(name, data.clone());
    }

    let done = Arc::new(AtomicBool::new(false));
    let done_clone = done.clone();
    let system_clone = system.clone();

    system.spawn(async move || {
        let jvm = JvmSupport::new_jvm(&system_clone, None, protos, &[], RustJavaJvmImplementation).await?;
        func(jvm).await.unwrap();

        done_clone.store(true, Ordering::Relaxed);

        Ok::<_, WieError>(())
    });

    loop {
        system.tick()?;
        if done.load(Ordering::Relaxed) {
            break;
        }
    }

    Ok(())
}
