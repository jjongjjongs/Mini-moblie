use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// KTF's own progress-bar widget, part of `com.ktf.kfc` (KTF Foundation
/// Classes) beside `GForm` and the rest. The archive ships no class file for
/// it, so a title that names it - 테일즈 판타지 lists it among the component
/// classes it loads by reflection - finds nothing unless the platform publishes
/// it, and the missing class takes the run down before its first screen.
///
/// Published so the class loads; only the members a title is seen to ask for
/// are served, and a title that reaches for more fails by name, which says what
/// to add next.
pub struct GProgressBar;

impl GProgressBar {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/ktf/kfc/GProgressBar",
            parent_class: Some("org/kwis/msp/lwc/Component"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("setMaximum", "(I)Z", Self::set_maximum, Default::default()),
                JavaMethodProto::new("setValue", "(I)Z", Self::set_value, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("com.ktf.kfc.GProgressBar::<init>()");

        let _: () = jvm.invoke_special(&this, "org/kwis/msp/lwc/Component", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// Set the bar's full extent, answering whether it took. The bar is not
    /// drawn yet, so the value is accepted and nothing else is done with it.
    async fn set_maximum(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, maximum: i32) -> JvmResult<bool> {
        tracing::debug!("com.ktf.kfc.GProgressBar::setMaximum({maximum})");

        Ok(true)
    }

    /// Move the bar to a value, answering whether it took. As with the extent,
    /// the value is accepted and the bar is not drawn from it yet.
    async fn set_value(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, value: i32) -> JvmResult<bool> {
        tracing::debug!("com.ktf.kfc.GProgressBar::setValue({value})");

        Ok(true)
    }
}
