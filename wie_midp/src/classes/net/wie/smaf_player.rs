use alloc::vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::io::InputStream;
use jvm::{ClassInstanceRef, Jvm, Result, runtime::JavaIoInputStream};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// `audio/midi`, for `<init>(InputStream, int)`.
pub const FORMAT_MIDI: i32 = 1;
/// `audio/x-wav`, for `<init>(InputStream, int)`.
pub const FORMAT_WAVE: i32 = 2;

// class net.wie.SmafPlayer
pub struct SmafPlayer;

impl SmafPlayer {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/SmafPlayer",
            parent_class: Some("java/lang/Object"),
            interfaces: vec!["javax/microedition/media/Player"],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/io/InputStream;)V", Self::init, Default::default()),
                JavaMethodProto::new("<init>", "(Ljava/io/InputStream;I)V", Self::init_with_format, Default::default()),
                JavaMethodProto::new("start", "()V", Self::start, Default::default()),
                JavaMethodProto::new("start", "(Z)V", Self::start_with_repeat, Default::default()),
                JavaMethodProto::new("stop", "()V", Self::stop, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
                JavaMethodProto::new("setVolume", "(I)I", Self::set_volume, Default::default()),
                JavaMethodProto::new("getVolume", "()I", Self::get_volume, Default::default()),
                JavaMethodProto::new("getState", "()I", Self::get_state, Default::default()),
                JavaMethodProto::new("setLoopCount", "(I)V", Self::set_loop_count, Default::default()),
                JavaMethodProto::new("setMediaTime", "(J)J", Self::set_media_time, Default::default()),
            ],
            fields: vec![
                JavaFieldProto::new("audioHandle", "I", Default::default()),
                JavaFieldProto::new("loopCount", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, stream: ClassInstanceRef<InputStream>) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::<init>({this:?}, {stream:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        let data = JavaIoInputStream::read_until_end(jvm, &stream).await?;
        let audio_handle = context.system().audio().load_smaf(&data).unwrap();

        jvm.put_field(&mut this, "audioHandle", "I", audio_handle as i32).await?;

        Ok(())
    }

    /// A clip in one of the other formats `Manager` takes - [`FORMAT_MIDI`] or
    /// [`FORMAT_WAVE`] - played through the same audio path. A file that does
    /// not read as its format is a `MediaException`, as on a handset.
    async fn init_with_format(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        stream: ClassInstanceRef<InputStream>,
        format: i32,
    ) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::<init>({this:?}, {stream:?}, {format})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        let data = JavaIoInputStream::read_until_end(jvm, &stream).await?;
        let mut audio = context.system().audio();
        let loaded = match format {
            FORMAT_MIDI => audio.load_midi(&data),
            FORMAT_WAVE => audio.load_wave(&data),
            _ => audio.load_smaf(&data),
        };
        drop(audio);

        let Ok(audio_handle) = loaded else {
            return Err(jvm
                .exception("javax/microedition/media/MediaException", "the clip does not read as its type")
                .await);
        };

        jvm.put_field(&mut this, "audioHandle", "I", audio_handle as i32).await?;

        Ok(())
    }

    async fn start(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<()> {
        // MIDP's loop count: -1 for ever, more than one for that many passes.
        // Left unset (zero) it is the one pass this always played.
        let loop_count: i32 = jvm.get_field(&this, "loopCount", "I").await?;

        Self::start_with_repeat(jvm, context, this, loop_count == -1 || loop_count > 1).await
    }

    /// MIDP's `Player.getState`: `STARTED` while the clip sounds, and
    /// `PREFETCHED` - loaded and ready - otherwise, which is where a player
    /// made from a stream is from the moment it exists here.
    async fn get_state(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<i32> {
        const PREFETCHED: i32 = 300;
        const STARTED: i32 = 400;

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        let playing = context.system().audio().is_playing(audio_handle as u32);
        tracing::debug!("net.wie.SmafPlayer::getState({this:?}) -> playing={playing}");

        Ok(if playing { STARTED } else { PREFETCHED })
    }

    /// MIDP's `Player.setLoopCount`, read by the next `start`.
    async fn set_loop_count(jvm: &Jvm, _: &mut WieJvmContext, mut this: ClassInstanceRef<Self>, count: i32) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::setLoopCount({this:?}, {count})");

        jvm.put_field(&mut this, "loopCount", "I", count).await?;

        Ok(())
    }

    /// MIDP's `Player.setMediaTime`. Every start plays from the beginning, so
    /// the rewind titles ask for before one is what happens anyway; the time
    /// it lands on is the beginning.
    async fn set_media_time(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>, time: i64) -> Result<i64> {
        tracing::debug!("net.wie.SmafPlayer::setMediaTime({this:?}, {time})");

        Ok(0)
    }

    async fn start_with_repeat(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, repeat: bool) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::start({this:?}, {repeat})");

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;

        let system = context.system();

        system.audio().play(system, audio_handle as u32, repeat).unwrap();

        Ok(())
    }

    async fn stop(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::stop({this:?})");

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;

        let system = context.system();

        system.audio().stop(audio_handle as u32);

        Ok(())
    }

    async fn close(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<()> {
        tracing::debug!("net.wie.SmafPlayer::close({this:?})");

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;

        let system = context.system();

        system.audio().close(audio_handle as u32).unwrap();

        Ok(())
    }

    async fn set_volume(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, volume: i32) -> Result<i32> {
        tracing::debug!("net.wie.SmafPlayer::setVolume({this:?}, {volume})");

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        let system = context.system();

        match system.audio().set_volume(audio_handle as u32, volume.clamp(0, 100) as u8) {
            Ok(()) => Ok(0),
            Err(_) => Ok(-9),
        }
    }

    async fn get_volume(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> Result<i32> {
        tracing::debug!("net.wie.SmafPlayer::getVolume({this:?})");

        let audio_handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        let system = context.system();

        match system.audio().get_volume(audio_handle as u32) {
            Ok(volume) => Ok(i32::from(volume)),
            Err(_) => Ok(-9),
        }
    }
}
