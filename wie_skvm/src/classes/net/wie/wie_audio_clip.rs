use alloc::{vec, vec::Vec};
use core::sync::atomic::Ordering;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_runtime::classes::java::lang::String;
use jvm::{Array, ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

/// How often a `play` or `loop` looks at whether its clip has stopped.
const WAIT_POLL_MS: u64 = 20;

// class net.wie.WieAudioClip
//
// SK-VM's `com.skt.m.AudioClip`. Every method used to be a stub, so no SK-VM
// title made a sound.
//
// **`play` and `loop` do not return while the clip is sounding.** `play` holds
// its caller until the clip has played out, `loop` until something stops it -
// a `stop` or `close` from another thread, or another start of the same clip.
// That is the vendor's contract, as the reference emulator (wfeature,
// `internal/platform/skt/skvm.go`) documents it from two titles, and
// 디지몬RPGII is a third: its sound thread is `play()` or `loop()` followed at
// once by `close()`, and it stops its music by calling `close` on the clip
// from the game thread. With a `play` that returned at once, the close right
// after it would cut every sound off before its first note.
pub struct WieAudioClip;

impl WieAudioClip {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "net/wie/WieAudioClip",
            parent_class: Some("java/lang/Object"),
            interfaces: vec!["com/skt/m/AudioClip"],
            methods: vec![
                JavaMethodProto::new("<init>", "(Ljava/lang/String;)V", Self::init, Default::default()),
                JavaMethodProto::new("open", "([BII)V", Self::open, Default::default()),
                JavaMethodProto::new("play", "()V", Self::play, Default::default()),
                JavaMethodProto::new("loop", "()V", Self::r#loop, Default::default()),
                JavaMethodProto::new("stop", "()V", Self::stop, Default::default()),
                JavaMethodProto::new("close", "()V", Self::close, Default::default()),
            ],
            // The backend's handle for what `open` loaded; 0 is nothing loaded,
            // since the backend hands out handles from 1.
            fields: vec![JavaFieldProto::new("audioHandle", "I", Default::default())],
            access_flags: Default::default(),
        }
    }

    async fn init(jvm: &Jvm, _context: &mut WieJvmContext, this: ClassInstanceRef<Self>, name: ClassInstanceRef<String>) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::<init>({this:?}, {name:?})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        Ok(())
    }

    /// Loads the clip's data, replacing whatever the clip held before.
    async fn open(
        jvm: &Jvm,
        context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        data: ClassInstanceRef<Array<i8>>,
        offset: i32,
        buffer_size: i32,
    ) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::open({this:?}, {data:?}, {offset}, {buffer_size})");

        if data.is_null() {
            return Err(jvm.exception("java/lang/NullPointerException", "data is null").await);
        }
        let length = jvm.array_length(&data).await? as i32;
        if offset < 0 || buffer_size < 0 || offset > length - buffer_size {
            return Err(jvm.exception("java/lang/ArrayIndexOutOfBoundsException", "").await);
        }

        let bytes: Vec<i8> = jvm.load_array(&data, offset as _, buffer_size as _).await?;
        let bytes: Vec<u8> = bytes.into_iter().map(|x| x as u8).collect();

        let previous: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        let mut audio = context.system().audio();
        if previous != 0 {
            let _ = audio.close(previous as u32);
        }
        let handle = audio.load_smaf(&bytes).map_or(0, |x| x as i32);
        drop(audio);

        jvm.put_field(&mut this, "audioHandle", "I", handle).await?;

        Ok(())
    }

    async fn play(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::play({this:?})");

        Self::start(jvm, context, this, false).await
    }

    async fn r#loop(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::loop({this:?})");

        Self::start(jvm, context, this, true).await
    }

    /// Starts the clip and waits for it: to the end of the clip for a play,
    /// and for a stop - which a loop has no other end than - for either.
    async fn start(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>, repeat: bool) -> JvmResult<()> {
        let handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        if handle == 0 {
            return Ok(());
        }

        let system = context.system().clone();
        let playback = system.audio().play_with_completion(&system, handle as u32, repeat);
        let Ok(playback) = playback else {
            return Ok(());
        };

        loop {
            let stopped = playback.stopped.load(Ordering::Relaxed) || playback.superseded.load(Ordering::Relaxed);
            if stopped || (!repeat && playback.completed.load(Ordering::Acquire)) {
                return Ok(());
            }

            system.sleep(WAIT_POLL_MS).await;
        }
    }

    async fn stop(jvm: &Jvm, context: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::stop({this:?})");

        let handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        if handle != 0 {
            context.system().audio().stop(handle as u32);
        }

        Ok(())
    }

    /// Stops the clip and lets go of its data. Closing a clip that holds
    /// nothing - one another thread already closed - does nothing.
    async fn close(jvm: &Jvm, context: &mut WieJvmContext, mut this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("net.wie.WieAudioClip::close({this:?})");

        let handle: i32 = jvm.get_field(&this, "audioHandle", "I").await?;
        if handle != 0 {
            let _ = context.system().audio().close(handle as u32);
            jvm.put_field(&mut this, "audioHandle", "I", 0).await?;
        }

        Ok(())
    }
}
