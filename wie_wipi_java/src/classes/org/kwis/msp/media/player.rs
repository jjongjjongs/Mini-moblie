use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use jvm::{ClassInstanceRef, Jvm, Result as JvmResult};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::org::kwis::msp::media::{BaseClip, Clip};

// class org.kwis.msp.media.Player
pub struct Player;

impl Player {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "org/kwis/msp/media/Player",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "()V", Self::init, Default::default()),
                JavaMethodProto::new("pause", "(Lorg/kwis/msp/media/BaseClip;)Z", Self::pause, MethodAccessFlags::STATIC),
                JavaMethodProto::new("stop", "(Lorg/kwis/msp/media/BaseClip;)Z", Self::stop, MethodAccessFlags::STATIC),
                JavaMethodProto::new("resume", "(Lorg/kwis/msp/media/BaseClip;)Z", Self::resume, MethodAccessFlags::STATIC),
                JavaMethodProto::new("play", "(Lorg/kwis/msp/media/BaseClip;Z)Z", Self::play, MethodAccessFlags::STATIC),
                JavaMethodProto::new("record", "(Lorg/kwis/msp/media/BaseClip;)Z", Self::record, MethodAccessFlags::STATIC),
                JavaMethodProto::new("play", "(Lorg/kwis/msp/media/Clip;Z)Z", Self::play_clip, MethodAccessFlags::STATIC),
                JavaMethodProto::new("stop", "(Lorg/kwis/msp/media/Clip;)Z", Self::stop_clip, MethodAccessFlags::STATIC),
                JavaMethodProto::new("pause", "(Lorg/kwis/msp/media/Clip;)Z", Self::pause_clip, MethodAccessFlags::STATIC),
                JavaMethodProto::new("resume", "(Lorg/kwis/msp/media/Clip;)Z", Self::resume_clip, MethodAccessFlags::STATIC),
                JavaMethodProto::new("record", "(Lorg/kwis/msp/media/Clip;)Z", Self::record_clip, MethodAccessFlags::STATIC),
            ],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn init(_: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<()> {
        tracing::debug!("org.kwis.msp.media.Player::<init>({this:?})");

        Ok(())
    }

    /// The same play the `Clip` overload below does.
    ///
    /// A title reaches for whichever of the two its own code is declared
    /// against, and `Clip` extends `BaseClip`, so both are handed a clip that
    /// can be played - the sound does not depend on which name the descriptor
    /// happens to carry. This one answered that nothing played, so the two
    /// 전설의 마법학교 titles, which call it for every sound they make, ran with
    /// no sound at all while loading their clips and setting volumes on them
    /// exactly as a title that could be heard does.
    async fn play(jvm: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<BaseClip>, repeat: bool) -> JvmResult<bool> {
        tracing::debug!("org.kwis.msp.media.Player::play({clip:?}, {repeat})");

        Self::play_any(jvm, &clip, repeat).await
    }

    /// The same stop the `Clip` overload below does - see [`Self::stop_clip`]
    /// for why the answer is the clip's own state rather than a fixed one.
    async fn stop(jvm: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<BaseClip>) -> JvmResult<bool> {
        tracing::debug!("org.kwis.msp.media.Player::stop({clip:?})");

        Self::stop_any(jvm, &clip).await
    }

    async fn pause(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<BaseClip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::pause({clip:?})");

        Ok(false)
    }

    async fn resume(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<BaseClip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::resume({clip:?})");

        Ok(false)
    }

    async fn record(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<BaseClip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::record({clip:?})");

        Ok(false)
    }

    /// Plays a clip, whichever of the two overloads asked for it.
    ///
    /// Everything it calls - `allocPlayer`, `mediaPlay` - is declared on
    /// `BaseClip`, so a `Clip` and a `BaseClip` are played the same way.
    async fn play_any<T>(jvm: &Jvm, clip: &ClassInstanceRef<T>, repeat: bool) -> JvmResult<bool> {
        // Titles call these with a clip slot they have not filled - a sound
        // that failed to load, or a "stop whatever is playing" call made before
        // anything was. They shipped on handsets doing it, so the platform
        // tolerates it; answering that nothing played is that answer.
        if clip.is_null() {
            return Ok(false);
        }

        let alloc_result: i32 = jvm.invoke_virtual(clip, "allocPlayer", "()I", ()).await?;
        if alloc_result != 0 {
            return Err(jvm.exception("org/kwis/msp/media/MediaUnavailableException", "").await);
        }

        let play_result: i32 = jvm.invoke_virtual(clip, "mediaPlay", "(Z)I", (repeat,)).await?;

        match play_result {
            0 => Ok(true),
            -16 | -9 | -7 | -6 | -1 => Err(jvm.exception("org/kwis/msp/media/MediaUnavailableException", "").await),
            _ => Ok(false),
        }
    }

    /// Stops a clip, whichever of the two overloads asked for it, and answers
    /// whether there was anything to stop.
    async fn stop_any<T>(jvm: &Jvm, clip: &ClassInstanceRef<T>) -> JvmResult<bool> {
        // As in `play_any` above: 레나크사가 stops a clip it never set, and a
        // deref here took the whole emulator down rather than the title.
        if clip.is_null() {
            return Ok(false);
        }

        let playing: bool = jvm.get_field(clip, "__wiePlaying", "Z").await?;
        let result: i32 = jvm.invoke_virtual(clip, "mediaStop", "()I", ()).await?;

        Ok(playing && result >= 0)
    }

    async fn play_clip(jvm: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<Clip>, repeat: bool) -> JvmResult<bool> {
        tracing::debug!("org.kwis.msp.media.Player::play({clip:?}, {repeat})");

        Self::play_any(jvm, &clip, repeat).await
    }

    /// Stops a clip, and answers whether there was anything to stop.
    ///
    /// The answer is not a health check on the audio system: a clip that is not
    /// sounding is not stopped by this, and saying otherwise is a title being
    /// told something that did not happen. 전설의 마법학교2 is what that costs -
    /// its `App.a(String, boolean)` at `0x106138` builds the clip, calls this,
    /// and reads the answer at `0x106312`:
    ///
    /// ```asm
    /// 10630e  bl    #0x14ac46     ; Player.stop(clip)
    /// 106312  lsls  r0, r0, #24   ; the boolean it answered
    /// 106316  beq   #0x10632c     ; false - carry on and play it
    /// 106318  movs  r3, #0xa4     ; true - park in state 0xa4 and return
    /// ```
    ///
    /// so a clip it has only just loaded is a clip it never plays. Answered
    /// out of what the clip is actually doing, that branch falls the way the
    /// handset's did and its BGM starts.
    async fn stop_clip(jvm: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<Clip>) -> JvmResult<bool> {
        tracing::debug!("org.kwis.msp.media.Player::stop({clip:?})");

        Self::stop_any(jvm, &clip).await
    }

    /// The three a clip cannot answer. Pausing and resuming need a clip-side
    /// call the reference does not declare either, and recording needs a
    /// capture source the audio backend does not offer; each answers that it
    /// did not happen, the same way the `BaseClip` overloads above do, so a
    /// title is told rather than left waiting on something that never runs.
    async fn pause_clip(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<Clip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::pause({clip:?})");

        Ok(false)
    }

    async fn resume_clip(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<Clip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::resume({clip:?})");

        Ok(false)
    }

    async fn record_clip(_: &Jvm, _: &mut WieJvmContext, clip: ClassInstanceRef<Clip>) -> JvmResult<bool> {
        tracing::warn!("stub org.kwis.msp.media.Player::record({clip:?})");

        Ok(false)
    }
}

#[cfg(test)]
mod test {
    use alloc::boxed::Box;

    use java_runtime::classes::java::lang::String;
    use jvm::{ClassInstanceRef, JavaError, runtime::JavaLangString};
    use test_utils::run_jvm_test;
    use wie_util::Result;

    use crate::{
        classes::org::kwis::msp::media::{BaseClip, Clip},
        get_protos,
    };

    /// The three a clip cannot answer stay unanswered on this overload too.
    #[test]
    fn test_base_clip_overloads_return_false() -> Result<()> {
        run_jvm_test(Box::new([wie_midp::get_protos().into(), get_protos().into()]), |jvm| async move {
            let clip: ClassInstanceRef<BaseClip> = jvm.new_class("org/kwis/msp/media/BaseClip", "()V", ()).await?.into();

            let paused: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "pause", "(Lorg/kwis/msp/media/BaseClip;)Z", (clip.clone(),))
                .await?;
            let resumed: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "resume", "(Lorg/kwis/msp/media/BaseClip;)Z", (clip.clone(),))
                .await?;
            let recorded: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "record", "(Lorg/kwis/msp/media/BaseClip;)Z", (clip.clone(),))
                .await?;

            assert!(!paused);
            assert!(!resumed);
            assert!(!recorded);

            // A clip with nothing in it is not stopped by a stop, the same
            // answer the `Clip` overload gives.
            let stopped: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "stop", "(Lorg/kwis/msp/media/BaseClip;)Z", (clip,))
                .await?;
            assert!(!stopped);

            Ok(())
        })
    }

    /// Playing through the `BaseClip` overload plays, exactly as the `Clip` one
    /// does.
    ///
    /// Which one a title reaches for is whichever its own code is declared
    /// against, and `Clip` extends `BaseClip`, so the sound cannot depend on
    /// it. This one answered `false` without playing anything, and the two
    /// 전설의 마법학교 titles - which load their clips and set volumes on them
    /// just like a title that can be heard - ran silent for it.
    #[test]
    fn either_overload_plays_the_same_clip() -> Result<()> {
        run_jvm_test(Box::new([wie_midp::get_protos().into(), get_protos().into()]), |jvm| async move {
            let r#type: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "audio/test").await?.into();
            let mut data = jvm.instantiate_array("B", 1).await?;
            jvm.store_array(&mut data, 0, [0i8]).await?;
            let clip: ClassInstanceRef<Clip> = jvm
                .new_class("org/kwis/msp/media/Clip", "(Ljava/lang/String;[B)V", (r#type, data))
                .await?
                .into();

            let played: bool = jvm
                .invoke_static(
                    "org/kwis/msp/media/Player",
                    "play",
                    "(Lorg/kwis/msp/media/BaseClip;Z)Z",
                    (clip.clone(), false),
                )
                .await?;
            assert!(played, "the BaseClip overload has to play the clip, not decline it");
            assert!(jvm.get_field::<bool>(&clip, "__wiePlaying", "Z").await?);

            // And the stop that goes with it reaches the same clip, so it
            // answers that it stopped something rather than that it did not.
            let stopped: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "stop", "(Lorg/kwis/msp/media/BaseClip;)Z", (clip.clone(),))
                .await?;
            assert!(stopped);
            assert!(!jvm.get_field::<bool>(&clip, "__wiePlaying", "Z").await?);

            // A clip with no data is unavailable through this overload too,
            // which is what the `Clip` one answers.
            let empty_type: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "audio/empty").await?.into();
            let empty: ClassInstanceRef<Clip> = jvm
                .new_class("org/kwis/msp/media/Clip", "(Ljava/lang/String;)V", (empty_type,))
                .await?
                .into();
            let result: core::result::Result<bool, JavaError> = jvm
                .invoke_static("org/kwis/msp/media/Player", "play", "(Lorg/kwis/msp/media/BaseClip;Z)Z", (empty, false))
                .await;
            let JavaError::JavaException(exception) = result.expect_err("an unbuffered clip must throw");
            assert!(jvm.is_instance(&*exception, "org/kwis/msp/media/MediaUnavailableException"));

            Ok(())
        })
    }

    #[test]
    fn test_clip_compatibility_overloads_remain() -> Result<()> {
        run_jvm_test(Box::new([wie_midp::get_protos().into(), get_protos().into()]), |jvm| async move {
            let r#type: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "audio/test").await?.into();
            let clip: ClassInstanceRef<Clip> = jvm.new_class("org/kwis/msp/media/Clip", "(Ljava/lang/String;)V", (r#type,)).await?.into();

            let alloc_result: i32 = jvm.invoke_virtual(&clip, "allocPlayer", "()I", ()).await?;
            let media_stop_result: i32 = jvm.invoke_virtual(&clip, "mediaStop", "()I", ()).await?;

            assert_eq!(alloc_result, -9);
            assert_eq!(media_stop_result, -9);

            let play_result: core::result::Result<bool, JavaError> = jvm
                .invoke_static(
                    "org/kwis/msp/media/Player",
                    "play",
                    "(Lorg/kwis/msp/media/Clip;Z)Z",
                    (clip.clone(), false),
                )
                .await;

            let JavaError::JavaException(exception) = play_result.expect_err("unbuffered Clip play must throw");
            assert!(jvm.is_instance(&*exception, "org/kwis/msp/media/MediaUnavailableException"));
            assert!(jvm.is_instance(&*exception, "java/lang/RuntimeException"));

            let stopped: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "stop", "(Lorg/kwis/msp/media/Clip;)Z", (clip,))
                .await?;
            assert!(!stopped);

            Ok(())
        })
    }

    /// A clip nobody played is a clip `Player.stop` stopped nothing of.
    ///
    /// 전설의 마법학교2 reads that answer before it starts its BGM - told the
    /// clip it has only just loaded was already sounding, it parks and plays
    /// nothing. See [`super::Player::stop_clip`].
    #[test]
    fn stopping_a_clip_nobody_played_stops_nothing() -> Result<()> {
        run_jvm_test(Box::new([wie_midp::get_protos().into(), get_protos().into()]), |jvm| async move {
            let r#type: ClassInstanceRef<String> = JavaLangString::from_rust_string(&jvm, "audio/test").await?.into();
            let mut clip: ClassInstanceRef<Clip> = jvm.new_class("org/kwis/msp/media/Clip", "(Ljava/lang/String;)V", (r#type,)).await?.into();

            let sounding: bool = jvm.get_field(&clip, "__wiePlaying", "Z").await?;
            assert!(!sounding);

            let stopped: bool = jvm
                .invoke_static("org/kwis/msp/media/Player", "stop", "(Lorg/kwis/msp/media/Clip;)Z", (clip.clone(),))
                .await?;
            assert!(!stopped);

            // And a clip that is sounding stops being so, so the next stop
            // answers for what is true then rather than for what was.
            jvm.put_field(&mut clip, "__wiePlaying", "Z", true).await?;
            let _: i32 = jvm.invoke_virtual(&clip, "mediaStop", "()I", ()).await?;

            let sounding: bool = jvm.get_field(&clip, "__wiePlaying", "Z").await?;
            assert!(!sounding);

            Ok(())
        })
    }
}
