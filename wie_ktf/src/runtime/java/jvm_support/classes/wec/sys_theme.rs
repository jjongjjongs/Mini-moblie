use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// A KTF handset's UI-theme record, handed out by `wec.OEMDevice.getSYSTheme`.
/// The archive ships no class file for it, so a title that reads the handset's
/// theme finds nothing unless the platform publishes it (the same reason
/// `wec.DMInfo` and `wec.OEMDevice` live here).
///
/// Only the members a title is seen to ask for are served; a title that reaches
/// for more fails by name, which says what to add next.
pub struct SYSTheme;

impl SYSTheme {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "wec/SYSTheme",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("<init>", "()V", Self::init, Default::default())],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("wec.SYSTheme::<init>()");

        Ok(())
    }
}
