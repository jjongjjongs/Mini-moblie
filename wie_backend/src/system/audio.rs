use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use smaf_player::{SmafEvent, parse_smaf};

use crate::{System, audio_sink::AudioSink};

pub type AudioHandle = u32;
#[derive(Debug)]
pub enum AudioError {
    InvalidHandle,
    InvalidAudio,
}

enum AudioFile {
    Smaf(Vec<u8>),
}

pub struct Audio {
    sink: Arc<Box<dyn AudioSink>>,
    files: BTreeMap<AudioHandle, AudioFile>,
    volumes: BTreeMap<AudioHandle, Arc<AtomicU8>>,
    /// The handset's own media level, which scales every clip's.
    ///
    /// Distinct from a clip's volume, which is the clip's alone: this is the
    /// one the user sets in the handset's sound menu, and the reference applies
    /// it over whatever a title set per sound. See [`Self::set_master_volume`].
    master: u8,
    playing: BTreeMap<AudioHandle, Arc<AtomicBool>>,
    last_audio_handle: AudioHandle,
    default_clip_handle: Option<AudioHandle>,
    /// The clip currently rendering through the sink's pre-rendered path, kept
    /// so a title that tears the player down and rebuilds it every frame with
    /// byte-identical looping data (시드 restarts its BGM in `paint`) does not
    /// restart the audio from zero each time - the identical re-play continues
    /// the existing playback instead. See [`Self::play_with_completion`].
    active: Option<ActiveSmaf>,
    /// Whether the deferred-stop reaper task has been spawned (once per Audio).
    reaper_started: bool,
}

/// A pre-rendered SMAF clip playing through the sink, tracked so an immediate
/// identical re-play is seamless and a real stop is honored after a short grace.
struct ActiveSmaf {
    /// Hash of the SMAF bytes, to recognize a byte-identical re-play.
    hash: u64,
    repeat: bool,
    /// The handle the title currently knows the clip by (remapped on each
    /// seamless re-play, since it allocates a fresh handle every time). Used to
    /// match the title's `stop`, which names this latest handle.
    handle: AudioHandle,
    /// The handle the sink actually plays the stream under - fixed at the first
    /// `play_smaf` and never remapped, because a seamless re-play keeps that
    /// original stream. Stopping the clip must target THIS handle, not the
    /// latest one the title allocated (which the sink never played).
    sink_handle: AudioHandle,
    stop_flag: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    /// Set instead of nothing when this playback is replaced by another rather
    /// than stopped, so its ending is not reported as one. See
    /// [`Audio::flush_active`].
    superseded: Arc<AtomicBool>,
    /// Reaper polls remaining before a deferred stop actually stops the sink;
    /// `None` while playing. A re-play clears it, so continuous re-play never
    /// stops; a genuine stop with no re-play flushes after the grace.
    pending_stop_polls: Option<u32>,
}

/// Reaper poll interval and the grace (in polls) before a deferred stop takes
/// effect - long enough to bridge a per-frame tear-down/rebuild, short enough
/// that a real stop is barely audible as a tail.
const REAPER_POLL_MS: u64 = 100;
const PENDING_STOP_GRACE_POLLS: u32 = 3;

/// The volume a clip has until a title says otherwise, which is what the
/// reference's clip record is created holding.
pub const FULL_VOLUME: u8 = 100;

/// A playback in flight, and the three things a caller watching for its ending
/// needs to tell apart.
pub struct Playback {
    /// Set when the clip reached the end of its media on its own.
    pub completed: Arc<AtomicBool>,
    /// Set when the playback is over, however it came to be over.
    pub stopped: Arc<AtomicBool>,
    /// Set when it is over because the title started another in its place,
    /// rather than because the title stopped it. The title asked for the
    /// replacement, so the ending is not news to it.
    pub superseded: Arc<AtomicBool>,
}

fn smaf_hash(data: &[u8]) -> u64 {
    // FNV-1a, enough to tell one clip's bytes from another.
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

impl Audio {
    pub fn new(sink: Box<dyn AudioSink>) -> Self {
        Self {
            sink: Arc::new(sink),
            files: BTreeMap::new(),
            volumes: BTreeMap::new(),
            playing: BTreeMap::new(),
            // Handles start at 1 so 0 can mean "nothing loaded here". A WIPI-C
            // clip record is zeroed when the title creates it and only gets its
            // handle from `MC_mdaClipPutData`, so a 0 there has to be tellable
            // from a real handle - otherwise a clip with no data reads back the
            // first-ever clip's volume instead of its own default.
            last_audio_handle: 1,
            master: FULL_VOLUME,
            default_clip_handle: None,
            active: None,
            reaper_started: false,
        }
    }

    /// The handle bound to the implicit "clip 0" default player.
    ///
    /// Some titles never allocate a clip object and drive the whole MC_mda*
    /// sequence with `clip == 0` - a single reusable player (나는마왕이다2, for
    /// one, loads and plays every effect and BGM this way). The clip's audio
    /// still has to live under a real handle for the sink to play it, so the one
    /// most recently loaded under clip 0 is remembered here and read back by the
    /// clip-0 play/volume/stop paths.
    pub fn set_default_clip(&mut self, handle: AudioHandle) {
        self.default_clip_handle = Some(handle);
    }

    pub fn default_clip(&self) -> Option<AudioHandle> {
        self.default_clip_handle
    }

    pub fn load_smaf(&mut self, data: &[u8]) -> Result<AudioHandle, AudioError> {
        let audio_handle = self.last_audio_handle;

        self.last_audio_handle += 1;
        self.files.insert(audio_handle, AudioFile::Smaf(data.to_vec()));
        self.volumes.insert(audio_handle, Arc::new(AtomicU8::new(FULL_VOLUME)));

        Ok(audio_handle)
    }

    pub fn play(&mut self, system: &System, audio_handle: AudioHandle, repeat: bool) -> Result<(), AudioError> {
        self.play_with_completion(system, audio_handle, repeat)?;

        Ok(())
    }

    /// Plays a clip and hands back the three things a caller watching for its
    /// ending needs to tell apart: whether it played out, whether it was
    /// stopped, and whether it was simply replaced by another play.
    pub fn play_with_completion(&mut self, system: &System, audio_handle: AudioHandle, repeat: bool) -> Result<Playback, AudioError> {
        let data = match self.files.get(&audio_handle) {
            Some(AudioFile::Smaf(data)) => data.clone(),
            None => return Err(AudioError::InvalidHandle),
        };
        let volume = self.volumes.get(&audio_handle).ok_or(AudioError::InvalidHandle)?.clone();
        let hash = smaf_hash(&data);

        // Seamless re-play: a looping clip torn down and rebuilt with identical
        // bytes (시드 restarts its BGM every paint) keeps the existing sink
        // playback instead of restarting from zero. Adopt the fresh handle and
        // cancel any deferred stop.
        if repeat
            && let Some(active) = self.active.as_mut()
            && active.repeat
            && active.hash == hash
        {
            active.pending_stop_polls = None;
            active.handle = audio_handle;
            let playback = Playback {
                completed: active.completed.clone(),
                stopped: active.stop_flag.clone(),
                superseded: active.superseded.clone(),
            };
            // The sink still plays this clip under the handle it first started
            // on, so the volume has to go there rather than to the fresh handle
            // the title just allocated - see `ActiveSmaf::sink_handle`.
            let sink_handle = active.sink_handle;
            self.sink.set_clip_volume(sink_handle, self.effective(volume.load(Ordering::Relaxed)));
            self.playing.insert(audio_handle, playback.stopped.clone());
            return Ok(playback);
        }

        // A different looping clip replaces the active one: stop it for real
        // first. A one-shot (SFX) never becomes active and never disturbs a
        // looping BGM playing alongside it. It is replaced rather than stopped,
        // so it is marked as such and its ending goes unreported.
        if repeat {
            self.flush_active(true);
        }

        self.stop(audio_handle);
        self.sink.set_clip_volume(audio_handle, self.effective(volume.load(Ordering::Relaxed)));

        let stop_flag = Arc::new(AtomicBool::new(false));
        let completed = Arc::new(AtomicBool::new(false));
        let superseded = Arc::new(AtomicBool::new(false));
        self.playing.insert(audio_handle, stop_flag.clone());

        // Offer the file to the sink's own renderer first. When it takes it, the
        // sink plays the pre-rendered stream and this task only watches for the
        // stop flag (looping) or the clip's length (one-shot).
        if let Some(duration_ms) = self.sink.play_smaf(audio_handle, &data, repeat) {
            if repeat {
                // Track the looping clip so an identical re-play coalesces and a
                // real stop is honored by the reaper after a short grace.
                self.active = Some(ActiveSmaf {
                    hash,
                    repeat,
                    handle: audio_handle,
                    sink_handle: audio_handle,
                    stop_flag: stop_flag.clone(),
                    completed: completed.clone(),
                    superseded: superseded.clone(),
                    pending_stop_polls: None,
                });
                self.ensure_reaper(system);

                let system_clone = system.clone();
                let stop_flag_clone = stop_flag.clone();
                system.spawn(async move || {
                    while !stop_flag_clone.load(Ordering::Relaxed) {
                        system_clone.sleep(50).await;
                    }
                    Ok(())
                });
            } else {
                let system_clone = system.clone();
                let sink_clone = self.sink.clone();
                let stop_flag_clone = stop_flag.clone();
                let completed_clone = completed.clone();
                system.spawn(async move || {
                    let mut elapsed = 0u64;
                    loop {
                        if stop_flag_clone.load(Ordering::Relaxed) {
                            break;
                        }
                        if elapsed >= u64::from(duration_ms) {
                            completed_clone.store(true, Ordering::Release);
                            break;
                        }
                        system_clone.sleep(50).await;
                        elapsed += 50;
                    }
                    sink_clone.stop_smaf(audio_handle);
                    Ok(())
                });
            }
            return Ok(Playback {
                completed,
                stopped: stop_flag,
                superseded,
            });
        }

        // Otherwise stream the sequence live as MIDI events.
        let player = SmafPlayer::new(&data);
        let clip = audio_handle;
        let mut system_clone = system.clone();
        let sink_clone = self.sink.clone();
        let stop_flag_clone = stop_flag.clone();
        let completed_clone = completed.clone();

        // TODO use dedicated audio player task
        system.spawn(async move || {
            player.play(clip, &mut system_clone, &**sink_clone, &stop_flag_clone, repeat).await;

            if !stop_flag_clone.load(Ordering::Relaxed) {
                completed_clone.store(true, Ordering::Release);
            }

            Ok(())
        });

        Ok(Playback {
            completed,
            stopped: stop_flag,
            superseded,
        })
    }

    pub fn is_playing(&self, audio_handle: AudioHandle) -> bool {
        self.playing.contains_key(&audio_handle)
    }

    /// Sets one clip's volume, which is the clip's own and nothing else's.
    ///
    /// This used to reach the sink as `set_master_volume`, so a title that
    /// turned one sound down turned everything down with it - and one that
    /// muted an effect before stopping it (졸라맨액션학원 pairs the two) silenced
    /// the music playing underneath. The reference has no such control: a
    /// volume belongs to a clip, and `syncKTFClipGain` sets that clip's gain
    /// alone.
    ///
    /// The level is kept whether or not the clip is sounding, because a title
    /// sets a volume before it plays and expects the sound to come out at it.
    pub fn set_volume(&mut self, audio_handle: AudioHandle, volume: u8) -> Result<(), AudioError> {
        let state = self.volumes.get(&audio_handle).ok_or(AudioError::InvalidHandle)?;
        let volume = volume.min(FULL_VOLUME);
        state.store(volume, Ordering::Relaxed);

        self.sink.set_clip_volume(self.sink_handle_of(audio_handle), self.effective(volume));

        Ok(())
    }

    /// A clip's level scaled by the handset's, which is what the sink plays at.
    fn effective(&self, clip_volume: u8) -> u8 {
        ((clip_volume as u16 * self.master as u16) / FULL_VOLUME as u16) as u8
    }

    /// Sets the handset's own media level, scaling every clip rather than
    /// replacing what any of them was set to.
    ///
    /// `MC_mdaClipSetVolume` must not come here - a clip's volume is the
    /// clip's alone, and routing it to a master is what once let a title that
    /// turned one sound down turn everything down with it. This is the other
    /// control, `MC_mdaSetVolume`, which genuinely is handset-wide: 드래곤로드's
    /// sound menu drives only this one, never a clip's, so with nothing behind
    /// it every step of its slider changed nothing at all.
    ///
    /// Clips already sounding are re-levelled, so a slider moved while the
    /// music plays is heard on the music rather than at the next track.
    pub fn set_master_volume(&mut self, volume: u8) {
        self.master = volume.min(FULL_VOLUME);

        let levels: Vec<(AudioHandle, u8)> = self
            .volumes
            .iter()
            .map(|(handle, state)| (*handle, state.load(Ordering::Relaxed)))
            .collect();
        for (handle, clip_volume) in levels {
            self.sink.set_clip_volume(self.sink_handle_of(handle), self.effective(clip_volume));
        }
    }

    /// The handset's own media level.
    pub fn master_volume(&self) -> u8 {
        self.master
    }

    /// The handle the sink knows a clip by.
    ///
    /// A looping clip re-played with identical data keeps the stream it first
    /// started, under the handle it started on, while the title goes on
    /// allocating fresh handles for it - so anything aimed at the sink has to
    /// be aimed there. See [`ActiveSmaf::sink_handle`].
    fn sink_handle_of(&self, audio_handle: AudioHandle) -> AudioHandle {
        match self.active.as_ref() {
            Some(active) if active.handle == audio_handle => active.sink_handle,
            _ => audio_handle,
        }
    }

    pub fn get_volume(&self, audio_handle: AudioHandle) -> Result<u8, AudioError> {
        self.volumes
            .get(&audio_handle)
            .map(|state| state.load(Ordering::Relaxed))
            .ok_or(AudioError::InvalidHandle)
    }

    pub fn stop(&mut self, audio_handle: AudioHandle) {
        // Defer stopping the active looping clip: the title tears it down and
        // rebuilds it every frame, so an immediate stop would restart the audio
        // from zero. The reaper stops it for real once no identical re-play has
        // arrived within the grace window.
        if let Some(active) = self.active.as_mut()
            && active.handle == audio_handle
        {
            if active.pending_stop_polls.is_none() {
                active.pending_stop_polls = Some(0);
            }
            self.playing.remove(&audio_handle);
            return;
        }

        if let Some(stop_flag) = self.playing.remove(&audio_handle) {
            stop_flag.store(true, Ordering::Relaxed);
        }
    }

    /// Stops the active looping clip's sink playback immediately and forgets it.
    ///
    /// `superseded` says whether this is the title replacing the track with
    /// another - in which case the playback ends without that being news, since
    /// the title is the one that replaced it - or a stop it asked for and may be
    /// waiting to hear about.
    fn flush_active(&mut self, superseded: bool) {
        if let Some(active) = self.active.take() {
            // Stop the stream the sink actually plays, not the latest handle the
            // title allocated - they differ once a looping clip has re-played.
            self.sink.stop_smaf(active.sink_handle);
            if superseded {
                active.superseded.store(true, Ordering::Relaxed);
            }
            active.stop_flag.store(true, Ordering::Relaxed);
            self.playing.remove(&active.handle);
        }
    }

    /// Spawns the one deferred-stop reaper for this `Audio`. It polls the active
    /// looping clip and, once a deferred stop has stood for the grace window
    /// with no identical re-play cancelling it, stops the sink for real.
    fn ensure_reaper(&mut self, system: &System) {
        if self.reaper_started {
            return;
        }
        self.reaper_started = true;

        let system_clone = system.clone();
        system.spawn(async move || {
            loop {
                system_clone.sleep(REAPER_POLL_MS).await;

                let mut audio = system_clone.audio();
                let flush = match audio.active.as_mut() {
                    Some(active) => match active.pending_stop_polls {
                        Some(polls) if polls + 1 >= PENDING_STOP_GRACE_POLLS => true,
                        Some(polls) => {
                            active.pending_stop_polls = Some(polls + 1);
                            false
                        }
                        None => false,
                    },
                    None => false,
                };
                if flush {
                    // The title asked for this stop and may be waiting to hear
                    // that it happened, so it is not a supersede.
                    audio.flush_active(false);
                }
            }
        });
    }

    pub fn close(&mut self, audio_handle: AudioHandle) -> Result<(), AudioError> {
        self.stop(audio_handle);

        if self.files.remove(&audio_handle).is_none() {
            return Err(AudioError::InvalidHandle);
        }
        self.volumes.remove(&audio_handle);
        // A closed clip has no volume of its own any more, and saying so is what
        // keeps the sink from remembering a level per handle for a title that
        // loads and closes thousands of them.
        self.sink.set_clip_volume(audio_handle, FULL_VOLUME);

        Ok(())
    }
}

pub struct SmafPlayer {
    events: Vec<(usize, SmafEvent)>,
}

impl SmafPlayer {
    pub fn new(data: &[u8]) -> Self {
        Self { events: parse_smaf(data) }
    }

    pub async fn play(&self, clip: AudioHandle, system: &mut System, sink: &dyn AudioSink, stop_flag: &AtomicBool, repeat: bool) {
        // An isolated voice for this clip, so its sequence does not collide with
        // other clips playing at the same time (a looping track under short
        // effects) on shared MIDI channels. It is opened in the clip's name so
        // the clip's volume reaches it.
        let voice = sink.open_midi_voice(clip);
        tracing::info!("[audio] SMAF clip on isolated voice {voice}");

        loop {
            let mut active_notes: Vec<(u8, u8)> = Vec::new();
            let mut used_channels: BTreeSet<u8> = BTreeSet::new();

            let start_time = system.platform().now();
            for (time, event) in &self.events {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                let now = system.platform().now();
                if (*time as u64) > now - start_time {
                    system.sleep(((*time as u64) - (now - start_time)) as _).await;

                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                }

                match event {
                    SmafEvent::Wave {
                        channel,
                        sampling_rate,
                        data,
                    } => {
                        sink.play_wave(clip, *channel, *sampling_rate, data);
                    }
                    SmafEvent::MidiNoteOn { channel, note, velocity } => {
                        sink.midi_note_on(voice, *channel, *note, *velocity);
                        active_notes.push((*channel, *note));
                        used_channels.insert(*channel);
                    }
                    SmafEvent::MidiNoteOff { channel, note, velocity } => {
                        sink.midi_note_off(voice, *channel, *note, *velocity);
                        active_notes.retain(|(c, n)| !(*c == *channel && *n == *note));
                    }
                    SmafEvent::MidiProgramChange { channel, program } => {
                        sink.midi_program_change(voice, *channel, *program);
                        used_channels.insert(*channel);
                    }
                    SmafEvent::MidiControlChange { channel, control, value } => {
                        sink.midi_control_change(voice, *channel, *control, *value);
                        used_channels.insert(*channel);
                    }
                    SmafEvent::MidiPitchBend { channel, value } => {
                        sink.midi_pitch_bend(voice, *channel, *value);
                        used_channels.insert(*channel);
                    }
                    SmafEvent::MidiSysEx(data) => {
                        sink.midi_sysex(voice, data);
                    }
                    SmafEvent::End => {}
                }
            }

            for (channel, note) in &active_notes {
                sink.midi_note_off(voice, *channel, *note, 0);
            }

            for channel in &used_channels {
                sink.midi_control_change(voice, *channel, 64, 0);
                sink.midi_control_change(voice, *channel, 120, 0);
                sink.midi_control_change(voice, *channel, 123, 0);
            }

            if !repeat || stop_flag.load(Ordering::Relaxed) {
                break;
            }
        }

        // The clip is done feeding events; let its voice ring out and be
        // reclaimed once silent.
        sink.close_midi_voice(voice);
    }
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, sync::Arc, vec};
    use alloc::{string::String, vec::Vec};
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use spin::Mutex;

    use smaf_player::SmafEvent;

    use super::SmafPlayer;
    use crate::{AudioSink, Database, DatabaseRepository, DefaultTaskRunner, Filesystem, Instant, Platform, Screen, System, canvas::Image};

    struct NullDatabase;

    #[async_trait::async_trait]
    impl Database for NullDatabase {
        async fn next_id(&self) -> u32 {
            1
        }

        async fn add(&mut self, _data: &[u8]) -> u32 {
            1
        }

        async fn get(&self, _id: u32) -> Option<alloc::vec::Vec<u8>> {
            None
        }

        async fn set(&mut self, _id: u32, _data: &[u8]) -> bool {
            true
        }

        async fn delete(&mut self, _id: u32) -> bool {
            true
        }

        async fn get_record_ids(&self) -> alloc::vec::Vec<u32> {
            vec![]
        }
    }

    struct NullDatabaseRepository;

    #[async_trait::async_trait]
    impl DatabaseRepository for NullDatabaseRepository {
        async fn open(&self, _name: &str, _app_id: &str) -> Box<dyn Database> {
            Box::new(NullDatabase)
        }

        async fn exists(&self, _name: &str, _app_id: &str) -> bool {
            false
        }

        async fn delete(&self, _name: &str, _app_id: &str) -> bool {
            false
        }

        async fn list(&self, _app_id: &str) -> Vec<String> {
            vec![]
        }
    }

    struct NullFilesystem;

    #[async_trait::async_trait]
    impl Filesystem for NullFilesystem {
        async fn exists(&self, _aid: &str, _path: &str) -> bool {
            false
        }

        async fn size(&self, _aid: &str, _path: &str) -> Option<usize> {
            None
        }

        async fn read(&self, _aid: &str, _path: &str, _offset: usize, _count: usize, _buf: &mut [u8]) -> Option<usize> {
            None
        }

        async fn write(&self, _aid: &str, _path: &str, _offset: usize, data: &[u8]) -> usize {
            data.len()
        }

        async fn truncate(&self, _aid: &str, _path: &str, _len: usize) {}

        async fn remove(&self, _aid: &str, _path: &str) -> bool {
            false
        }

        async fn mkdir(&self, _aid: &str, _path: &str) -> core::result::Result<(), crate::platform::FilesystemMkdirError> {
            Err(crate::platform::FilesystemMkdirError::NotFound)
        }

        async fn rmdir(&self, _aid: &str, _path: &str) -> core::result::Result<(), crate::platform::FilesystemRmDirError> {
            Err(crate::platform::FilesystemRmDirError::NotFound)
        }

        async fn rename(&self, _aid: &str, _from: &str, _to: &str) -> core::result::Result<(), crate::platform::FilesystemRenameError> {
            Err(crate::platform::FilesystemRenameError::NotFound)
        }

        async fn set_mode(&self, _aid: &str, _path: &str, _mode: u32) -> core::result::Result<(), crate::platform::FilesystemSetModeError> {
            Err(crate::platform::FilesystemSetModeError::NotFound)
        }

        async fn total_space(&self, _aid: &str) -> Option<u64> {
            None
        }

        async fn available_space(&self, _aid: &str) -> Option<u64> {
            None
        }

        async fn list(&self, _aid: &str, _path: &str) -> Option<Vec<String>> {
            None
        }
    }

    struct NullScreen;

    impl Screen for NullScreen {
        fn request_redraw(&self) -> wie_util::Result<()> {
            Ok(())
        }

        fn paint(&self, _image: &dyn Image) {}

        fn width(&self) -> u32 {
            240
        }

        fn height(&self) -> u32 {
            320
        }
    }

    struct NullPlatform {
        screen: NullScreen,
        database_repository: NullDatabaseRepository,
        filesystem: NullFilesystem,
        now: AtomicUsize,
        smaf_play: Arc<AtomicUsize>,
        smaf_stop: Arc<AtomicUsize>,
        smaf_last_played: Arc<AtomicUsize>,
        smaf_last_stopped: Arc<AtomicUsize>,
    }

    impl NullPlatform {
        fn new() -> Self {
            Self {
                screen: NullScreen,
                database_repository: NullDatabaseRepository,
                filesystem: NullFilesystem,
                now: AtomicUsize::new(0),
                smaf_play: Arc::new(AtomicUsize::new(0)),
                smaf_stop: Arc::new(AtomicUsize::new(0)),
                smaf_last_played: Arc::new(AtomicUsize::new(usize::MAX)),
                smaf_last_stopped: Arc::new(AtomicUsize::new(usize::MAX)),
            }
        }
    }

    impl Platform for NullPlatform {
        fn screen(&self) -> &dyn Screen {
            &self.screen
        }

        fn now(&self) -> Instant {
            Instant::from_epoch_millis(self.now.fetch_add(8, Ordering::SeqCst) as u64)
        }

        fn database_repository(&self) -> &dyn DatabaseRepository {
            &self.database_repository
        }

        fn filesystem(&self) -> &dyn Filesystem {
            &self.filesystem
        }

        fn audio_sink(&self) -> Box<dyn AudioSink> {
            Box::new(SmafCountingSink {
                play: self.smaf_play.clone(),
                stop: self.smaf_stop.clone(),
                last_played: self.smaf_last_played.clone(),
                last_stopped: self.smaf_last_stopped.clone(),
            })
        }

        fn write_stdout(&self, _buf: &[u8]) {}

        fn write_stderr(&self, _buf: &[u8]) {}

        fn exit(&self) {}

        fn vibrate(&self, _duration_ms: u64, _intensity: u8) {}

        fn set_backlight_mode(&self, _mode: u8) {}
    }

    struct NoopAudioSink;

    impl AudioSink for NoopAudioSink {
        fn play_wave(&self, _clip: u32, _channel: u8, _sampling_rate: u32, _wave_data: &[i16]) {}

        fn midi_note_on(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}

        fn midi_note_off(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}

        fn midi_program_change(&self, _voice: u32, _channel_id: u8, _program: u8) {}

        fn midi_control_change(&self, _voice: u32, _channel_id: u8, _control: u8, _value: u8) {}

        fn midi_pitch_bend(&self, _voice: u32, _channel_id: u8, _value: u16) {}

        fn midi_sysex(&self, _voice: u32, _data: &[u8]) {}
    }

    /// A sink with a pre-rendered SMAF path that counts how many times a clip is
    /// started and stopped, to observe the coalescing of identical re-plays.
    struct SmafCountingSink {
        play: Arc<AtomicUsize>,
        stop: Arc<AtomicUsize>,
        /// The handle passed to the most recent `play_smaf` / `stop_smaf`, so a
        /// test can assert a stop targets the stream the sink actually played.
        last_played: Arc<AtomicUsize>,
        last_stopped: Arc<AtomicUsize>,
    }

    impl AudioSink for SmafCountingSink {
        fn play_wave(&self, _clip: u32, _channel: u8, _sampling_rate: u32, _wave_data: &[i16]) {}
        fn midi_note_on(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}
        fn midi_note_off(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}
        fn midi_program_change(&self, _voice: u32, _channel_id: u8, _program: u8) {}
        fn midi_control_change(&self, _voice: u32, _channel_id: u8, _control: u8, _value: u8) {}
        fn midi_pitch_bend(&self, _voice: u32, _channel_id: u8, _value: u16) {}
        fn midi_sysex(&self, _voice: u32, _data: &[u8]) {}

        fn play_smaf(&self, id: u32, _data: &[u8], _repeat: bool) -> Option<u32> {
            self.play.fetch_add(1, Ordering::SeqCst);
            self.last_played.store(id as usize, Ordering::SeqCst);
            Some(16_000)
        }

        fn stop_smaf(&self, id: u32) {
            self.stop.fetch_add(1, Ordering::SeqCst);
            self.last_stopped.store(id as usize, Ordering::SeqCst);
        }
    }

    /// Records every clip volume the sink is handed, so a test can see which
    /// clip a level was aimed at rather than only that one was set. The log is
    /// shared, so a test keeps hold of it after handing the `Audio` its sink.
    #[derive(Clone, Default)]
    struct VolumeRecordingSink {
        volumes: Arc<Mutex<Vec<(u32, u8)>>>,
    }

    impl AudioSink for VolumeRecordingSink {
        fn set_clip_volume(&self, clip: u32, volume: u8) {
            self.volumes.lock().push((clip, volume));
        }

        fn play_wave(&self, _clip: u32, _channel: u8, _sampling_rate: u32, _wave_data: &[i16]) {}
        fn midi_note_on(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}
        fn midi_note_off(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}
        fn midi_program_change(&self, _voice: u32, _channel_id: u8, _program: u8) {}
        fn midi_control_change(&self, _voice: u32, _channel_id: u8, _control: u8, _value: u8) {}
        fn midi_pitch_bend(&self, _voice: u32, _channel_id: u8, _value: u16) {}
        fn midi_sysex(&self, _voice: u32, _data: &[u8]) {}
    }

    struct CountingSink {
        program_change_count: Arc<AtomicUsize>,
        stop_after: usize,
        stop_flag: Arc<AtomicBool>,
    }

    impl AudioSink for CountingSink {
        fn play_wave(&self, _clip: u32, _channel: u8, _sampling_rate: u32, _wave_data: &[i16]) {}

        fn midi_note_on(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}

        fn midi_note_off(&self, _voice: u32, _channel_id: u8, _note: u8, _velocity: u8) {}

        fn midi_program_change(&self, _voice: u32, _channel_id: u8, _program: u8) {
            let count = self.program_change_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count >= self.stop_after {
                self.stop_flag.store(true, Ordering::SeqCst);
            }
        }

        fn midi_control_change(&self, _voice: u32, _channel_id: u8, _control: u8, _value: u8) {}

        fn midi_pitch_bend(&self, _voice: u32, _channel_id: u8, _value: u16) {}

        fn midi_sysex(&self, _voice: u32, _data: &[u8]) {}
    }

    fn new_system() -> System {
        System::new(Box::new(NullPlatform::new()), "test-pid", "test-aid", DefaultTaskRunner)
    }

    /// A system whose sink pre-renders SMAF, plus the (play, stop) counters that
    /// sink increments.
    struct SmafCounters {
        play: Arc<AtomicUsize>,
        stop: Arc<AtomicUsize>,
        last_played: Arc<AtomicUsize>,
        last_stopped: Arc<AtomicUsize>,
    }

    fn new_system_with_smaf_counters() -> (System, SmafCounters) {
        let platform = NullPlatform::new();
        let counters = SmafCounters {
            play: platform.smaf_play.clone(),
            stop: platform.smaf_stop.clone(),
            last_played: platform.smaf_last_played.clone(),
            last_stopped: platform.smaf_last_stopped.clone(),
        };
        let system = System::new(Box::new(platform), "test-pid", "test-aid", DefaultTaskRunner);
        (system, counters)
    }

    #[test]
    fn identical_looping_replay_does_not_restart_the_sink() {
        let (system, counters) = new_system_with_smaf_counters();

        // First BGM start: the sink begins playing the looping clip under h1.
        let h1 = system.audio().load_smaf(b"BGM-DATA").unwrap();
        system.audio().play_with_completion(&system, h1, true).unwrap();
        assert_eq!(counters.play.load(Ordering::SeqCst), 1);
        assert_eq!(counters.last_played.load(Ordering::SeqCst), h1 as usize);

        // The title tears the player down and rebuilds it with byte-identical
        // data (stop, close, load, play) - exactly 시드's per-frame pattern. The
        // sink must NOT be restarted or stopped: the playback continues, and the
        // stream stays under h1 even as the title allocates fresh handles.
        let mut latest = h1;
        for _ in 0..5 {
            system.audio().stop(latest);
            let _ = system.audio().close(latest);
            latest = system.audio().load_smaf(b"BGM-DATA").unwrap();
            system.audio().play_with_completion(&system, latest, true).unwrap();
        }
        assert_eq!(counters.play.load(Ordering::SeqCst), 1, "identical re-plays should coalesce");
        assert_eq!(counters.stop.load(Ordering::SeqCst), 0, "no stop while re-playing identical data");
        assert_ne!(latest, h1, "the title allocated fresh handles");

        // A genuinely different looping clip must stop the ORIGINAL stream (h1,
        // what the sink actually plays) - not the latest handle the title used,
        // which the sink never played - then start the new one.
        let other = system.audio().load_smaf(b"OTHER-BGM").unwrap();
        system.audio().play_with_completion(&system, other, true).unwrap();
        assert_eq!(counters.play.load(Ordering::SeqCst), 2);
        assert_eq!(counters.stop.load(Ordering::SeqCst), 1);
        assert_eq!(
            counters.last_stopped.load(Ordering::SeqCst),
            h1 as usize,
            "the stop must target the stream the sink played (h1), not a later handle"
        );
    }

    #[futures_test::test]
    async fn plays_once_when_repeat_is_false() {
        let counter = Arc::new(AtomicUsize::new(0));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let sink = CountingSink {
            program_change_count: counter.clone(),
            stop_after: usize::MAX,
            stop_flag: stop_flag.clone(),
        };
        let player = SmafPlayer {
            events: vec![(0, SmafEvent::MidiProgramChange { channel: 0, program: 1 })],
        };
        let mut system = new_system();

        player.play(1, &mut system, &sink, &stop_flag, false).await;

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[futures_test::test]
    async fn repeats_until_stop_flag_is_set() {
        let counter = Arc::new(AtomicUsize::new(0));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let sink = CountingSink {
            program_change_count: counter.clone(),
            stop_after: 2,
            stop_flag: stop_flag.clone(),
        };
        let player = SmafPlayer {
            events: vec![(0, SmafEvent::MidiProgramChange { channel: 0, program: 1 })],
        };
        let mut system = new_system();

        player.play(1, &mut system, &sink, &stop_flag, true).await;

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    /// A volume is one clip's, and reaches the sink named as that clip's.
    ///
    /// It used to reach the sink as a master volume - one level for everything
    /// it was playing - so a title that muted an effect muted the music under
    /// it. It was also dropped entirely unless the clip happened to be sounding
    /// at the time, which lost a volume set before a play.

    /// The handset's own level scales a clip's rather than replacing it, so a
    /// title that sets both is heard at the product of the two - and one that
    /// sets only the handset's is heard at all.
    ///
    /// 드래곤로드's sound menu drives only `MC_mdaSetVolume`; with nothing behind
    /// that control every step of its slider changed nothing.
    #[test]
    fn the_handsets_level_scales_a_clips_own() {
        let sink = VolumeRecordingSink::default();
        let mut audio = super::Audio::new(Box::new(sink.clone()));

        let music = audio.load_smaf(b"music").unwrap();
        let effect = audio.load_smaf(b"effect").unwrap();

        // Nothing has set the handset's level, so a clip is heard at its own.
        assert_eq!(audio.master_volume(), super::FULL_VOLUME);
        audio.set_volume(music, 80).unwrap();
        assert_eq!(sink.volumes.lock().last(), Some(&(music, 80)));

        // Halving the handset's halves what every clip already sounding is
        // heard at, and leaves what each was set to alone.
        sink.volumes.lock().clear();
        audio.set_master_volume(50);
        assert_eq!(audio.master_volume(), 50);
        assert_eq!(
            sink.volumes.lock().as_slice(),
            &[(music, 40), (effect, 50)],
            "every live clip is re-levelled, each by its own volume"
        );
        assert_eq!(audio.get_volume(music).unwrap(), 80, "the clip's own level is untouched");

        // A level set afterwards is scaled the same way.
        sink.volumes.lock().clear();
        audio.set_volume(effect, 60).unwrap();
        assert_eq!(sink.volumes.lock().as_slice(), &[(effect, 30)]);
        assert_eq!(audio.get_volume(effect).unwrap(), 60);

        // A title that only ever drives the handset's control still gets the
        // whole range out of it, because a clip it never set is at full scale.
        sink.volumes.lock().clear();
        let bare = audio.load_smaf(b"bare").unwrap();
        audio.set_master_volume(20);
        assert!(sink.volumes.lock().contains(&(bare, 20)));

        // Over 100 is held at 100, as a clip's own level is.
        audio.set_master_volume(250);
        assert_eq!(audio.master_volume(), 100);
    }

    #[test]
    fn a_volume_belongs_to_one_clip() {
        let sink = VolumeRecordingSink::default();
        let mut audio = super::Audio::new(Box::new(sink.clone()));

        let music = audio.load_smaf(b"music").unwrap();
        let effect = audio.load_smaf(b"effect").unwrap();

        // Set before either has played: the level has to stick, because that is
        // when titles set it.
        audio.set_volume(effect, 0).unwrap();
        audio.set_volume(music, 80).unwrap();

        assert_eq!(audio.get_volume(effect).unwrap(), 0);
        assert_eq!(audio.get_volume(music).unwrap(), 80, "one clip's volume is not the other's");

        assert_eq!(
            sink.volumes.lock().as_slice(),
            &[(effect, 0), (music, 80)],
            "each level reaches the sink named as the clip it belongs to"
        );

        // Over 100 is held at 100, as the reference clamps the record it syncs.
        audio.set_volume(music, 250).unwrap();
        assert_eq!(audio.get_volume(music).unwrap(), 100);

        // Closing a clip takes its level back to full scale, so the sink has no
        // reason to remember it.
        audio.close(effect).unwrap();
        assert_eq!(sink.volumes.lock().last(), Some(&(effect, 100)));
    }

    /// A looping clip the title replaces with another is superseded, not
    /// stopped; one the title stops is stopped, not superseded.
    ///
    /// The two look the same from the audio layer - a playback that is over -
    /// but they are not the same news. A title that stopped a clip may be
    /// waiting to hear that it ended; a title that replaced one already knows,
    /// because it asked. 놈ZERO is the second: told the track it had just
    /// replaced had ended, it tore the clip down and built it again, which
    /// replaced the track once more, and its music came out in fragments.
    #[test]
    fn a_replaced_playback_is_superseded_and_a_stopped_one_is_not() {
        let (system, _counters) = new_system_with_smaf_counters();

        let music = system.audio().load_smaf(b"BGM-ONE").unwrap();
        let first = system.audio().play_with_completion(&system, music, true).unwrap();
        let (stopped, superseded) = (first.stopped, first.superseded);
        assert!(!stopped.load(Ordering::SeqCst));
        assert!(!superseded.load(Ordering::SeqCst));

        // A different looping track takes its place. The playback is over, but
        // the title is the one that ended it.
        let other = system.audio().load_smaf(b"BGM-TWO").unwrap();
        let second = system.audio().play_with_completion(&system, other, true).unwrap();
        let (other_stopped, other_superseded) = (second.stopped, second.superseded);

        assert!(stopped.load(Ordering::SeqCst), "the replaced playback is over");
        assert!(superseded.load(Ordering::SeqCst), "and it is over because it was replaced");

        // Stopping the one that is playing now is the other case: over, and
        // over because the title said so.
        system.audio().stop(other);
        system.audio().flush_active(false);

        assert!(other_stopped.load(Ordering::SeqCst));
        assert!(!other_superseded.load(Ordering::SeqCst), "a stop the title asked for is not a supersede");
    }

    #[test]
    fn default_clip_binds_the_last_clip_zero_load() {
        let mut audio = super::Audio::new(Box::new(NoopAudioSink));
        // Nothing bound until a clip-0 load happens.
        assert_eq!(audio.default_clip(), None);

        // A clip-0 title loads its data under a real handle and binds it as the
        // default player; the most recent load wins.
        let first = audio.load_smaf(b"first").unwrap();
        audio.set_default_clip(first);
        assert_eq!(audio.default_clip(), Some(first));

        let second = audio.load_smaf(b"second").unwrap();
        audio.set_default_clip(second);
        assert_eq!(audio.default_clip(), Some(second));
    }
}
