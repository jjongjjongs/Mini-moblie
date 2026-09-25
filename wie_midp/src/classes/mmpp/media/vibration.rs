use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.Vibration
//
// LG WIPI's vibration control. 호국전기이순신 buzzes with `start(int, int)`
// (a level and a duration) from its canvas loop; there is no motor to drive
// here, so the call is accepted and does nothing.
pub struct Vibration;

impl Vibration {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/media/Vibration",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new("start", "(II)V", Self::start, MethodAccessFlags::STATIC)],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn start(_jvm: &Jvm, _context: &mut WieJvmContext, level: i32, duration: i32) -> JvmResult<()> {
        tracing::debug!("stub mmpp.media.Vibration::start({level}, {duration})");

        Ok(())
    }
}
