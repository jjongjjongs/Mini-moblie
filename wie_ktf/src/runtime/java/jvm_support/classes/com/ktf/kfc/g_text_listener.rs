use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// What a `GTextField` hands a title so it can hear about the text.
///
/// A title keeps the one its field gives it - 강호동신맞고2 holds it in a field
/// of its own - and talks to it rather than to the field.
pub struct GTextListener;

impl GTextListener {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GTextListener",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("setIMEModes", "([I)V", Self::set_ime_modes, Default::default()),
            ],
            fields: vec![JavaFieldProto::new("imeModes", "[I", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GTextListener::<init>()");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// The input modes the field will take, in the order it offers them.
    ///
    /// Kept rather than acted on: the field's own component is what runs the
    /// input method, and a title that sets this and reads nothing back is
    /// telling us which modes it wants rather than asking for anything.
    async fn set_ime_modes(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, modes: ClassInstanceRef<Array<i32>>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GTextListener::setIMEModes({:?})", &modes);

        jvm.put_field(&mut this, "imeModes", "[I", modes).await?;

        Ok(())
    }
}
