use alloc::vec;

use java_class_proto::JavaMethodProto;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// A KTF handset's own device class, named by a title but shipped by no
/// archive: the handset provides it, so a title that links against it finds
/// nothing unless the platform publishes it here (the same reason `wec.DMInfo`
/// lives here).
///
/// Only the members a title is seen to ask for are served; anything else is
/// left absent on purpose, so a title that reaches for more fails by name and
/// says what to add next.
pub struct OEMDevice;

impl OEMDevice {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "wec/OEMDevice",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("enableSleep", "(Z)Z", Self::enable_sleep, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("wec.OEMDevice::<init>()");

        Ok(())
    }

    /// Turn the handset's idle sleep on or off, answering whether the request
    /// took. A title toggles it so its own screen does not blank mid-play; this
    /// host never sleeps the screen, so the request always takes.
    async fn enable_sleep(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, enable: bool) -> JvmResult<bool> {
        tracing::debug!("wec.OEMDevice::enableSleep({enable})");

        Ok(true)
    }
}
