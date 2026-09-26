use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaIoInputStream, runtime::JavaLangClassLoader, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.MediaPlayer
//
// LG WIPI's location-based sound player: a title points it at a resource with
// `setMediaLocation`, sets a volume, and starts and stops it. 호국전기이순신
// makes one in its GeneralCanvas and drives every sound through it - a `.mmf`
// (Yamaha SMAF) clip named by a jar path like `/sound/m_sel.mmf`.
//
// The clip is loaded from the classpath the first time it is started and kept
// under a handle so a restart of the same location does not parse it again;
// changing the location drops the old handle. Playback runs through the same
// SMAF path as `net.wie.SmafPlayer`.
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
                JavaFieldProto::new("__wieHandle", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::<init>({this:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        // No clip loaded yet.
        jvm.put_field(&mut this, "__wieHandle", "I", -1).await?;

        Ok(())
    }

    async fn set_media_location(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        location: ClassInstanceRef<String>,
    ) -> JvmResult<()> {
        let path = if location.is_null() {
            "null".into()
        } else {
            JavaLangString::to_rust_string(jvm, &location).await?
        };
        tracing::debug!("mmpp.media.MediaPlayer::setMediaLocation({this:?}, {path:?})");

        // A new location invalidates the loaded clip; drop the old handle so
        // `start` loads the one now named.
        let handle: i32 = jvm.get_field(&this, "__wieHandle", "I").await?;
        if handle >= 0 {
            context.system().audio().close(handle as u32).ok();
            jvm.put_field(&mut this, "__wieHandle", "I", -1).await?;
        }

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

        // Kept for the record; the level's scale is the handset's own and the
        // clip plays at the mixer's default rather than risk muting it.
        jvm.put_field(&mut this, "__wieVolume", "Ljava/lang/String;", level).await?;

        Ok(())
    }

    async fn start(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::start({this:?})");

        let mut handle: i32 = jvm.get_field(&this, "__wieHandle", "I").await?;

        if handle < 0 {
            let location: ClassInstanceRef<String> = jvm.get_field(&this, "__wieLocation", "Ljava/lang/String;").await?;
            if location.is_null() {
                return Ok(());
            }
            let path = JavaLangString::to_rust_string(jvm, &location).await?;

            let Some(data) = Self::read_resource(jvm, &path).await? else {
                tracing::warn!("mmpp.media.MediaPlayer::start: clip not found: {path:?}");
                return Ok(());
            };

            match context.system().audio().load_smaf(&data) {
                Ok(loaded) => {
                    handle = loaded as i32;
                    jvm.put_field(&mut this, "__wieHandle", "I", handle).await?;
                }
                Err(error) => {
                    tracing::warn!("mmpp.media.MediaPlayer::start: cannot load {path:?}: {error:?}");
                    return Ok(());
                }
            }
        }

        let system = context.system();
        // These titles reuse one player for one-shot cues, starting it fresh
        // each time, so the clip plays once rather than looping.
        system.audio().play(system, handle as u32, false).ok();

        Ok(())
    }

    async fn stop(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::stop({this:?})");

        let handle: i32 = jvm.get_field(&this, "__wieHandle", "I").await?;
        if handle >= 0 {
            context.system().audio().stop(handle as u32);
        }

        Ok(())
    }

    /// Reads a clip from the classpath, tolerating the leading slash LG titles
    /// write into the location (`/sound/m_sel.mmf`) but a class loader drops.
    async fn read_resource(jvm: &Jvm, path: &str) -> JvmResult<Option<alloc::vec::Vec<u8>>> {
        let class_loader = jvm.current_class_loader().await?;

        let mut stream = JavaLangClassLoader::get_resource_as_stream(jvm, &class_loader, path).await?;
        if let (None, Some(trimmed)) = (&stream, path.strip_prefix('/')) {
            stream = JavaLangClassLoader::get_resource_as_stream(jvm, &class_loader, trimmed).await?;
        }

        let Some(stream) = stream else {
            return Ok(None);
        };

        Ok(Some(JavaIoInputStream::read_until_end(jvm, &stream).await?))
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use jvm::ClassInstanceRef;
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::get_protos;

    #[test]
    fn media_player_class_resolves_and_constructs() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let player: ClassInstanceRef<()> = jvm.new_class("mmpp/media/MediaPlayer", "()V", ()).await?.into();
            assert!(!player.is_null());
            // With no location set, start and stop are quiet no-ops.
            let _: () = jvm.invoke_virtual(&player, "start", "()V", ()).await?;
            let _: () = jvm.invoke_virtual(&player, "stop", "()V", ()).await?;
            Ok::<(), jvm::JavaError>(())
        })
    }
}
