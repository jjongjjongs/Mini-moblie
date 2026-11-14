use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// KTF's own modal message box.
///
/// 강호동신맞고2 names it in its constant pool - beside a
/// `(Lorg/kwis/msp/lcdui/Display;IIIIZ)V` constructor and a `doModal()I` - but
/// has not reached the screen that uses it in any capture, so neither is
/// written here on a guess. It is published so the class loads; a title that
/// asks for a method of it fails by name, and the name is what says what to
/// write.
pub struct GMsgBox;

impl GMsgBox {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GMsgBox",
            parent_class: Some("com/ktf/kfc/GForm"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("<init>", "()V", Self::init, Default::default())],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GMsgBox::<init>()");

        let _: () = jvm.invoke_special(&this, "com/ktf/kfc/GForm", "<init>", "()V", ()).await?;

        Ok(())
    }
}
