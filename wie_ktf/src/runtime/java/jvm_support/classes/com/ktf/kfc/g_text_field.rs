use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::runtime::java::jvm_support::classes::com::ktf::kfc::{GMenubarForm, GTextListener};

/// KTF's own text field: a `TextFieldComponent` that puts itself on the form it
/// is given.
///
/// 강호동신맞고2 is what asks for it - its name entry is a `GMenubarForm` with
/// one of these on it - and the archive ships no class file for it, so a title
/// that names it finds nothing unless the platform publishes it.
pub struct GTextField;

impl GTextField {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GTextField",
            parent_class: Some("org/kwis/msp/lwc/TextFieldComponent"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new(
                    "<init>",
                    "(Lcom/ktf/kfc/GMenubarForm;Ljava/lang/String;I)V",
                    Self::init,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "getGTextListener",
                    "()Lcom/ktf/kfc/GTextListener;",
                    Self::get_g_text_listener,
                    Default::default(),
                ),
            ],
            fields: vec![JavaFieldProto::new("listener", "Lcom/ktf/kfc/GTextListener;", Default::default())],
            access_flags: Default::default(),
        }
    }

    /// The form it belongs to, the text it starts with, and how much it takes.
    async fn init(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        this: ClassInstanceRef<Self>,
        form: ClassInstanceRef<GMenubarForm>,
        text: ClassInstanceRef<String>,
        max_length: i32,
    ) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GTextField::<init>({:?}, {max_length})", &form);

        let _: () = jvm
            .invoke_special(
                &this,
                "org/kwis/msp/lwc/TextFieldComponent",
                "<init>",
                "(Ljava/lang/String;I)V",
                (text, max_length),
            )
            .await?;

        // A field that is not on its form is a field nobody can reach: the form
        // is what lays it out, focuses it and sends it keys.
        if !form.is_null() {
            let _: i32 = jvm
                .invoke_virtual(&form, "addComponent", "(Lorg/kwis/msp/lwc/Component;)I", (this.clone(),))
                .await?;
        }

        Ok(())
    }

    /// The listener this field talks through, made on the first ask and kept.
    ///
    /// A title compares the one it is holding with the one it is given -
    /// handing out a new object each time would make every comparison false.
    async fn get_g_text_listener(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<GTextListener>> {
        tracing::debug!("com.ktf.kfc.GTextField::getGTextListener()");

        let existing: ClassInstanceRef<GTextListener> = jvm.get_field(&this, "listener", "Lcom/ktf/kfc/GTextListener;").await?;
        if !existing.is_null() {
            return Ok(existing);
        }

        let listener = jvm.new_class("com/ktf/kfc/GTextListener", "()V", ()).await?;
        jvm.put_field(&mut this, "listener", "Lcom/ktf/kfc/GTextListener;", listener.clone())
            .await?;

        Ok(listener.into())
    }
}
