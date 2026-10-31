mod audio;
mod audio_formats;
mod event_queue;
mod file_system;
mod input_method;

use alloc::{borrow::ToOwned, boxed::Box, string::String, sync::Arc};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use spin::{RwLock, RwLockWriteGuard};

use wie_util::Result;

use crate::{
    AsyncCallable,
    executor::Executor,
    local_network::LocalNetwork,
    platform::Platform,
    task::{SleepFuture, YieldFuture},
    task_runner::TaskRunner,
};

pub(crate) use self::event_queue::note_frame_presented;
use self::{audio::Audio, event_queue::EventQueue, input_method::InputMethod};

pub use self::{
    event_queue::{Event, KeyCode, PointerKind, set_touch_enabled, touch_enabled},
    file_system::FilesystemOverlay,
    input_method::InputMethodOutput,
};

#[derive(Clone)]
pub struct System {
    pid: String,
    aid: String,
    executor: Executor,
    platform: Arc<Box<dyn Platform>>,
    filesystem: FilesystemOverlay,
    event_queue: Arc<RwLock<EventQueue>>,
    audio: Arc<RwLock<Audio>>,
    input_method: Arc<RwLock<InputMethod>>,
    task_runner: Arc<dyn TaskRunner>,
    /// The servers this run answers for itself, in place of ones that have been
    /// switched off for years. Empty unless the host registered one.
    local_network: Arc<RwLock<LocalNetwork>>,
    /// Set once the title has been seen drawing into the LCD frame buffer
    /// itself. See [`System::title_drives_lcd`].
    title_drives_lcd: Arc<AtomicBool>,
    /// Whether this title draws its picture sideways into an upright panel.
    /// See [`System::title_draws_sideways`].
    title_draws_sideways: Arc<AtomicBool>,
    /// Whether this title lays its screens out below a status strip.
    /// See [`System::title_expects_annunciator`].
    title_expects_annunciator: Arc<AtomicBool>,
    /// How many rows that strip takes, or zero for the size table's own answer.
    /// See [`System::title_annunciator_rows`].
    title_annunciator_rows: Arc<AtomicU32>,
    /// Whether this title's clips include their far edge.
    /// See [`System::title_clip_includes_far_edge`].
    title_clip_includes_far_edge: Arc<AtomicBool>,
    /// Whether the screen buffer is wiped before each paint for this title.
    /// See [`System::title_clears_screen_each_paint`].
    title_clears_screen_each_paint: Arc<AtomicBool>,
    /// How many rows below a `Displayable` the platform keeps for itself.
    /// See [`System::displayable_reserved_rows`].
    displayable_reserved_rows: Arc<AtomicU32>,
    /// Whether the MIDP key path delivers the de-facto standard negative nav
    /// codes. See [`System::midp_uses_standard_key_codes`].
    midp_uses_standard_key_codes: Arc<AtomicBool>,
    /// Whether this title reads its keys as the SK-VM handset's positive
    /// scancodes. See [`System::title_keys_as_skvm_scancodes`].
    title_keys_as_skvm_scancodes: Arc<AtomicBool>,
    /// Set once the title has drawn through the SK-VM graphics path
    /// (`com.skt.m.Graphics2D`), which keeps its own translate and clip on the
    /// screen graphics between frames. See [`System::title_owns_graphics_state`].
    title_owns_graphics_state: Arc<AtomicBool>,
    /// Whether the scene is painted whole each pass rather than only the region
    /// the title asked for. See [`System::title_repaints_whole_frame`].
    title_repaints_whole_frame: Arc<AtomicBool>,
    /// Whether a fresh mutable `Image` starts transparent rather than opaque
    /// white for this title. See [`System::title_blank_mutable_image_transparent`].
    title_blank_mutable_image_transparent: Arc<AtomicBool>,
    /// How many rows are cropped from the bottom of the frame before it reaches
    /// the screen. See [`System::title_present_crop_bottom`].
    title_present_crop_bottom: Arc<AtomicU32>,
    /// The picture size enlarged to the panel as it is decoded, as
    /// `width << 16 | height`, `0` for none.
    /// See [`System::title_stretched_picture`].
    title_stretched_picture: Arc<AtomicU32>,
    /// Whether opening a file that is not there to read fails.
    /// See [`System::missing_file_fails_read_open`].
    missing_file_fails_read_open: Arc<AtomicBool>,
}

impl System {
    pub fn new<T>(platform: Box<dyn Platform>, pid: &str, aid: &str, task_runner: T) -> Self
    where
        T: TaskRunner + 'static,
    {
        let audio_sink = platform.audio_sink();
        let platform = Arc::new(platform);
        let filesystem = FilesystemOverlay::new(platform.clone(), aid);

        let mut local_network = LocalNetwork::new();
        for endpoint in platform.local_endpoints() {
            local_network.register(endpoint);
        }

        // The servers this emulator answers for itself, behind whatever the host
        // offers: a run that sets one of the diagnostic endpoints is asking to
        // see the exchange rather than to have it answered.
        local_network.register(Box::new(crate::local_network::GpangEndpoint::new()));

        // The server 아무이유없어2 checks for a save on and buys through, gone
        // for years. Host-gated.
        local_network.register(Box::new(crate::local_network::NoReasonEndpoint::new()));

        // 서든어택포켓2's KTF build buys through `121.78.119.73:45351` over
        // `FastRelay`, gone for years; the connect failed and the title reported
        // 수신상태불량. Host-gated.
        local_network.register(Box::new(crate::local_network::SuddenAttack2Endpoint));

        // Nexon Mobile's GXG gateway, `ngw.gxg.com:7300`, which the SKT builds of
        // its titles buy through. Answered by name as well as by address, the
        // name no longer resolving.
        local_network.register(Box::new(crate::local_network::GxgEndpoint::new()));
        local_network.register(Box::new(crate::local_network::NexonRelayEndpoint::new()));

        // 엑스피드스노보드's ranking/map server, gone for years. Answering it in
        // process lets the title past the `네트워크 접속 에러` its title screen
        // shows the moment it dials out. Host-gated, so no other title is
        // touched.
        local_network.register(Box::new(crate::local_network::SnowBoardEndpoint));

        // The carrier relay middleware `com.vdigm.billcom.relay` (`01039AD6`) a
        // KTF title dials through to create its save slot. The gateway at
        // `wipiwicgsfg.magicn.com:17096` has been gone for years; answering it
        // in process lets 오즈-천공의 기사단's 새로하기 reach character-name
        // entry rather than stopping on `서버와의 접속이 끊어졌습니다`.
        // Host-gated, so no other title is touched.
        // 월드접속's 코단 world is answered behind it too, keeping what it
        // remembers among the title's own files.
        local_network.register(Box::new(crate::local_network::RelayEndpoint::new(Some(filesystem.clone()))));

        // 렙업만이살길1's shop server (211.113.45.131:9002), gone for years. A
        // purchase of 돼지 dials it and sits on `구매중 입니다.` waiting for a
        // connect that never completes. Answered in process, the purchase and
        // the gift go through. Host-gated, so no other title is touched.
        local_network.register(Box::new(crate::local_network::LevelUpEndpoint));

        // 파워풀프로야구2009's trial server (121.254.169.55:15022), gone for
        // years, which `정식판 전환` dials. See
        // `crate::local_network::pawapro`.
        local_network.register(Box::new(crate::local_network::PawaproEndpoint));

        // 테라-영원의혼돈's KTF build fetches its 인증서 from this server, gone
        // for years. See `crate::local_network::tera`.
        local_network.register(Box::new(crate::local_network::TeraEndpoint));

        // SD세계대전's patch server, gone for years. See
        // `crate::local_network::sdw`.
        local_network.register(Box::new(crate::local_network::SdwEndpoint));

        // The game server EA프로야구2010's SKT builds ask for the subscriber's
        // saves, gone for years. See `crate::local_network::dsp`.
        local_network.register(Box::new(crate::local_network::DspEndpoint));

        // 질주쾌감 스케쳐2's shop/billing server (222.231.31.45:28013), gone for
        // years. The title opens a plain socket to it and writes the carrier
        // billing frame `ff ff 12 00 68 00 <subscriber> 03` - the `0x68`
        // purchase - by hand; dialed at the dead host the read never answers and
        // the shop shows `구매 실패`. Answered in process with the family's
        // `granted` frame, the purchase goes through. Host-gated, so no other
        // title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new(
            "billing(222.231.31.45:28013)",
            "222.231.31.45",
            28013,
        )));

        // The 컴투스 GP4 login server (211.115.66.250:15133), gone for years.
        // 미니게임천국4 and the other GP4 titles open a plain socket to it on
        // startup and speak the big-endian length-prefixed login handshake
        // (`[u16be length][u16 type][0x30 ...]`); dialed at the dead host the
        // connect never completes and the title sits on its loading screen.
        // Answered in process with the family's `granted` reply
        // (`crate::billing::lgt_local_apf2_response`), the login goes through.
        // Host-gated, so no other title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_length_prefixed(
            "billing(211.115.66.250:15133)",
            "211.115.66.250",
            15133,
        )));
        // The same server's other address, which the 2010 GP titles -
        // 미니게임천국5 and 액션퍼즐패밀리4 - dial for their shops.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_length_prefixed(
            "billing(211.115.66.252:15133)",
            "211.115.66.252",
            15133,
        )));

        // 슈퍼액션히어로2 (KTF) dials another of 컴투스's servers for its 별구매,
        // over a plain socket, and speaks the same session there.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_length_prefixed(
            "billing(211.115.66.244:15193)",
            "211.115.66.244",
            15193,
        )));

        // GAMEVIL's shop servers for its 2010-11 KTF builds, gone for years:
        // 2011프로야구 dials 23205, 2011슈퍼사커 and 놈5 29505, and a purchase
        // sat on a connect that never completed. The answer to their opening
        // question sends them on to 31105, the shop proper, answered here too.
        // Their records are the
        // `tagNetHeader` ones 제노니아 writes - a `u16le` length and a `u16le`
        // command - read the way their own reader reads them
        // (`crate::billing::ktf_local_gamevil_2011_response`). Host-gated, so
        // no other title is touched. 놈4 (KTF 01042258) authenticates at 32015
        // with the same records - 0x010a, answered 0x010b - before it will
        // start at all.
        for (name, port) in [
            ("gamevil(218.145.70.36:23205)", 23205),
            ("gamevil(218.145.70.36:29505)", 29505),
            ("gamevil(218.145.70.36:31105)", crate::billing::GAMEVIL_SHOP_PORT),
            ("gamevil(218.145.70.36:32015)", 32015),
        ] {
            local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_gamevil(
                name,
                "218.145.70.36",
                port,
            )));
        }
        // The address beside it, which 놈4 dialled on a handset - its server
        // list names both, and which one a run picks is not this side's to
        // settle - at its own port.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_gamevil(
            "gamevil(218.145.70.37:32175)",
            "218.145.70.37",
            32175,
        )));

        // 영웅서기5's shop and 창고 server for its KTF build, which it reaches
        // through `FastRelay` at the port after its LGT build's, gone for
        // years. The frames are the LGT build's, service code and all
        // (`crate::billing::lgt_local_hero5_response`). Host-gated, so no
        // other title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_u32_length_prefixed(
            "billing(210.222.18.28:18183)",
            "210.222.18.28",
            18183,
        )));

        // 크로이센's shop server for its KTF build (222.231.57.145:56000), gone
        // for years. A purchase dials it and waits on a connect that never
        // completes. Answered in process with the record its LGT build is
        // granted through the billing gateway
        // (`crate::billing::lgt_local_chroisen_response`), the item is given.
        // Host-gated, so no other title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_kp_tagged(
            "billing(222.231.57.145:56000)",
            "222.231.57.145",
            56000,
        )));

        // 미니게임히어로즈2's KTF build logs in where its LGT build does, over a
        // plain socket named "BILLSOCK" rather than a billing socket, and sat
        // on `인증 중` while the connect to the dead server never completed.
        // Both stages are answered in process
        // (`crate::billing::ktf_local_miniheroes2_response`) and it says
        // `인증에 성공했습니다`. Host-gated, so no other title is touched.
        for (name, host, port) in [
            ("miniheroes2(211.239.165.14:7205)", "211.239.165.14", 7205),
            ("miniheroes2(211.239.165.13:8036)", "211.239.165.13", 8036),
            // 미니파이터 검사편's game server, dialled after its login - see
            // `crate::billing::ktf_local_minifighter_server_response`.
            ("minifighter(211.239.165.13:8034)", "211.239.165.13", 8034),
        ] {
            local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_miniheroes(name, host, port)));
        }

        // 윈즈테일's billing server, gone for years. A 하트 purchase opens a
        // plain socket it names "TEST_BILLSOCK", writes the `ENSKTF` record and
        // sat on `연결중...` waiting for the answer. Answered in process
        // (`crate::billing::lgt_local_ens_record_response`), the 하트 are given.
        // Host-gated, so no other title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_ens(
            "billing(218.50.3.88:5018)",
            "218.50.3.88",
            5018,
        )));

        // 디펜스히어로's 인증 server, gone for years. 사용자 인증 opens a plain
        // socket to it, writes a "WM" frame and sat on its connecting screen.
        // Answered in process (`crate::billing::ktf_local_wm_response`), the
        // title is authenticated. Host-gated, so no other title is touched.
        local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_wm(
            "billing(211.43.222.180:21000)",
            "211.43.222.180",
            21000,
        )));

        // SBS포트리스4's KTF build logs in at this server, gone for years, and
        // sat on its connecting box. See `crate::local_network::fortress`.
        local_network.register(Box::new(crate::local_network::FortressEndpoint));

        // 템페스트's 정품인증 servers, gone for years, which it reaches through the
        // `FastRelay` carrier library rather than a socket of its own: the
        // relay's address and the two ports its other modes dial. Answered in
        // process (`crate::billing::ktf_local_tempest_response`), the 인증서 is
        // granted. 슈퍼액션 히어로5's 아이템샵 is the same relay and frames. Host-gated,
        // so no other title is touched.
        for (name, host, port) in [
            ("billing(211.115.66.252:15136)", "211.115.66.252", 15136),
            ("billing(211.115.66.252:15155)", "211.115.66.252", 15155),
            ("billing(211.115.66.252:18005)", "211.115.66.252", 18005),
            ("billing(211.233.42.196:15136)", "211.233.42.196", 15136),
            ("billing(211.233.42.196:15155)", "211.233.42.196", 15155),
            ("billing(211.233.42.196:18005)", "211.233.42.196", 18005),
        ] {
            local_network.register(Box::new(crate::local_network::BillingGatewayEndpoint::new_relay(name, host, port)));
        }

        Self {
            pid: pid.to_owned(),
            aid: aid.to_owned(), // TODO create metadata dictionary or something
            executor: Executor::new(),
            filesystem,
            platform,
            event_queue: Arc::new(RwLock::new(EventQueue::new())),
            audio: Arc::new(RwLock::new(Audio::new(audio_sink))),
            input_method: Arc::new(RwLock::new(InputMethod::new())),
            task_runner: Arc::new(task_runner),
            local_network: Arc::new(RwLock::new(local_network)),
            title_drives_lcd: Arc::new(AtomicBool::new(false)),
            title_draws_sideways: Arc::new(AtomicBool::new(false)),
            title_expects_annunciator: Arc::new(AtomicBool::new(false)),
            title_annunciator_rows: Arc::new(AtomicU32::new(0)),
            title_clip_includes_far_edge: Arc::new(AtomicBool::new(false)),
            title_clears_screen_each_paint: Arc::new(AtomicBool::new(false)),
            displayable_reserved_rows: Arc::new(AtomicU32::new(0)),
            midp_uses_standard_key_codes: Arc::new(AtomicBool::new(false)),
            title_keys_as_skvm_scancodes: Arc::new(AtomicBool::new(false)),
            title_owns_graphics_state: Arc::new(AtomicBool::new(false)),
            title_repaints_whole_frame: Arc::new(AtomicBool::new(false)),
            title_blank_mutable_image_transparent: Arc::new(AtomicBool::new(false)),
            title_present_crop_bottom: Arc::new(AtomicU32::new(0)),
            title_stretched_picture: Arc::new(AtomicU32::new(0)),
            missing_file_fails_read_open: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        let platform = self.platform.clone();
        self.executor.tick(move || platform.now())
    }

    /// Whether the emulator has nothing runnable until a timer fires (every
    /// task asleep with its wake-up in the future). The host loop uses this to
    /// stop early and sleep the leftover budget rather than busy-waiting.
    pub fn is_idle(&self) -> bool {
        self.executor.is_idle()
    }

    /// How long the host may sleep before the emulator has work again, in
    /// milliseconds. See [`Executor::idle_for`](crate::Executor::idle_for).
    pub fn idle_for(&self) -> Option<u64> {
        self.executor.idle_for(self.platform.now())
    }

    pub fn spawn<C>(&self, callable: C)
    where
        C: AsyncCallable<Result<()>> + 'static + Send,
    {
        let runner_clone = self.task_runner.clone();
        self.executor.spawn(async move || runner_clone.run(Box::pin(callable.call())).await);
    }

    pub fn sleep(&self, timeout: u64) -> SleepFuture {
        SleepFuture::new(timeout, &self.executor)
    }

    pub fn current_task_id(&self) -> u64 {
        self.executor.current_task_id()
    }

    pub fn yield_now(&self) -> YieldFuture {
        YieldFuture::waiting(&self.executor)
    }

    /// Unified filesystem view. Reads consult the persistent platform
    /// backend first and fall back to the in-memory virtual layer loaded
    /// from archives; writes always hit the platform backend.
    pub fn filesystem(&self) -> &FilesystemOverlay {
        &self.filesystem
    }

    pub fn pid(&self) -> &str {
        &self.pid
    }

    pub fn aid(&self) -> &str {
        &self.aid
    }

    pub fn local_network(&self) -> RwLockWriteGuard<'_, LocalNetwork> {
        self.local_network.write()
    }

    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref().as_ref()
    }

    pub fn audio(&self) -> RwLockWriteGuard<'_, Audio> {
        self.audio.as_ref().write()
    }

    pub fn event_queue(&self) -> RwLockWriteGuard<'_, EventQueue> {
        self.event_queue.write()
    }

    /// Whether the title paints the LCD frame buffer itself.
    ///
    /// A title whose drawing is a C engine writes that buffer directly and never
    /// flushes it, because on the handset the buffer is the display. The
    /// emulator's tick notices the first such frame and says so here, and the
    /// MIDP layer then stops flushing its own screen image over the top - which
    /// is the same reason `Display.disablePaint` exists for the clet wrapper,
    /// reached for a title that never goes through that wrapper.
    ///
    /// Stays false for every title that draws through the Java layer, so their
    /// frames keep reaching the screen the way they always have.
    pub fn title_drives_lcd(&self) -> bool {
        self.title_drives_lcd.load(Ordering::SeqCst)
    }

    pub fn set_title_drives_lcd(&self) {
        self.title_drives_lcd.store(true, Ordering::SeqCst);
    }

    /// Whether the title composes a landscape picture and copies it onto its
    /// upright panel a quarter turn clockwise, because it was written to be
    /// played with the handset held sideways.
    ///
    /// A fact about one title rather than anything the API reports, so it is
    /// looked up in `crate::quirks` and set here by the emulator that loaded
    /// the archive. [`crate::present`] is what reads it.
    pub fn title_draws_sideways(&self) -> bool {
        self.title_draws_sideways.load(Ordering::SeqCst)
    }

    pub fn set_title_draws_sideways(&self, sideways: bool) {
        self.title_draws_sideways.store(sideways, Ordering::SeqCst);
    }

    /// Whether the title was written for a handset whose `setClip` took in the
    /// pixel at the far edge of the rectangle as well. Looked up in
    /// `crate::quirks` and set here by the emulator that loaded the archive;
    /// the MIDP `Graphics` is what reads it.
    pub fn title_clip_includes_far_edge(&self) -> bool {
        self.title_clip_includes_far_edge.load(Ordering::SeqCst)
    }

    pub fn set_title_clip_includes_far_edge(&self, includes: bool) {
        self.title_clip_includes_far_edge.store(includes, Ordering::SeqCst);
    }

    /// Whether the runtime should wipe the screen buffer to black before every
    /// paint for this title, because it composes each frame over a blank
    /// surface and leaves the rows it does not draw to whatever was there.
    /// Looked up in `crate::quirks` and set here by the emulator that loaded
    /// the archive; the MIDP `Display` is what reads it.
    pub fn title_clears_screen_each_paint(&self) -> bool {
        self.title_clears_screen_each_paint.load(Ordering::SeqCst)
    }

    pub fn set_title_clears_screen_each_paint(&self, clears: bool) {
        self.title_clears_screen_each_paint.store(clears, Ordering::SeqCst);
    }

    /// How many rows fewer than the display a `Displayable` reports as its
    /// height, for the platform's own bar below it. Zero everywhere but on
    /// SK-VM, whose Canvas is sixteen rows shorter than the display; the
    /// emulator that loaded the archive sets it.
    pub fn displayable_reserved_rows(&self) -> u32 {
        self.displayable_reserved_rows.load(Ordering::SeqCst)
    }

    pub fn set_displayable_reserved_rows(&self, rows: u32) {
        self.displayable_reserved_rows.store(rows, Ordering::SeqCst);
    }

    /// Whether this platform hands a MIDP `Canvas` the de-facto standard nav
    /// key codes - `KEY_UP` = -1 down through `KEY_FIRE` = -5, the values Nokia
    /// set and the rest of the J2ME world followed - rather than the positive
    /// table SK-VM's handsets used. A pure J2ME MIDlet that reads a d-pad
    /// straight out of `keyPressed` (호국전기이순신 switches on exactly -5..-1)
    /// gets nothing from the SK-VM codes, so the J2ME emulator sets this and
    /// SK-VM/WIPI, which share the same key enum, leave it off. `net.wie
    /// .EventQueue` and the MIDP `Canvas`'s `getGameAction` read it.
    pub fn midp_uses_standard_key_codes(&self) -> bool {
        self.midp_uses_standard_key_codes.load(Ordering::SeqCst)
    }

    pub fn set_midp_uses_standard_key_codes(&self) {
        self.midp_uses_standard_key_codes.store(true, Ordering::SeqCst);
    }

    /// Whether this title reads its d-pad, select, clear and soft keys as the
    /// SK-VM handset's own positive scancodes rather than the org.kwis codes a
    /// `Card` is handed by default, because its key table is keyed on them.
    /// Looked up in `crate::quirks` and set here by the emulator that loaded the
    /// archive; `net.wie.CardCanvas` reads it.
    pub fn title_keys_as_skvm_scancodes(&self) -> bool {
        self.title_keys_as_skvm_scancodes.load(Ordering::SeqCst)
    }

    pub fn set_title_keys_as_skvm_scancodes(&self, uses: bool) {
        self.title_keys_as_skvm_scancodes.store(uses, Ordering::SeqCst);
    }

    /// Whether the title keeps its own translate and clip on the screen graphics
    /// between frames, so the runtime must not reset them after a paint.
    ///
    /// An SK-VM title draws through `com.skt.m.Graphics2D` from its own loop and
    /// leaves the screen graphics translated into its play area (Chaos블레이드
    /// keeps it at 39,79) frame to frame, rather than re-establishing it from a
    /// blank each paint the way a MIDP `Canvas` does. Resetting the graphics
    /// after the paint - which a MIDP title needs, to start its next paint from
    /// a clean origin - zeroes a translate the SK-VM title still counts on, and
    /// its next `translate(-39,-79)/translate(39,79)` pair, meant to return to
    /// the play area, lands at the screen origin instead. A 162x162 white fill
    /// then sits at (0,0) as a box over the scene. Looked up in `crate::quirks`
    /// and set by the emulator that loaded the archive; read in
    /// `Display.handlePaintEvent`.
    pub fn title_owns_graphics_state(&self) -> bool {
        self.title_owns_graphics_state.load(Ordering::SeqCst)
    }

    pub fn set_title_owns_graphics_state(&self) {
        self.title_owns_graphics_state.store(true, Ordering::SeqCst);
    }

    /// Whether the runtime paints the whole scene each pass for this title,
    /// rather than only the region a `Card.repaint` marked, because the regions
    /// it marks leave earlier screens' pixels standing where no later region
    /// reaches them. Looked up in `crate::quirks` and set here by the emulator
    /// that loaded the archive; `net.wie.CardCanvas` reads it.
    pub fn title_repaints_whole_frame(&self) -> bool {
        self.title_repaints_whole_frame.load(Ordering::SeqCst)
    }

    pub fn set_title_repaints_whole_frame(&self, repaints: bool) {
        self.title_repaints_whole_frame.store(repaints, Ordering::SeqCst);
    }

    /// Whether a freshly created mutable `Image` starts fully transparent for
    /// this title rather than the opaque white MIDP specifies. Looked up in
    /// `crate::quirks` and set here by the emulator that loaded the archive;
    /// `Image.createImage` reads it. See
    /// [`crate::quirks::TitleQuirks::blank_mutable_image_transparent`].
    pub fn title_blank_mutable_image_transparent(&self) -> bool {
        self.title_blank_mutable_image_transparent.load(Ordering::SeqCst)
    }

    pub fn set_title_blank_mutable_image_transparent(&self, transparent: bool) {
        self.title_blank_mutable_image_transparent.store(transparent, Ordering::SeqCst);
    }

    /// How many rows to drop from the bottom of a finished frame before it
    /// reaches the screen, `0` for none. Looked up in `crate::quirks` and set
    /// here by the emulator that loaded the archive; `crate::present` reads it.
    /// See [`crate::quirks::TitleQuirks::present_crop_bottom`].
    pub fn title_present_crop_bottom(&self) -> u32 {
        self.title_present_crop_bottom.load(Ordering::SeqCst)
    }

    pub fn set_title_present_crop_bottom(&self, rows: u32) {
        self.title_present_crop_bottom.store(rows, Ordering::SeqCst);
    }

    /// The size of a picture that is enlarged to the panel as it is decoded,
    /// `None` for none. Looked up in `crate::quirks` and set here by the
    /// emulator that loaded the archive; the image decoder reads it.
    /// See [`crate::quirks::TitleQuirks::stretched_picture`].
    pub fn title_stretched_picture(&self) -> Option<(u32, u32)> {
        match self.title_stretched_picture.load(Ordering::SeqCst) {
            0 => None,
            packed => Some((packed >> 16, packed & 0xffff)),
        }
    }

    pub fn set_title_stretched_picture(&self, size: Option<(u32, u32)>) {
        let packed = size.map_or(0, |(width, height)| (width & 0xffff) << 16 | (height & 0xffff));
        self.title_stretched_picture.store(packed, Ordering::SeqCst);
    }

    /// Whether `org.kwis.msp.io.File` opening a file that is not there to read
    /// throws, rather than opening it empty.
    ///
    /// The handsets disagree, and titles were written against theirs. A KTF
    /// one throws: 나이트테일즈 opens `save0.Dat` read-only and closes it again
    /// as its test for a save, so an empty one opened in its place reads as a
    /// save with nothing in it and the load runs off the end of its array. An
    /// LGT one does not: 일지매 reads its store before it has ever written it
    /// and has no catch around the open. Set by the emulator that loaded the
    /// archive.
    pub fn missing_file_fails_read_open(&self) -> bool {
        self.missing_file_fails_read_open.load(Ordering::SeqCst)
    }

    pub fn set_missing_file_fails_read_open(&self, fails: bool) {
        self.missing_file_fails_read_open.store(fails, Ordering::SeqCst);
    }

    /// Whether the title lays its screens out below the handset's status strip,
    /// so the strip has to be there for them to land where they belong.
    ///
    /// The same fact the WIPI-C side reads out of `ANNUNCIATOR_ROWS_PTR`, for
    /// the titles that reach the strip through `org.kwis.msp.lwc` instead.
    /// Looked up in `crate::quirks` and set here by the emulator that loaded the
    /// archive.
    pub fn title_expects_annunciator(&self) -> bool {
        self.title_expects_annunciator.load(Ordering::SeqCst)
    }

    pub fn set_title_expects_annunciator(&self, expects: bool) {
        self.title_expects_annunciator.store(expects, Ordering::SeqCst);
    }

    /// How tall that strip is for this title, or `None` to take the height the
    /// platform's own size table gives the panel's width.
    pub fn title_annunciator_rows(&self) -> Option<u32> {
        match self.title_annunciator_rows.load(Ordering::SeqCst) {
            0 => None,
            rows => Some(rows),
        }
    }

    pub fn set_title_annunciator_rows(&self, rows: Option<u32>) {
        self.title_annunciator_rows.store(rows.unwrap_or(0), Ordering::SeqCst);
    }

    pub fn current_input_mode(&self) -> u32 {
        self.input_method.read().current_mode()
    }

    pub fn set_current_input_mode(&self, mode: u32) {
        self.input_method.write().set_current_mode(mode);
    }

    pub fn input_composition_size(&self) -> usize {
        self.input_method.read().composition_size()
    }

    pub fn set_input_composition_size(&self, size: usize) {
        self.input_method.write().set_composition_size(size);
    }

    /// Feeds a keypress to the handset's input method.
    ///
    /// The guest clock goes with it: multi-tap finishes a character when the
    /// same key is left alone long enough, and measuring that on guest time
    /// rather than the host's keeps a frontend that runs ticks in batches
    /// typing the same text as one running live.
    /// Lets go of the character the input method is still building, without
    /// finishing it into anything.
    ///
    /// For a field whose whole text has been set from under it: what it was
    /// composing is either already in that text or was meant to be dropped, so
    /// finishing it would add a second copy or text nobody asked for.
    pub fn reset_input_method_composition(&self) {
        let mode = self.input_method.read().current_mode();

        self.input_method.write().set_current_mode(mode);
    }

    pub fn handle_input_method(&self, key: i8, event: u32) -> InputMethodOutput {
        let now = self.platform().now();

        self.input_method.write().handle_input(key, event, now)
    }
}
