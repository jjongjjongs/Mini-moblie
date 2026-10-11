use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class org.kwis.msp.media.MediaUnsupportedException
pub struct MediaUnsupportedException;

impl MediaUnsupportedException {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            // Checked, not guessed: the handset's own table has this one extending
            // Exception where its sibling MediaUnavailableException extends
            // RuntimeException. A title that catches it by the checked type would
            // miss it if we made it unchecked.
            name: "org/kwis/msp/media/MediaUnsupportedException",
            parent_class: Some("java/lang/Exception"),
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
        tracing::debug!("org.kwis.msp.media.MediaUnsupportedException::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Exception", "<init>", "()V", ()).await?;

        Ok(())
    }

    async fn init_with_message(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, message: ClassInstanceRef<String>) -> Result<()> {
        tracing::debug!("org.kwis.msp.media.MediaUnsupportedException::<init>({this:?}, {message:?})");

        let _: () = jvm
            .invoke_special(&this, "java/lang/Exception", "<init>", "(Ljava/lang/String;)V", (message,))
            .await?;

        Ok(())
    }
}
