use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.BackLight
//
// LG WIPI's backlight control. 호국전기이순신 turns it on with `on(int)` from
// its canvas loop; the handset has no backlight to drive here, so the call is
// accepted and does nothing.
pub struct BackLight;

impl BackLight {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/media/BackLight",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("on", "(I)V", Self::on, MethodAccessFlags::STATIC)],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn on(_jvm: &Jvm, _context: &mut WieJvmContext, duration: i32) -> JvmResult<()> {
        tracing::debug!("stub mmpp.media.BackLight::on({duration})");

        Ok(())
    }
}
