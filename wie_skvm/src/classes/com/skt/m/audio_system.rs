use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::{FieldAccessFlags, MethodAccessFlags};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::com::skt::m::audio_clip::AudioClip;

/// The top of the volume scale. A title asks for it to work out the step of
/// its own volume setting, so it has to be the range `setVolume` takes: at 0,
/// 드래곤아이즈 took its scale from nothing. (wfeature's, `skvm.go`.)
const MAX_VOLUME: i32 = 100;

/// The volume a title reads before it has set one.
const DEFAULT_VOLUME: i32 = 50;

// class com.skt.m.AudioSystem
//
// One volume, whatever the format named: a handset had one output, and all
// the titles seen name "mmf". It is what the title set, read back; playback
// itself is not scaled by it.
pub struct AudioSystem;

impl AudioSystem {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "com/skt/m/AudioSystem",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new(
                    "getAudioClip",
                    "(Ljava/lang/String;)Lcom/skt/m/AudioClip;",
                    Self::get_audio_clip,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new("getMaxVolume", "(Ljava/lang/String;)I", Self::get_max_volume, MethodAccessFlags::STATIC),
                JavaMethodProto::new("getVolume", "(Ljava/lang/String;)I", Self::get_volume, MethodAccessFlags::STATIC),
                JavaMethodProto::new("setVolume", "(Ljava/lang/String;I)V", Self::set_volume, MethodAccessFlags::STATIC),
            ],
            fields: vec![
                JavaFieldProto::new("wieVolume", "I", FieldAccessFlags::STATIC),
                JavaFieldProto::new("wieVolumeSet", "Z", FieldAccessFlags::STATIC),
            ],
            access_flags: Default::default(),
        }
    }

    async fn get_audio_clip(jvm: &Jvm, _context: &mut WieJvmContext, name: ClassInstanceRef<String>) -> JvmResult<ClassInstanceRef<AudioClip>> {
        tracing::debug!("com.skt.m.AudioSystem::getAudioClip({name:?})");

        let audio_clip = jvm.new_class("net/wie/WieAudioClip", "(Ljava/lang/String;)V", (name,)).await?;

        Ok(audio_clip.into())
    }

    async fn get_max_volume(jvm: &Jvm, _context: &mut WieJvmContext, format: ClassInstanceRef<String>) -> JvmResult<i32> {
        tracing::debug!("com.skt.m.AudioSystem::getMaxVolume({format:?})");

        require_format(jvm, &format).await?;

        Ok(MAX_VOLUME)
    }

    async fn get_volume(jvm: &Jvm, _context: &mut WieJvmContext, format: ClassInstanceRef<String>) -> JvmResult<i32> {
        tracing::debug!("com.skt.m.AudioSystem::getVolume({format:?})");

        require_format(jvm, &format).await?;

        let set: bool = jvm.get_static_field("com/skt/m/AudioSystem", "wieVolumeSet", "Z").await?;
        if !set {
            return Ok(DEFAULT_VOLUME);
        }

        jvm.get_static_field("com/skt/m/AudioSystem", "wieVolume", "I").await
    }

    async fn set_volume(jvm: &Jvm, _context: &mut WieJvmContext, format: ClassInstanceRef<String>, level: i32) -> JvmResult<()> {
        tracing::debug!("com.skt.m.AudioSystem::setVolume({format:?}, {level})");

        require_format(jvm, &format).await?;

        jvm.put_static_field("com/skt/m/AudioSystem", "wieVolume", "I", level.clamp(0, MAX_VOLUME))
            .await?;
        jvm.put_static_field("com/skt/m/AudioSystem", "wieVolumeSet", "Z", true).await
    }
}

/// A null format is refused the way the handset refused it.
async fn require_format(jvm: &Jvm, format: &ClassInstanceRef<String>) -> JvmResult<()> {
    if format.is_null() {
        return Err(jvm.exception("java/lang/NullPointerException", "audio format is null").await);
    }

    Ok(())
}
