use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.Vibration
//
// LG WIPI's vibration control, the mirror of com.skt.m.Vibration. Titles ask
// `getLevelNum` for the number of intensity steps, then buzz with
// `start(level, duration)`; 열혈강호2 reads the level count while setting sound
// up. The level is mapped onto the host vibrator so a real handset actually
// buzzes, and is a no-op where there is no motor.
pub struct Vibration;

impl Vibration {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/media/Vibration",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("getLevelNum", "()I", Self::get_level_num, MethodAccessFlags::STATIC),
                JavaMethodProto::new("start", "(II)V", Self::start, MethodAccessFlags::STATIC),
                JavaMethodProto::new("stop", "()V", Self::stop, MethodAccessFlags::STATIC),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn get_level_num(_jvm: &Jvm, _context: &mut WieJvmContext) -> JvmResult<i32> {
        tracing::debug!("mmpp.media.Vibration::getLevelNum()");

        Ok(10)
    }

    async fn start(_jvm: &Jvm, context: &mut WieJvmContext, level: i32, duration: i32) -> JvmResult<()> {
        tracing::debug!("mmpp.media.Vibration::start({level}, {duration})");

        let duration_ms = duration.max(0) as u64;
        let intensity = (level.clamp(0, 10) * 10) as u8;
        context.system().platform().vibrate(duration_ms, intensity);

        Ok(())
    }

    async fn stop(_jvm: &Jvm, context: &mut WieJvmContext) -> JvmResult<()> {
        tracing::debug!("mmpp.media.Vibration::stop()");

        context.system().platform().vibrate(0, 0);

        Ok(())
    }
}
