use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// KTF's own top-level form, which is an LWC shell under another name.
///
/// 강호동신맞고2 is what asks for it: it loads the class at start-up and
/// dies with `java.lang.Error` out of `startApp` when nothing answers. Every
/// method it has taken so far is one `ShellComponent` already has - the
/// `(IIII)V` constructor among them - so nothing is written here. A title that
/// asks for more fails by name, which is the evidence needed to serve it.
pub struct GForm;

impl GForm {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GForm",
            parent_class: Some("org/kwis/msp/lwc/ShellComponent"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("<init>", "()V", Self::init, Default::default())],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GForm::<init>()");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lwc/ShellComponent", "<init>", "()V", ()).await?;

        Ok(())
    }
}
