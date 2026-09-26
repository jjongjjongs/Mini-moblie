use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class javax.microedition.rms.RecordStoreNotFoundException
//
// Thrown by openRecordStore when it is told not to create a store and there is
// none. It is its own type, not the plain RecordStoreException, because a title
// catches it on its own to tell "no save yet" apart from a real store failure:
// 크레이지버스 opens `CrazyBus` with create=false, catches this to open it again
// with create=true and write its defaults, and left it as the base exception it
// fell through to a handler that never created the store and then closed a null
// one.
pub struct RecordStoreNotFoundException;

impl RecordStoreNotFoundException {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/rms/RecordStoreNotFoundException",
            parent_class: Some("javax/microedition/rms/RecordStoreException"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init_with_message, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<()> {
        tracing::debug!("javax.microedition.rms.RecordStoreNotFoundException::<init>({this:?})");

        let _: () = jvm
            .invoke_special(&this, "javax/microedition/rms/RecordStoreException", "<init>", "()V", ())
            .await?;

        Ok(())
    }

    async fn init_with_message(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, message: ClassInstanceRef<String>) -> Result<()> {
        tracing::debug!("javax.microedition.rms.RecordStoreNotFoundException::<init>({this:?}, {message:?})");

        let _: () = jvm
            .invoke_special(
                &this,
                "javax/microedition/rms/RecordStoreException",
                "<init>",
                "(Ljava/lang/String;)V",
                (message,),
            )
            .await?;

        Ok(())
    }
}
