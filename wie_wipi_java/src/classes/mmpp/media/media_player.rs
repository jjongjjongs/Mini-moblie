use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.MediaPlayer
//
// LG WIPI's location-based sound player: a title points it at a resource with
// `setMediaLocation`, sets a volume, and starts and stops it. 호국전기이순신's
// GeneralCanvas makes one in its constructor, so without the class the title
// threw NoClassDefFoundError before it could draw anything.
//
// The location and volume are kept so a reader can see what was asked for; the
// playback itself is not wired up here, so the title runs without this sound
// rather than not at all.
pub struct MediaPlayer;

impl MediaPlayer {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "mmpp/media/MediaPlayer",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("setMediaLocation", "(Ljava/lang/String;)V", Self::set_media_location, Default::default()),
                JavaMethodProto::new("setVolumeLevel", "(Ljava/lang/String;)V", Self::set_volume_level, Default::default()),
                JavaMethodProto::new("start", "()V", Self::start, Default::default()),
                JavaMethodProto::new("stop", "()V", Self::stop, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("__wieLocation", "Ljava/lang/String;", Default::default()),
                JavaFieldProto::new("__wieVolume", "Ljava/lang/String;", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }

    async fn set_media_location(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        location: ClassInstanceRef<String>,
    ) -> JvmResult<()> {
        let path = if location.is_null() {
            "null".into()
        } else {
            JavaLangString::to_rust_string(jvm, &location).await?
        };
        tracing::debug!("mmpp.media.MediaPlayer::setMediaLocation({this:?}, {path:?})");

        jvm.put_field(&mut this, "__wieLocation", "Ljava/lang/String;", location).await?;

        Ok(())
    }

    async fn set_volume_level(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        level: ClassInstanceRef<String>,
    ) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::setVolumeLevel({this:?}, {level:?})");

        jvm.put_field(&mut this, "__wieVolume", "Ljava/lang/String;", level).await?;

        Ok(())
    }

    async fn start(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::warn!("stub mmpp.media.MediaPlayer::start({this:?})");

        Ok(())
    }

    async fn stop(_jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::warn!("stub mmpp.media.MediaPlayer::stop({this:?})");

        Ok(())
    }
}
