use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// A KTF handset's WAP-gateway record, named by a title but shipped by no
/// archive: the handset provides it, so a title that links against it finds
/// nothing unless the platform publishes it here (the same reason `wec.DMInfo`
/// lives here).
///
/// Only the members a title is seen to ask for are served; anything else is
/// left absent on purpose, so a title that reaches for more fails by name and
/// says what to add next.
pub struct GatewayIP;

impl GatewayIP {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "wec/GatewayIP",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("SetBillcommGWIP", "(Ljava/lang/String;)Z", Self::set_billcomm_gwip, Default::default()),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("wec.GatewayIP::<init>()");

        Ok(())
    }

    /// Point the handset's billing traffic at a gateway IP, answering whether it
    /// took. Billing runs through the host's own local endpoints rather than a
    /// real gateway, so the address is noted and the request always takes.
    async fn set_billcomm_gwip(_: &Jvm, _: &mut WieJvmContext, _this: ClassInstanceRef<Self>, _ip: ClassInstanceRef<String>) -> JvmResult<bool> {
        tracing::debug!("wec.GatewayIP::SetBillcommGWIP(..)");

        Ok(true)
    }
}
