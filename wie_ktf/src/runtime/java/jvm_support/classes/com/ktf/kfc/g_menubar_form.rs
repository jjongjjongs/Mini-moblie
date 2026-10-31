use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// A `GForm` with a menu bar, which is what a KTF title builds an input
/// screen on.
///
/// Named because `GTextField`'s constructor takes one: 강호동신맞고2 puts its
/// text field on one of these. Nothing has asked for a method of its own yet,
/// so it is its parent under another name.
pub struct GMenubarForm;

impl GMenubarForm {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GMenubarForm",
            parent_class: Some("com/ktf/kfc/GForm"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("<init>", "()V", Self::init, Default::default())],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GMenubarForm::<init>()");

        let _: () = jvm.invoke_special(&this, "com/ktf/kfc/GForm", "<init>", "()V", ()).await?;

        Ok(())
    }
}
