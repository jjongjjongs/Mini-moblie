use alloc::{vec, vec::Vec};

use bytemuck::cast_vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult, runtime::JavaIoInputStream, runtime::JavaLangClassLoader, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

// class mmpp.media.MediaPlayer
//
// LG WIPI's sound player: a title points it at a clip, sets a volume, and
// starts and stops it. 호국전기이순신 makes one in its GeneralCanvas and drives
// every sound through it - a `.mmf` (Yamaha SMAF) clip named by a jar path like
// `/sound/m_sel.mmf` through `setMediaLocation`.
//
// The clip is either named by a classpath resource (`setMediaLocation`) or
// handed over whole as bytes (`setMediaSource`): 나이트세이버 reads each `.mmf`
// out of its own jar itself and passes the bytes in, naming no location. Either
// way it is loaded the first time it is started and kept under a handle so a
// restart does not parse it again; naming a new clip drops the old handle.
// Playback runs through the same SMAF path as `net.wie.SmafPlayer`.
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
                JavaMethodProto::new("setMediaSource", "([B)V", Self::set_media_source, Default::default()),
                JavaMethodProto::new("setVolumeLevel", "(Ljava/lang/String;)V", Self::set_volume_level, Default::default()),
                JavaMethodProto::new("setPlayBackLoop", "(Z)V", Self::set_play_back_loop, Default::default()),
                JavaMethodProto::new("start", "()V", Self::start, Default::default()),
                JavaMethodProto::new("stop", "()V", Self::stop, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("__wieLocation", "Ljava/lang/String;", Default::default()),
                JavaFieldProto::new("__wieSource", "[B", Default::default()),
                JavaFieldProto::new("__wieVolume", "Ljava/lang/String;", Default::default()),
                JavaFieldProto::new("__wieHandle", "I", Default::default()),
                JavaFieldProto::new("__wieLoop", "Z", Default::default()),
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

    async fn set_media_source(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        source: ClassInstanceRef<Array<i8>>,
    ) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::setMediaSource({this:?}, {source:?})");

        // A clip handed over whole invalidates any loaded one; drop the old
        // handle so `start` loads the bytes now held.
        let handle: i32 = jvm.get_field(&this, "__wieHandle", "I").await?;
        if handle >= 0 {
            context.system().audio().close(handle as u32).ok();
            jvm.put_field(&mut this, "__wieHandle", "I", -1).await?;
        }

        // The bytes win over a location: naming a source is the title saying it
        // holds the clip itself, so a stale location must not be loaded instead.
        jvm.put_field(&mut this, "__wieLocation", "Ljava/lang/String;", None).await?;
        jvm.put_field(&mut this, "__wieSource", "[B", source).await?;

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

    async fn set_play_back_loop(jvm: &Jvm, _context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, loop_: bool) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::setPlayBackLoop({this:?}, {loop_:?})");

        // Remembered here and applied on `start`, so a title's looping BGM
        // repeats while its one-shot cues (left at false) play once. 열혈강호2
        // sets this on every clip it loads in `loadSnd`.
        jvm.put_field(&mut this, "__wieLoop", "Z", loop_).await?;

        Ok(())
    }

    async fn start(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("mmpp.media.MediaPlayer::start({this:?})");

        let mut handle: i32 = jvm.get_field(&this, "__wieHandle", "I").await?;

        if handle < 0 {
            // A source given as bytes is loaded as it is; otherwise the clip is
            // read from the classpath by its location. A title uses one or the
            // other, and a set source has already cleared any location.
            let source: ClassInstanceRef<Array<i8>> = jvm.get_field(&this, "__wieSource", "[B").await?;
            let (data, what) = if !source.is_null() {
                let length = jvm.array_length(&source).await?;
                let data: Vec<i8> = jvm.load_array(&source, 0, length).await?;

                (cast_vec(data), "source".into())
            } else {
                let location: ClassInstanceRef<String> = jvm.get_field(&this, "__wieLocation", "Ljava/lang/String;").await?;
                if location.is_null() {
                    return Ok(());
                }
                let path = JavaLangString::to_rust_string(jvm, &location).await?;

                let Some(data) = Self::read_resource(jvm, &path).await? else {
                    tracing::warn!("mmpp.media.MediaPlayer::start: clip not found: {path:?}");
                    return Ok(());
                };

                (data, path)
            };

            match context.system().audio().load_smaf(&data) {
                Ok(loaded) => {
                    handle = loaded as i32;
                    jvm.put_field(&mut this, "__wieHandle", "I", handle).await?;
                }
                Err(error) => {
                    tracing::warn!("mmpp.media.MediaPlayer::start: cannot load {what:?}: {error:?}");
                    return Ok(());
                }
            }
        }

        let loop_: bool = jvm.get_field(&this, "__wieLoop", "Z").await?;

        let system = context.system();
        // One-shot cues start fresh each time and play once; a clip flagged by
        // `setPlayBackLoop(true)` (e.g. BGM) repeats instead.
        system.audio().play(system, handle as u32, loop_).ok();

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
            let _: () = jvm.invoke_virtual(&player, "setPlayBackLoop", "(Z)V", (true,)).await?;
            let _: () = jvm.invoke_virtual(&player, "start", "()V", ()).await?;
            let _: () = jvm.invoke_virtual(&player, "stop", "()V", ()).await?;
            Ok::<(), jvm::JavaError>(())
        })
    }

    /// A clip handed over as bytes resolves the method 나이트세이버 calls and
    /// starts without a location; bytes that are not a clip are a quiet no-op
    /// rather than a fault.
    #[test]
    fn media_player_takes_a_source_as_bytes() -> Result<()> {
        run_jvm_test(Box::new([get_protos().into()]), |jvm| async move {
            let player: ClassInstanceRef<()> = jvm.new_class("mmpp/media/MediaPlayer", "()V", ()).await?.into();

            let mut source = jvm.instantiate_array("B", 4).await?;
            jvm.store_array(&mut source, 0, [1i8, 2, 3, 4]).await?;

            let _: () = jvm.invoke_virtual(&player, "setMediaSource", "([B)V", (source,)).await?;
            let _: () = jvm.invoke_virtual(&player, "start", "()V", ()).await?;
            let _: () = jvm.invoke_virtual(&player, "stop", "()V", ()).await?;
            Ok::<(), jvm::JavaError>(())
        })
    }
}
