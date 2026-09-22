use alloc::{boxed::Box, sync::Arc, vec, vec::Vec};

use bytemuck::{Pod, Zeroable};
use core::sync::atomic::{AtomicBool, Ordering};

use wipi_types::wipic::WIPICWord;

use wie_util::{Result, WieError, read_generic, write_generic};

use spin::Mutex;

use crate::{WIPICResult, context::WIPICContext, method::MethodBody};

/// Top of the `MC_mdaClipSetVolume` range, and what a clip with no level of its
/// own reads back as.
const FULL_VOLUME: u8 = 100;

/// The level each clip is set to, by the clip's own address.
///
/// The level belongs to the clip, not to the load. `MC_mdaClipPutData` mints a
/// fresh audio handle every time a title hands it data, and routing a level
/// only to the handle that existed when it was set means a title that reloads
/// the clip loses it.
///
/// 이노티아1's volume screen does exactly that, once per step of its slider:
///
/// ```text
/// PutData -> handle 0x4 ; Play(0x4) ; SetVolume(0x4, 60)
/// ClipFree ; ClipCreate ; PutData -> handle 0x5 ; Play(0x5) ; SetVolume(0x5, 80)
/// ```
///
/// Every beep it plays is a clip loaded a moment earlier, so every one of them
/// played at full scale and the setting reached no sound at all - the first
/// step did not even have a handle to route to (`handle=0x0`, the clip's data
/// was not in yet). 붉은보석, whose volume has always worked, is the same calls
/// in the other order: it sets the level after `PutData` and before `Play`.
///
/// So the level is kept here and put back on whatever handle the clip is next
/// loaded under. The record is keyed by address and outlives the clip's own
/// free and re-create: a title tears the clip down and builds it again at the
/// same slot between two sounds, which is the very gap the level has to cross.
///
/// One of these belongs to one run of one title. It used to be a `static`, and
/// that was wrong twice over: the Android bridge starts and stops titles in a
/// single process, so a title began with whatever levels the title before it
/// left at the same addresses; and the tests here, which all build a context
/// handing out the same first clip address, could only keep out of each other's
/// way by clearing the `static` as they started - which is no help at all when
/// two of them run at once, each clearing what the other is midway through
/// using.
#[derive(Default)]
pub struct MediaState {
    clip_levels: Vec<(WIPICWord, u8)>,
}

/// How many clips carry a level, so a title that allocates without end cannot
/// grow this without end either. Titles here use one or two clip slots.
const CLIP_LEVELS_LIMIT: usize = 64;

impl MediaState {
    /// Remembers the level `clip` is set to. See [`MediaState`].
    fn remember_clip_level(&mut self, clip: WIPICWord, level: u8) {
        if let Some(entry) = self.clip_levels.iter_mut().find(|(candidate, _)| *candidate == clip) {
            entry.1 = level;
            return;
        }
        if self.clip_levels.len() >= CLIP_LEVELS_LIMIT {
            self.clip_levels.remove(0);
        }
        self.clip_levels.push((clip, level));
    }

    /// The level `clip` was last set to, if it was set at all.
    fn remembered_clip_level(&self, clip: WIPICWord) -> Option<u8> {
        self.clip_levels.iter().find(|(candidate, _)| *candidate == clip).map(|(_, level)| *level)
    }
}

pub type SharedMediaState = Arc<Mutex<MediaState>>;

pub fn new_state() -> SharedMediaState {
    Arc::new(Mutex::new(MediaState::default()))
}

/// Remembers the level `clip` is set to, on this run's own state.
fn remember_clip_level(context: &dyn WIPICContext, clip: WIPICWord, level: u8) {
    context.media_state().lock().remember_clip_level(clip, level);
}

/// The level `clip` was last set to on this run, if it was set at all.
fn remembered_clip_level(context: &dyn WIPICContext, clip: WIPICWord) -> Option<u8> {
    context.media_state().lock().remembered_clip_level(clip)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MdaClip {
    clip_id: i32,
    h_proc: i32,
    r#type: u8,
    in_use: u8, // bool
    _padding1: [u8; 2],
    dev_id: i32,

    x: i32,
    y: i32,
    w: i32,
    h: i32,
    mute: u8, // bool
    _padding2: [u8; 3],
    watermark: i32,
    position: i32,
    quality: i32,
    mode: i32,
    state: i32,
    penpot: i32,
    num_slave: i32,

    clip_save: WIPICWord, // MC_MdaClip**

    audio_tone_saved_len: i32,
    audio_tone_len: i32,
    audio_tone: WIPICWord,          // MC_MdaToneType*
    audio_tone_duration: WIPICWord, // M_Int32 *

    audio_freq_saved_len: i32,
    audio_freq_len: i32,
    audio_hi_freq: WIPICWord,       // M_Int32 *
    audio_low_freq: WIPICWord,      // M_Int32 *
    audio_freq_duration: WIPICWord, // M_Int32 *

    sound_data_saved_len: i32,
    sound_data_len: i32,
    sound_data: WIPICWord, // M_Byte *

    original_volume: i32,

    pos: i8,
    _padding3: [u8; 3],
    codec_config_data_size: i32,
    codec_config_data: WIPICWord, // M_Byte *
    tick_duration: i32,

    b_control: u8, // bool
    _padding4: [u8; 3],

    movie_record_size_width: i32,
    movie_record_size_height: i32,
    max_record_length: i32,

    temp_record_space: WIPICWord, // M_Byte *
    temp_record_space_size: i32,
    temp_record_size: i32,

    next_ptr: WIPICWord, // MC_MdaClip*

    mda_id: i32,
    device_info: i32,

    // not in sdk, for internal usage
    handle: u32,
}

pub async fn clip_create(context: &mut dyn WIPICContext, ptr_type: WIPICWord, buf_size: WIPICWord, callback: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_mdaClipCreate({ptr_type:#x}, {buf_size:#x}, {callback:#x})");

    let clip_address = context.alloc_raw(size_of::<MdaClip>() as u32)?;
    let clip = MdaClip {
        h_proc: callback as i32,
        in_use: 1,
        ..MdaClip::zeroed()
    };
    write_generic(context, clip_address, clip)?;

    tracing::info!("[media] MC_mdaClipCreate(type={ptr_type:#x}, buf_size={buf_size:#x}, cb={callback:#x}) -> clip {clip_address:#x}");

    Ok(clip_address)
}

pub async fn clip_free(context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_mdaClipFree({clip:#x})");

    // some app call clip free with null clip...
    if clip == 0 {
        return Ok(0);
    }

    context.free_raw(clip, size_of::<MdaClip>() as u32)?;

    Ok(0)
}

pub async fn clip_get_type(_context: &mut dyn WIPICContext, clip: WIPICWord, buf: WIPICWord, buf_size: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaClipGetType({clip:#x}, {buf:#x}, {buf_size:#x})");

    Ok(0)
}

pub async fn get_mute_state(_context: &mut dyn WIPICContext, source: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaGetMuteState({source:#x})");

    Ok(0)
}

pub async fn clip_get_info(
    _context: &mut dyn WIPICContext,
    clip: WIPICWord,
    command: WIPICWord,
    buf: WIPICWord,
    buf_size: WIPICWord,
) -> Result<WIPICWord> {
    tracing::warn!("stub OEMC_mdaClipGetInfo({clip:#x}, {command:#x}, {buf:#x}, {buf_size:#x})");

    Ok(0)
}

pub async fn clip_put_data(context: &mut dyn WIPICContext, ptr_clip: WIPICWord, buf: WIPICWord, buf_size: WIPICWord) -> Result<i32> {
    tracing::debug!("MC_mdaClipPutData({ptr_clip:#x}, {buf:#x}, {buf_size:#x})");

    let mut data = vec![0; buf_size as _];
    context.read_bytes(buf, &mut data)?;

    // First bytes help identify the format (SMAF "MMMD", etc.) when a title's
    // clips fail to load and stay silent.
    let magic: Vec<u8> = data.iter().take(4).copied().collect();
    let handle = context.system().audio().load_smaf(&data);
    if let Err(x) = handle {
        tracing::error!("[media] MC_mdaClipPutData(clip={ptr_clip:#x}, size={buf_size:#x}, magic={magic:02x?}) load_smaf FAILED: {x:?}");
        return Ok(0);
    }

    let handle = handle.unwrap();
    tracing::info!("[media] MC_mdaClipPutData(clip={ptr_clip:#x}, size={buf_size:#x}, magic={magic:02x?}) load_smaf -> handle {handle:#x}");

    // Titles that never allocate a clip object load their audio under the
    // implicit clip 0; bind the loaded handle to the default player so the
    // clip-0 play/volume/stop paths can reach it (otherwise every such effect is
    // silent). Titles that use real clip objects store the handle in the object.
    // The level the title set on this clip belongs to the clip, so put it on
    // the handle the data just arrived under. See `MediaState`.
    if let Some(level) = remembered_clip_level(context, ptr_clip) {
        tracing::info!("[media] clip {ptr_clip:#x} carries level {level} onto handle {handle:#x}");
        let _ = context.system().audio().set_volume(handle, level);
    }

    if ptr_clip == 0 {
        context.system().audio().set_default_clip(handle);
        return Ok(buf_size as _);
    }

    let mut clip: MdaClip = read_generic(context, ptr_clip)?;
    clip.handle = handle;
    write_generic(context, ptr_clip, clip)?;

    Ok(buf_size as _)
}

pub async fn clip_get_data(_context: &mut dyn WIPICContext, clip: WIPICWord, buf: WIPICWord, buf_size: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaClipGetData({clip:#x}, {buf:#x}, {buf_size:#x})");

    Ok(0)
}

pub async fn clip_set_position(_context: &mut dyn WIPICContext, clip: WIPICWord, ms: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaClipSetPosition({clip:#x}, {ms:#x})");

    Ok(0)
}

/// The level a clip is at, which is the level [`clip_set_volume`] last routed
/// to its handle.
///
/// This answered 0 as a stub, and 0 is the one answer that silences a title:
/// they read the level to put it back. 영웅서기5 sets a clip to 20, reads it, and
/// sets what it read - so every effect it loaded played at zero, and the game
/// ran mute. A clip whose data has not been loaded yet has no level of its own,
/// so answer full scale rather than the silence a zero would restore.
pub async fn clip_get_volume(context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    let handle = if clip == 0 {
        // Default-player titles ask about the clip-0 handle, as they set it.
        context.system().audio().default_clip()
    } else {
        // A clip record is zeroed at creation and only gets its handle from
        // `MC_mdaClipPutData`, so a 0 here is "nothing loaded" - no handle is
        // ever 0.
        let mda_clip: MdaClip = read_generic(context, clip)?;
        (mda_clip.handle != 0).then_some(mda_clip.handle)
    };

    // The clip's own level first: a title that sets one before loading anything
    // reads back what it set, not the full scale an absent handle would give.
    let level = remembered_clip_level(context, clip)
        .or_else(|| handle.and_then(|handle| context.system().audio().get_volume(handle).ok()))
        .unwrap_or(FULL_VOLUME);

    tracing::info!("[media] MC_mdaClipGetVolume(clip={clip:#x}) handle={handle:?} level={level}");

    Ok(level as WIPICWord)
}

pub async fn clip_set_volume(context: &mut dyn WIPICContext, clip: WIPICWord, volume: WIPICWord) -> Result<WIPICWord> {
    let level = (volume & 0xFF).min(FULL_VOLUME as WIPICWord) as u8;

    // The clip keeps the level whether or not it has anything loaded right now,
    // so the next load plays at it. See `MediaState`.
    remember_clip_level(context, clip, level);

    if clip == 0 {
        // Default-player titles set the volume of the clip-0 handle.
        let default = context.system().audio().default_clip();
        if let Some(handle) = default {
            let _ = context.system().audio().set_volume(handle, level);
        }
        return Ok(0);
    }

    // The clip carries the audio handle its data was loaded under; route the
    // requested level (0..=100) to that handle so the rendered stream plays at
    // the volume the title asked for. Titles set this well below full scale
    // (Zenonia uses 50) to leave headroom, and honouring it keeps a bright,
    // near-full-scale sequence from being driven into the output limiter and
    // sounding harsh.
    let mda_clip: MdaClip = read_generic(context, clip)?;
    let handle = mda_clip.handle;
    tracing::info!("[media] MC_mdaClipSetVolume(clip={clip:#x}, volume={volume:#x}) handle={handle:#x} level={level}");

    let _ = context.system().audio().set_volume(handle, level);

    Ok(0)
}

/// The handset's media volume.
///
/// A title reads this to restore it, or to decide whether there is any point
/// playing at all, so before anything sets it the answer is full scale rather
/// than the zero that would read as muted.
pub async fn get_volume(context: &mut dyn WIPICContext) -> Result<WIPICWord> {
    let level = context.system().audio().master_volume();
    tracing::debug!("MC_mdaGetVolume -> {level}");

    Ok(level as WIPICWord)
}

/// `MC_mdaSetVolume(level)`, the handset's overall media volume.
///
/// KTF keeps it at media slot 15, where the level walks the scale on its own -
/// 겟앰프드 sets it in `startApp`, before it has a clip to set anything on, and
/// this slot answering "unimplemented" ended the run there with the title
/// screen never drawn.
///
/// This is the handset's own level, not a clip's, so it scales what a title
/// set per sound rather than replacing it - the level a clip plays at is its
/// own times this one. Taken and dropped, as it used to be, a title whose sound
/// menu drives only this control has no volume at all: 드래곤로드's slider walks
/// 20, 30, 40 ... 90, stopping and replaying a sample at each step to let you
/// hear it, and never once sets a clip's own volume. Every one of those steps
/// sounded the same.
pub async fn set_volume(context: &mut dyn WIPICContext, level: WIPICWord) -> Result<WIPICWord> {
    let level = (level & 0xFF).min(FULL_VOLUME as WIPICWord) as u8;
    tracing::debug!("MC_mdaSetVolume({level})");

    context.system().audio().set_master_volume(level);

    Ok(0)
}

/// The playbacks a completion watcher is already waiting on.
///
/// A playback is named by the stop flag it shares with the audio layer, which
/// is the one thing a re-play of the same looping clip keeps. An entry lives
/// exactly as long as its watcher, which holds that flag alive, so an address
/// here is never a stale one.
static WATCHED: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// How many playbacks can be watched at once, so a title that plays without
/// ever stopping cannot grow this without end.
const WATCHED_LIMIT: usize = 64;

/// A claim on watching one playback, given up when the watcher is dropped.
struct WatchedPlayback(usize);

impl WatchedPlayback {
    /// Takes the claim, or answers `None` if this playback is watched already
    /// or there are too many watched at once.
    fn claim(stopped: &Arc<AtomicBool>) -> Option<Self> {
        let key = Arc::as_ptr(stopped) as usize;
        let mut watched = WATCHED.lock();

        if watched.contains(&key) || watched.len() >= WATCHED_LIMIT {
            return None;
        }
        watched.push(key);

        Some(Self(key))
    }
}

impl Drop for WatchedPlayback {
    fn drop(&mut self) {
        WATCHED.lock().retain(|watched| *watched != self.0);
    }
}

/// The clip's playback has begun. A title has no call that asks a clip whether
/// it is sounding, so this is how it knows.
const MDA_EVENT_PLAYING: WIPICWord = 2;

/// The clip reached the end of its media - or was stopped, which ends it too.
const MDA_EVENT_END_OF_MEDIA: WIPICWord = 3;

pub async fn play(context: &mut dyn WIPICContext, ptr_clip: WIPICWord, repeat: WIPICWord) -> Result<i32> {
    if ptr_clip == 0 {
        // Default-player titles (clip 0) play the handle their MC_mdaClipPutData
        // bound to the default clip. No clip object means no completion
        // callback; those titles drive stop/replay themselves.
        let default = context.system().audio().default_clip();
        if let Some(handle) = default {
            tracing::info!("[media] MC_mdaPlay(clip=0, repeat={repeat}) default handle={handle:#x}");
            let system = context.system();
            if let Err(error) = system.audio().play_with_completion(system, handle, repeat != 0) {
                tracing::error!("Failed to play default clip: {error:?}");
            }
        }
        return Ok(0);
    }

    let clip: MdaClip = read_generic(context, ptr_clip)?;
    let callback = clip.h_proc as WIPICWord;
    tracing::info!("[media] MC_mdaPlay(clip={ptr_clip:#x}, repeat={repeat}) handle={:#x}", clip.handle);

    let completed = {
        let system = context.system();
        system.audio().play_with_completion(system, clip.handle, repeat != 0)
    };

    let playback = match completed {
        Ok(playback) => playback,
        Err(error) => {
            tracing::error!("Failed to play audio: {error:?}");
            return Ok(0);
        }
    };
    let (completed, stopped, superseded) = (playback.completed, playback.stopped, playback.superseded);

    // The clip is told that it is playing.
    //
    // A title reads its clip's state out of the events it has been handed -
    // there is no call that asks - so one that is never told playback began
    // reads its own music as not playing. 놈ZERO's menu tests exactly that,
    // every frame:
    //
    //     0x47f44  ldrb r3, [r0, #0x11]   ; the last event it was handed
    //     0x47f46  lsrs r2, r3, #4
    //     0x47f48  cmp  r2, #0            ; 0 - never started
    //     0x47f4c  cmp  r2, #3            ; 3 - ended
    //
    // and on a no it asks for its background music again. With nothing but the
    // ending ever reported the answer was always no, so it tore the track down
    // and rebuilt it twice a second for as long as it ran - which is what its
    // music sounded like. Told that it is playing, it leaves the track alone:
    // twenty thousand ticks of play came to 168 plays and now come to 3.
    //
    // The reference reports the same way, one state change at a time, out of
    // `setMediaState` - `MC_mdaStop` hands the callback 0, `MC_mdaPause` 2,
    // `MC_mdaResume` 1 - and queues them rather than calling in from inside the
    // service it is serving, which is why this is spawned rather than called
    // here.
    if callback != 0 {
        struct PlaybackStartedCallback {
            callback: WIPICWord,
            clip: WIPICWord,
        }

        #[async_trait::async_trait]
        impl MethodBody<WieError> for PlaybackStartedCallback {
            async fn call(&self, context: &mut dyn WIPICContext, _: Box<[WIPICWord]>) -> Result<WIPICResult> {
                tracing::debug!("MC_mdaPlay started callback({:#x}, event={MDA_EVENT_PLAYING})", self.callback);
                context.call_function(self.callback, &[self.clip, MDA_EVENT_PLAYING]).await?;

                Ok(WIPICResult { results: Vec::new() })
            }
        }

        context.spawn(Box::new(PlaybackStartedCallback { callback, clip: ptr_clip }))?;
    }

    // A looping clip is watched too. It never reaches the end of its media on
    // its own, but it does end when the title stops it, and a title that drives
    // its sound engine off that event needs telling either way - KBO 프로야구
    // 2010 stops its menu track to move to the next one and waits to be told it
    // ended, so leaving a looping clip unwatched left the game as silent from
    // the menu on as it had been from the title screen before.
    //
    // A playback is watched once rather than once per play: a looping clip
    // re-played with identical data continues the playback the first play
    // started and shares its flags, so watching per call would pile up tasks
    // that all report the one ending.
    if callback != 0
        && let Some(watch) = WatchedPlayback::claim(&stopped)
    {
        /// How often a clip's completion flag is read while it plays.
        const COMPLETION_POLL_PERIOD: u64 = 16;

        struct PlaybackCompletedCallback {
            completed: Arc<AtomicBool>,
            stopped: Arc<AtomicBool>,
            superseded: Arc<AtomicBool>,
            callback: WIPICWord,
            clip: WIPICWord,
            /// Held for as long as this watcher lives, so the playback is not
            /// watched twice over.
            _watch: WatchedPlayback,
        }

        #[async_trait::async_trait]
        impl MethodBody<WieError> for PlaybackCompletedCallback {
            async fn call(&self, context: &mut dyn WIPICContext, _: Box<[WIPICWord]>) -> Result<WIPICResult> {
                while !self.completed.load(Ordering::Acquire) && !self.stopped.load(Ordering::Acquire) {
                    // A frame, not a millisecond: the audio layer writes these
                    // flags from a watcher of its own that polls at 50ms, so a
                    // finer read cannot see anything sooner and only takes
                    // executor time away from the guest.
                    context.system().sleep(COMPLETION_POLL_PERIOD).await;
                }

                // A playback the title replaced with another is not reported.
                //
                // It ended, but the title is the one that ended it by asking for
                // the next track, and it is not waiting to hear about the one it
                // moved off. 놈ZERO is: told the track it had just replaced had
                // ended, its handler tears the clip down and its tick builds it
                // again, which replaces the track once more - so its music
                // restarted a dozen times a second and came out in fragments.
                if self.superseded.load(Ordering::Acquire) {
                    tracing::debug!("MC_mdaPlay superseded, nothing to report for clip {:#x}", self.clip);

                    return Ok(WIPICResult { results: Vec::new() });
                }

                // A stopped clip has ended too, and is told so.
                //
                // This used to return here without telling the title anything,
                // on the reasoning that a clip the title stopped itself needs no
                // end-of-media. But a title's sound engine is a state machine
                // whose only way forward is that event, and stopping is how it
                // gets from one sound to the next.
                //
                // KBO 프로야구 2010 is built exactly that way: to play a sound it
                // records the one it wants, stops whatever is sounding and waits
                // to be told the clip ended before starting it. Its handler
                // takes events 1, 2, 3 and 9 and ignores everything else - the
                // `-1` MC_mdaStop answers with is nothing to it - so with this
                // event withheld it played its opening jingle, asked for a stop
                // every frame for the rest of the run, and never sounded again.
                let ended = if self.stopped.load(Ordering::Acquire) { "stopped" } else { "completed" };

                tracing::debug!("MC_mdaPlay {ended} callback({:#x}, event={MDA_EVENT_END_OF_MEDIA})", self.callback);
                context.call_function(self.callback, &[self.clip, MDA_EVENT_END_OF_MEDIA]).await?;

                Ok(WIPICResult { results: Vec::new() })
            }
        }

        context.spawn(Box::new(PlaybackCompletedCallback {
            _watch: watch,
            completed,
            stopped,
            superseded,
            callback,
            clip: ptr_clip,
        }))?;
    }

    Ok(0)
}

/// `MC_mdaClipControl` (WIPI-C index `0x4b6`) — the player-path play/control
/// call that titles like Zenonia use instead of `MC_mdaPlay`. The sequence is
/// `ClipAllocPlayer` → `ClipSetVolume` → `ClipControl(clip, cmd, …)` →
/// `ClipFreePlayer`; the clip's audio was already loaded by `ClipPutData`
/// (`load_smaf`). Without this the loaded clip is never played and those effects
/// are silent. `cmd` selects the play mode: `0x31` loops (BGM), `0x30` plays
/// once (SFX); other commands are logged and ignored for now.
/// `MC_mdaClipClearData` (service 0x4b6), which drops whatever a clip has
/// buffered.
///
/// A clip's data arrives whole through `MC_mdaClipPutData` and is decoded into
/// an audio handle there, so there is no partial buffer to drop and the handle
/// stays valid for the replay that follows. Accepted and logged: this is the
/// service 0x4b6 actually is - it was routed to `clip_control` and logged under
/// that name until the reference firmware's service table gave both their real
/// numbers (`MC_mdaClipControl` is 0x4ca).
pub async fn clip_clear_data(_context: &mut dyn WIPICContext, clip: WIPICWord, a1: WIPICWord, a2: WIPICWord, a3: WIPICWord) -> Result<WIPICWord> {
    tracing::info!("[media] MC_mdaClipClearData(clip={clip:#x}, {a1:#x}, {a2:#x}, {a3:#x})");

    Ok(0)
}

pub async fn clip_control(_context: &mut dyn WIPICContext, clip: WIPICWord, cmd: WIPICWord, arg1: WIPICWord, arg2: WIPICWord) -> Result<WIPICWord> {
    // Diagnostic (INFO): playing here blindly double-triggered clips the game
    // also drives another way and layered them, so this is a logging no-op for
    // now. The media-path INFO trace (clip_create/put_data/play) shows the real
    // protocol; the player path is wired for real once that is understood.
    tracing::info!("[media] MC_mdaClipControl(clip={clip:#x}, cmd={cmd:#x}, arg1={arg1:#x}, arg2={arg2:#x})");

    Ok(0)
}

pub async fn clip_alloc_player(_context: &mut dyn WIPICContext, clip: WIPICWord, param: WIPICWord) -> Result<WIPICWord> {
    // Returning a non-null handle here makes titles read the player as "already
    // set up" and skip the following MC_mdaPlay, leaving the logo jingle and
    // looping BGM silent. Keep the validated stub so MC_mdaPlay drives playback.
    tracing::warn!("stub MC_mdaClipAllocPlayer({clip:#x}, {param:#x})");

    Ok(0)
}

pub async fn clip_free_player(context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    // Titles free the player to stop the clip - the game's stop for a looping
    // track. Without this a `repeat=1` BGM plays forever and every new track
    // stacks another endless loop on top (garbled, ever-louder audio); the
    // player-path stop (ClipControl + this) is where the sequence is meant to
    // end. Stop the clip's playback by its loaded handle.
    if clip == 0 {
        return Ok(0);
    }

    let mda_clip: MdaClip = read_generic(context, clip)?;
    let handle = mda_clip.handle;
    tracing::info!("[media] MC_mdaClipFreePlayer(clip={clip:#x}) stop handle {handle:#x}");
    context.system().audio().stop(handle);

    Ok(0)
}

pub async fn vibrator(context: &mut dyn WIPICContext, level: i32, timeout: i32) -> Result<WIPICWord> {
    tracing::debug!("MC_mdaVibrator({level}, {timeout})");

    let duration_ms = timeout.max(0) as u64;
    let intensity = (level.clamp(0, 10) * 10) as u8;
    context.system().platform().vibrate(duration_ms, intensity);

    Ok(0)
}

pub async fn set_mute_state(_context: &mut dyn WIPICContext, source: i32, b_mute: i32) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaSetMuteState({source:#x}, {b_mute})");

    Ok(0)
}

pub async fn pause(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaPause({clip:#x})");

    Ok(0)
}

pub async fn resume(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaResume({clip:#x})");

    Ok(0)
}

pub async fn stop(context: &mut dyn WIPICContext, ptr_clip: WIPICWord) -> Result<WIPICWord> {
    tracing::debug!("MC_mdaStop({ptr_clip:#x})");

    if ptr_clip == 0 {
        // Default-player titles stop the clip-0 handle before loading the next.
        let default = context.system().audio().default_clip();
        if let Some(handle) = default {
            context.system().audio().stop(handle);
        }
        return Ok(0);
    }

    let clip: MdaClip = read_generic(context, ptr_clip)?;

    // One ending, reported once.
    //
    // A stopped playback ends, and the watcher `MC_mdaPlay` leaves behind says
    // so - see there. This used to say so as well, from in here and before the
    // stop had even returned, so a title heard the same ending twice: once as
    // `-1` inside its own stop, and again as end-of-media a frame later.
    //
    // 놈ZERO acts on both. Its clip handler reads `-1` as the end (it maps the
    // one to the other outright, at 0x47db4) and tears the clip down, and the
    // end-of-media that follows tears it down again, so a title that should
    // have been left playing its looping track rebuilt and restarted it a
    // dozen times a second - twenty seconds of play came to six hundred plays
    // where the ending told once is two hundred.
    //
    // Nothing is lost by leaving it to the watcher: a title that reads `-1`
    // reads it as the end, and one that waits for end-of-media (KBO 프로야구
    // 2010) only ever hears that.
    context.system().audio().stop(clip.handle);

    Ok(0)
}

pub async fn record(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaRecord({clip:#x})");

    Ok(0)
}

pub async fn unk7(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaUnk7({clip:#x})");

    Ok(0)
}

pub async fn unk17(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaUnk17({clip:#x})");

    Ok(0)
}

pub async fn unk18(_context: &mut dyn WIPICContext, clip: WIPICWord) -> Result<WIPICWord> {
    tracing::warn!("stub MC_mdaUnk18({clip:#x})");

    Ok(0)
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, vec::Vec};

    use test_utils::TestPlatform;
    use wie_backend::{DefaultTaskRunner, System};
    use wie_util::ByteWrite;

    use crate::context::{WIPICContext, test::TestContext};

    use core::sync::atomic::AtomicBool;

    use alloc::sync::Arc;

    use super::{FULL_VOLUME, MdaClip, WatchedPlayback, clip_create, clip_get_volume, clip_put_data, clip_set_volume, play, read_generic, stop};

    /// What the clip callback was called with. A static, because the test
    /// context takes a plain function rather than a closure.
    static CALLS: spin::Mutex<Vec<(u32, u32)>> = spin::Mutex::new(Vec::new());

    fn record(_address: u32, args: &[u32]) -> u32 {
        CALLS.lock().push((args[0], args[1]));

        0
    }

    fn test_context() -> TestContext {
        // Every context here hands out the same first clip address, so the
        // levels have to belong to the context rather than to the process -
        // see `MediaState`. Each one built here starts with its own, empty.
        let system = System::new(Box::new(TestPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner);
        TestContext::with_system(system)
    }

    /// The audio handle a clip's data is loaded under.
    fn handle_of(context: &TestContext, clip: u32) -> u32 {
        let clip: MdaClip = read_generic(context, clip).unwrap();

        clip.handle
    }

    /// 이노티아1's volume screen, in the order it actually calls: it sets the
    /// level on the clip it has just played, then tears that clip down and
    /// loads the next beep into a fresh one. The level used to live on the
    /// audio handle `MC_mdaClipPutData` minted, so every beep played at full
    /// scale and the setting reached no sound at all.
    #[futures_test::test]
    async fn a_clip_keeps_its_level_when_its_data_is_loaded_again() {
        let mut context = test_context();

        let clip = clip_create(&mut context, 0, 0x793, 0).await.unwrap();
        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap();
        clip_set_volume(&mut context, clip, 20).await.unwrap();

        let played_at = handle_of(&context, clip);
        assert_eq!(context.system().audio().get_volume(played_at).unwrap(), 20);

        // The next beep: the same clip slot, loaded again, behind a new handle.
        clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap();

        let reloaded = handle_of(&context, clip);
        assert_ne!(reloaded, played_at, "a reload mints a new handle");
        assert_eq!(
            context.system().audio().get_volume(reloaded).unwrap(),
            20,
            "the level the title set on this clip has to follow it onto the handle it is loaded under"
        );
    }

    /// And a level set before the clip holds anything reaches the load that
    /// follows: 이노티아1's first step sets one while the clip is still empty,
    /// which had no handle to route to at all.
    #[futures_test::test]
    async fn a_level_set_before_a_clip_holds_anything_still_takes() {
        let mut context = test_context();

        let clip = clip_create(&mut context, 0, 0x793, 0).await.unwrap();
        clip_set_volume(&mut context, clip, 40).await.unwrap();
        assert_eq!(handle_of(&context, clip), 0, "nothing is loaded yet");

        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap();

        let loaded = handle_of(&context, clip);
        assert_eq!(context.system().audio().get_volume(loaded).unwrap(), 40);
    }

    /// A level belongs to the run that set it and reaches no other.
    ///
    /// The Android bridge starts and stops titles inside one process, and the
    /// allocator hands the second title the same first clip address as the
    /// first - so while these levels lived in a `static`, a title opened with
    /// whatever the title before it had left there.
    #[futures_test::test]
    async fn a_level_reaches_no_other_run() {
        let mut first = test_context();

        let clip = clip_create(&mut first, 0, 0x793, 0).await.unwrap();
        clip_set_volume(&mut first, clip, 20).await.unwrap();
        assert_eq!(clip_get_volume(&mut first, clip).await.unwrap(), 20);

        // The next title up, at the address the last one was handed.
        let mut second = test_context();
        let same_clip = clip_create(&mut second, 0, 0x793, 0).await.unwrap();
        assert_eq!(same_clip, clip, "the second run is handed the same address");

        assert_eq!(
            clip_get_volume(&mut second, same_clip).await.unwrap(),
            FULL_VOLUME as u32,
            "a clip nothing has set reads back full scale, not the level another run left"
        );
    }

    /// 영웅서기5's own sequence: it sets an effect's level, reads it back, and
    /// sets what it read. While the read answered 0 every effect it loaded
    /// played at zero and the game ran mute.
    #[futures_test::test]
    async fn a_clip_reads_back_the_level_it_was_set_to() {
        let mut context = test_context();

        let clip = clip_create(&mut context, 0, 0x793, 0).await.unwrap();
        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        assert_eq!(clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap(), 8);

        assert_eq!(clip_set_volume(&mut context, clip, 20).await.unwrap(), 0);
        assert_eq!(clip_get_volume(&mut context, clip).await.unwrap(), 20);

        // And what it reads is what it can set back.
        let level = clip_get_volume(&mut context, clip).await.unwrap();
        assert_eq!(clip_set_volume(&mut context, clip, level).await.unwrap(), 0);
        assert_eq!(clip_get_volume(&mut context, clip).await.unwrap(), 20);
    }

    /// A looping clip that is stopped is told its media ended, the same as a
    /// one-shot that played out.
    ///
    /// A looping clip never reaches the end of its own accord, so it used to be
    /// left unwatched and a stop told the title nothing it could act on. KBO
    /// 프로야구 2010 drives its sound engine off that event - it stops the track
    /// it is playing and waits to be told it ended before starting the next -
    /// so its menu stayed as silent as its title screen had been.
    #[futures_test::test]
    async fn a_looping_clip_that_is_stopped_reports_its_end() {
        CALLS.lock().clear();

        let mut context = test_context();
        context.set_guest_function(record);

        // The third argument is the clip's callback; a title that wants to be
        // told anything passes one.
        let clip = clip_create(&mut context, 0, 0x793, 0x3d89).await.unwrap();
        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap();

        play(&mut context, clip, 1).await.unwrap();
        assert_eq!(
            context.spawned(),
            2,
            "a looping clip is told it is playing, and is watched so its end can be reported"
        );

        stop(&mut context, clip).await.unwrap();

        // The watcher is what reports the end; run it, as the executor would.
        for body in context.take_spawned() {
            body.call(&mut context, Box::new([])).await.unwrap();
        }

        let calls = CALLS.lock().clone();
        assert!(
            calls.contains(&(clip, 3)),
            "the title has to be told the clip ended, not only that it was stopped: {calls:?}"
        );
    }

    /// A clip that starts playing is told so.
    ///
    /// A title has no call that asks a clip whether it is sounding - it reads
    /// the last event it was handed - so one that is never told playback began
    /// reads its own music as stopped. 놈ZERO's menu asks that question every
    /// frame and rebuilds its background music on a no, so with only the ending
    /// ever reported its music restarted twice a second for as long as it ran.
    #[futures_test::test]
    async fn a_clip_that_starts_playing_is_told_so() {
        CALLS.lock().clear();

        let mut context = test_context();
        context.set_guest_function(record);

        let clip = clip_create(&mut context, 0, 0x793, 0x47ded).await.unwrap();
        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        clip_put_data(&mut context, clip, 0x1000, 8).await.unwrap();

        play(&mut context, clip, 1).await.unwrap();

        // The report is queued rather than made from inside the play, as the
        // reference queues its own state changes. Run it - and only it, since
        // the watcher queued behind it waits for an ending this test never
        // brings about.
        let queued = context.take_spawned().into_iter().next().unwrap();
        queued.call(&mut context, Box::new([])).await.unwrap();

        let calls = CALLS.lock().clone();
        assert!(calls.contains(&(clip, 2)), "the title has to be told its clip is playing: {calls:?}");
    }

    /// One playback is watched once, and watching it again is possible only
    /// once the first watcher has gone.
    ///
    /// A looping clip re-played with identical data continues the playback the
    /// first play started and shares its flags, so without this a title that
    /// re-plays its background track every frame would pile up watchers that
    /// all report the one ending.
    #[test]
    fn a_playback_is_watched_once() {
        let playback = Arc::new(AtomicBool::new(false));
        let other = Arc::new(AtomicBool::new(false));

        let claim = WatchedPlayback::claim(&playback).expect("the first watcher takes it");
        assert!(WatchedPlayback::claim(&playback).is_none(), "the same playback is not watched twice");

        // A different playback is its own business.
        assert!(WatchedPlayback::claim(&other).is_some());

        // And once the watcher is gone the playback can be watched again.
        drop(claim);
        assert!(WatchedPlayback::claim(&playback).is_some());
    }

    /// A clip with nothing loaded has no level of its own. Answer full scale:
    /// a title restoring what it read must not restore silence. It must also
    /// not read back some other clip's level - a clip record is zeroed at
    /// creation, so the handle it has not been given yet must not name one.
    #[futures_test::test]
    async fn a_clip_with_no_data_reads_back_full_volume() {
        let mut context = test_context();

        let loaded = clip_create(&mut context, 0, 0x793, 0).await.unwrap();
        context.write_bytes(0x1000, b"MMMD\0\0\0\0").unwrap();
        clip_put_data(&mut context, loaded, 0x1000, 8).await.unwrap();
        clip_set_volume(&mut context, loaded, 20).await.unwrap();

        let empty = clip_create(&mut context, 0, 0x793, 0).await.unwrap();

        assert_eq!(clip_get_volume(&mut context, empty).await.unwrap(), FULL_VOLUME as _);
    }
}
