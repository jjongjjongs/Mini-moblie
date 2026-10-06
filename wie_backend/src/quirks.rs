//! What this runtime does differently for one named title.
//!
//! Some titles were written against a handset this runtime does not reproduce
//! exactly, and the difference is not something the title can be talked out of
//! at runtime: it sizes its screens from a panel it assumes, or it draws
//! around a status strip it assumes is there. Each of those is a fact about
//! one title, established by running it, and there is nowhere in the emulated
//! API to put such a fact.
//!
//! They therefore live here, keyed by the platform and the application id its
//! descriptor carries, so that every one of them is in a single place rather
//! than spread through whichever module happened to need it first. The
//! reference emulator keeps the same kind of table (`internal/quirkdb`, keyed
//! by a title hash), which is what suggested collecting ours.
//!
//! A title with no entry gets [`TitleQuirks::default`], which asks for nothing.

/// The platform whose descriptor named an application id.
///
/// Ids are only unique within a platform - they are assigned per carrier - so
/// a lookup that did not say which platform it meant could answer for the
/// wrong title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitlePlatform {
    Ktf,
    Lgt,
    Skt,
}

/// Everything known to need doing differently for one title.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TitleQuirks {
    /// The panel the title was drawn for, when that is not the one a host
    /// would pick by default. `None` leaves the host's own default standing.
    ///
    /// A title lays out from what the screen reports, so a panel it was not
    /// written for is one it lays out wrongly. The host has to size its screen
    /// before there is an emulator to ask, so this is read from the archive's
    /// descriptor rather than from a running title.
    pub screen_size: Option<(u32, u32)>,

    /// Whether the title draws its own status strip, so the runtime must leave
    /// room for one above the drawing area rather than handing the title the
    /// whole panel.
    pub expects_annunciator: bool,

    /// How many rows that strip takes, when the size table's answer for the
    /// panel's width is not the one the title was drawn under. `None` leaves
    /// the table standing.
    pub annunciator_rows: Option<u32>,

    /// Whether the title draws its picture a quarter turn clockwise into an
    /// upright panel, because it was meant to be played with the handset held
    /// sideways.
    ///
    /// Nothing in the API says so - the title simply composes a landscape
    /// scene and transposes it on its way to the screen - so the frame has to
    /// be turned back where it is presented. See `wie_backend::present`.
    pub drawn_sideways: bool,

    /// Whether the handset the title was written for took `setClip(x, y, w,
    /// h)` to include the pixel at `x + w` and `y + h`, so the clip is one
    /// pixel wider and taller than MIDP says.
    ///
    /// 이터널사가 (SKT 3826345643) says so itself: every one of its 191
    /// `setClip` calls adds a field that holds -1 to the width and the height,
    /// and it cuts its 16-pixel map tiles out of their strips with
    /// `setClip(x, y, 16 - 1, 16 - 1)`. Clipped the way MIDP reads it, each
    /// tile lost its last column and row, and the map was a grid of black
    /// lines. Other SK-VM titles pass the full size (교실이데아 clips its
    /// tiles to 16 by 16), so this is the title's and not the platform's.
    pub clip_includes_far_edge: bool,

    /// Whether the runtime should wipe the screen buffer to black before every
    /// paint, because the title composes each frame expecting a blank surface
    /// and leaves the rows it does not draw to whatever was there before.
    ///
    /// A MIDP screen buffer keeps what the last frame left in it, and most
    /// titles rely on that. A few do not: they bottom-anchor a fixed-size
    /// picture and draw a heads-up strip over the band above it, and they clear
    /// the whole screen only when a loading screen goes by, trusting the buffer
    /// to hold that clear under the strip's gaps for the rest of the run. Where
    /// the buffer instead holds a title's own earlier screen, its picture shows
    /// through those gaps. Wiping before each paint gives such a title the blank
    /// surface it composes for, and costs it nothing, since it draws its frame
    /// whole every time.
    pub clears_screen_each_paint: bool,

    /// Whether the title reads its d-pad, select, clear and soft keys as the
    /// SK-VM handset's own positive scancodes (up=1, left=2, right=5, down=6,
    /// select=8, clear=99, soft ok=92, soft cancel=90) rather than the org.kwis
    /// codes this runtime hands a `Card` by default.
    ///
    /// An InFusio port keeps its key table as a resource - 에이지오브엠파이어2's
    /// `res/SKT_WIPI.raw` is twenty (raw, internal) byte pairs - and looks the
    /// raw code an event carries up in it, so a code the table has no row for is
    /// simply dropped. The digits, `*` and `#` reach the table as their ASCII
    /// values either way and work; the d-pad, select, clear and soft keys reach
    /// it as org.kwis's negative codes, which the table does not list, so
    /// direction and back did nothing. `net.wie.CardCanvas` reads this and hands
    /// such a title the scancodes its table is keyed on instead.
    pub keys_as_skvm_scancodes: bool,

    /// Whether the title keeps its own translate and clip on the screen graphics
    /// between frames, so the runtime must not reset them after a paint. See
    /// [`crate::System::title_owns_graphics_state`].
    pub owns_graphics_state: bool,

    /// Whether the runtime should repaint the whole scene each pass rather than
    /// only the region the title asked for, because the regions it asks for do
    /// not cover everything that has to be redrawn and stale pixels are left
    /// standing.
    ///
    /// `net.wie.CardCanvas` paints only the dirty region a `Card.repaint` marks,
    /// so that an ez-i title typing a dialogue out one glyph cell at a time
    /// leaves the rest of the box standing. A title driven through
    /// `org.kwis.msp.lwc` breaks that assumption: its `ProxyCard` asks for
    /// partial regions that fall short of where an earlier screen drew (its
    /// title band, a divider rule), and those earlier pixels are never inside a
    /// later region, so they stay on screen under the new one. 학교가는 길 (LGT
    /// 00023917) is where that shows: its comic select screen kept the title
    /// laid over it and the building screen kept a stray red rule, until the
    /// region was ignored and the scene painted whole. `net.wie.CardCanvas`
    /// reads this.
    pub repaints_whole_frame: bool,

    /// Whether a freshly created mutable `Image` (`Image.createImage(w, h)`)
    /// starts fully transparent for this title, rather than the opaque white
    /// MIDP specifies and this runtime fills by default.
    ///
    /// The white default is correct and most titles want it - 미니스포츠클럽
    /// composes its scene into the lower rows of a screen-sized back buffer and
    /// relies on the untouched rows covering the frame behind it. 던전앤파이터
    /// 격투가 is the exception, and it is a workaround for an emulator
    /// interaction rather than a trait of the title: its native engine, driven
    /// through the Java layer, composes into a 240x296 mutable back buffer, and
    /// on the white fill it follows a path that hands its own C blitter a
    /// framebuffer's indirect-pointer handle as if it were the pixel address
    /// and stores an intro sprite (`/img/39.gsg`) over the handle's header,
    /// corrupting it and faulting the next draw out of bounds. The handset runs
    /// the same engine on the same white buffer without faulting, so the fault
    /// is this runtime's; until the engine's framebuffer path is modelled
    /// exactly, starting this title's mutable images transparent keeps it on
    /// the path it took before the white fill and renders as it did on the
    /// handset. Read through [`crate::System::title_blank_mutable_image_transparent`].
    pub blank_mutable_image_transparent: bool,

    /// How many rows to drop from the bottom of the frame before it reaches the
    /// screen, `0` for none.
    ///
    /// A GAMEVIL title draws its scene in the top `height - 80` rows and its own
    /// on-screen keypad (▲▼◀▶ ok 취소) along the bottom 80, meant for a
    /// touchscreen handset. 놈4 (LGT 0002FBB4) is given a 240x400 panel so its
    /// 320-row scene is not squeezed, which leaves that keypad on the bottom 80.
    /// WIE already offers the player a keypad of its own, and the title takes the
    /// WIPI direction/OK key events it drives, so the on-screen one is redundant;
    /// cropping those 80 rows away shows the 320-row scene alone. The title still
    /// lays out for the panel it was given - only what the host shows is trimmed.
    /// See `wie_backend::present`, read through
    /// [`crate::System::title_present_crop_bottom`].
    pub present_crop_bottom: u32,
}

const fn panel(width: u32, height: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: Some((width, height)),
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A 240x296 title whose mutable images must start transparent rather than the
/// opaque white MIDP specifies. See [`TitleQuirks::blank_mutable_image_transparent`].
const fn panel_transparent_mutable(width: u32, height: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: Some((width, height)),
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: true,
        present_crop_bottom: 0,
    }
}

const fn annunciator() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: true,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A strip of a height the size table does not give for this panel.
const fn annunciator_of(rows: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: true,
        annunciator_rows: Some(rows),
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

const fn sideways() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: true,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A title that needs a blank surface before each paint and asks nothing else
/// of the panel. See [`TitleQuirks::clears_screen_each_paint`].
const fn clears_frame() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: true,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// Every title this runtime knows something about, and what it knows.
///
/// The reasoning behind each entry is at the function that reads it - the
/// screen size at `LgtEmulator::screen_size`, the status strip at
/// `wie_lgt`'s `title_expects_annunciator`, the quarter turn at
/// `wie_backend::present`.
///
/// A KTF title is shown the whole panel unless it lays itself out around a
/// strip. 던전앤파이터 격투가 was listed for one and is not any more - it draws
/// 296 rows into a 320-row panel and what it leaves under them is its own
/// business, and it draws the same either way. 만귀토벌전 and 셔터2 데스트니
/// stay listed: they place every screen inside 204 of their 220 rows, so the
/// strip's height is what their layout is measured from, and without it they
/// lose the screen rather than a band at the bottom.
const fn clip_includes_far_edge() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: true,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A title whose key table is keyed on the SK-VM handset's positive scancodes.
/// See [`TitleQuirks::keys_as_skvm_scancodes`].
const fn skvm_scancode_keys() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: true,
        owns_graphics_state: false,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A title that keeps its own translate and clip on the screen graphics between
/// frames. See [`TitleQuirks::owns_graphics_state`].
const fn owns_graphics_state() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: true,
        repaints_whole_frame: false,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

/// A title whose paint regions do not cover everything that has to be redrawn,
/// so the scene has to be painted whole each pass. See
/// [`TitleQuirks::repaints_whole_frame`].
const fn repaints_whole_frame() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
        keys_as_skvm_scancodes: false,
        owns_graphics_state: false,
        repaints_whole_frame: true,
        blank_mutable_image_transparent: false,
        present_crop_bottom: 0,
    }
}

impl TitleQuirks {
    /// This entry, for a title that also clips the way
    /// [`clip_includes_far_edge`](Self::clip_includes_far_edge) describes.
    const fn with_clip_including_far_edge(self) -> Self {
        Self {
            clip_includes_far_edge: true,
            ..self
        }
    }

    /// This entry, for a title that also needs a blank surface each paint the
    /// way [`clears_screen_each_paint`](Self::clears_screen_each_paint)
    /// describes.
    const fn with_screen_cleared_each_paint(self) -> Self {
        Self {
            clears_screen_each_paint: true,
            ..self
        }
    }

    /// This entry, with the bottom `rows` cropped from the frame before it
    /// reaches the screen the way
    /// [`present_crop_bottom`](Self::present_crop_bottom) describes.
    const fn with_bottom_cropped(self, rows: u32) -> Self {
        Self {
            present_crop_bottom: rows,
            ..self
        }
    }
}

const QUIRKS: &[(TitlePlatform, &str, TitleQuirks)] = &[
    // 미니게임 히어로즈2 터치: repaints a 240x80 sponsor banner along the
    // bottom of whatever height it is told, so its 320 rows of screen need a
    // 400-row panel underneath them.
    (TitlePlatform::Lgt, "00030F5B", panel(240, 400)),
    // 놈4 (GAMEVIL): the same engine. It draws its screen in the top `height-80`
    // and its own on-screen keypad (▲▼◀▶ ok 취소) along the bottom 80 rows, so on
    // the default 240x320 panel the screen is squeezed into 240 rows and the
    // keypad lands below the panel, leaving a stale band. Its screens are 320
    // rows, so they want a 240x400 panel the keypad sits under - and since WIE
    // offers its own keypad and the title takes the WIPI keys that drive from it,
    // those bottom 80 rows are cropped away so the 320-row scene shows alone.
    (TitlePlatform::Lgt, "0002FBB4", panel(240, 400).with_bottom_cropped(80)),
    // 초코초코타이쿤 (게임빌): a native C engine that composes its scene below the
    // handset's 24-row status strip - its タイムゲージ machine starts one strip
    // down and fills the rest, with the cacao-block targets and their counts on
    // the last rows. Without the strip reserved the whole scene lands 24 rows low
    // (a black band above it) and those bottom rows - the targets, the counts,
    // the `#:SKIP`/key bar - fall off the panel. Reserving the strip puts the
    // scene back at the top and brings its bottom back onto the screen.
    (TitlePlatform::Lgt, "00029F79", annunciator()),
    // KBO 프로야구 2009 (LGT, ZIO interactive): the same - it lays its screens out
    // below the 24-row strip (the batter view's `*:게임메뉴`/`#:작전메뉴` bar, the
    // roster's `현재자산` line are on the last rows), so without it reserved the
    // scene sits a strip low and that bottom bar is cut. Its KTF build (01035ACD)
    // draws the whole panel itself and keeps its own 240x320 entry.
    (TitlePlatform::Lgt, "0002A8D4", annunciator()),
    // 베이징올림픽 2008 (LGT, ZIO interactive): the same ZIO engine as KBO 2009,
    // composing below the 24-row strip, so without it reserved every screen lands
    // a strip low with its bottom row cut.
    (TitlePlatform::Lgt, "0002728F", annunciator()),
    // 판타지나이트: without the strip its bottom 24 rows keep a stale band.
    (TitlePlatform::Lgt, "0002787C", annunciator()),
    // 프로야구 2009.
    (TitlePlatform::Lgt, "0002CB6A", annunciator()),
    // 지크2: centres a 296-row picture and then adds the strip itself.
    (TitlePlatform::Lgt, "0002A52B", annunciator()),
    // 알바타이쿤2: every screen it draws lands exactly one strip down.
    (TitlePlatform::Lgt, "0002D4D0", annunciator()),
    // 아무이유없어: its coloured words are stamped straight into the screen's
    // memory by its own renderer (`0xbfd4`), which adds the 24-row strip to every
    // row it writes (`adds r1, #0x18` at `0xbfee`) - the pointer
    // `MC_grpGetFrameBufferPointer` hands it starts at the top of the panel, the
    // strip included, on the handset. Without the strip every highlighted word
    // sat 24 rows below the line it belongs to while the black text around it,
    // drawn through `MC_grpDrawImage`, stood where it should.
    (TitlePlatform::Lgt, "00029288", annunciator()),
    // 마구마구2011: the same, and it composes through its own off-screen
    // surface rather than the `MC_grp*` calls, so nothing but the strip's
    // height moves it. Told the whole 320-row panel it asks for a 240x320
    // surface, lays its scene out a strip down it, and flushes the lot: the
    // top 24 rows reach the screen black and the bottom 24 - the `CLR 메뉴`
    // and `TIME` bar under the diamond - fall off the end of the surface and
    // are not drawn at all. Told the 296 a strip leaves, it asks for 240x296
    // and fills it from its first row.
    (TitlePlatform::Lgt, "00030DD8", annunciator()),
    // 셔터2 데스트니: the same SDK and the same arithmetic - it clips every
    // screen to 176x204 on its 176x220 panel - and it answers the same way,
    // 57 colours down to 6 without the strip.
    (TitlePlatform::Ktf, "01037EBF", annunciator_of(16)),
    // 던전앤파이터 격투가: draws 296 rows into the 320 its descriptor asks for
    // and leaves the rest alone, so the panel is the 296 it draws.
    (TitlePlatform::Ktf, "0103BF27", panel_transparent_mutable(240, 296)),
    // 인형뽑기타이쿤: every screen - title, menu, town and the cutscenes between
    // - leaves the bottom 24 rows of its 240x320 panel alone for the handset's
    // soft-key strip, which our buffer showed as a white band. It reserves that
    // strip from the height it is told, so a shorter panel only moves the band up;
    // instead keep the full 320 it draws for and crop the 24-row strip away as the
    // frame reaches the screen.
    (TitlePlatform::Ktf, "0102E32F", panel(240, 320).with_bottom_cropped(24)),
    // 탁재훈 신맞고2007 and 광수의 똥! 생각: the same handset strip. Every
    // screen stops 24 rows short of the 240x320 panel, and the rows under it
    // keep whatever stood there before - black under one, a loading screen's
    // watermark under the other.
    (TitlePlatform::Ktf, "010366DB", panel(240, 320).with_bottom_cropped(24)),
    (TitlePlatform::Ktf, "01031E04", panel(240, 320).with_bottom_cropped(24)),
    // 2006현영맞고: its pictures are 300 rows tall, and the 20 under them kept
    // the title's logo under the menu.
    (TitlePlatform::Ktf, "01033511", panel(240, 320).with_bottom_cropped(20)),
    // 맞고삼국대전: 176x220 by its descriptor, and every screen - title, map,
    // story - fills the top 204 rows and leaves 16 blank under them.
    (TitlePlatform::Ktf, "010247AB", panel(176, 220).with_bottom_cropped(16)),
    // 두뇌게임Q: 176x220 by its descriptor, but every screen - its title, the
    // profile form, the confirmation box - paints the top 210 rows and leaves a
    // 10-row white strip under them. Keep the 220 the title lays itself out in
    // and crop that strip off what is shown, so it is not a band beneath the
    // picture.
    (TitlePlatform::Ktf, "01038485", panel(176, 220).with_bottom_cropped(10)),
    // 겟앰프드: its descriptor says 240*320, but every full-screen picture it
    // carries - title, menu, each map - is 240x296, and it centres its popup
    // frame in whatever height the screen reports. Told 320 it put the frame at
    // `(320 - 168) / 2 = 76`, twelve rows below the text it had laid out for
    // `(296 - 168) / 2 = 64`, so a notice's title sat across the bottom of its
    // own title bar and its first line across the bottom of the message box.
    (TitlePlatform::Ktf, "01031C0A", panel(240, 296)),
    // 텐가이: its descriptor names no panel, and the pictures it carries say
    // which one it was drawn for. Its intro background is 88x204 and it paints
    // it twice, at 0 and at half the width the screen reports, which tiles a
    // 176-wide panel exactly and leaves a 32-column gap of the screen before it
    // on a 240-wide one. 204 rows and a 16-row strip is the 220 of that panel.
    (TitlePlatform::Ktf, "01031C47", panel(176, 220)),
    // KBO 프로야구 2009: its descriptor says 176*220 and every screen it lays
    // out is 240 wide. It puts 선수명단 and 선수상세설명 side by side, which
    // only fits in 240 - on a 176-wide panel the right one is cut down the
    // middle - and its menu header is two strips, `KBO 프로야구 2009` and the
    // screen's name, which land on top of each other when there is no room for
    // the second. Its title picture is the same story: the 2009 under the logo
    // and the rating badge in the corner are both off the bottom and the right
    // of a 176x220 panel. All 320 rows, not the 296 a strip would leave: its
    // key bar - `CLR:뒤로 OK:선택 #:도움말` - is on the last of them.
    (TitlePlatform::Ktf, "01035ACD", panel(240, 320)),
    // 대박돈까스: its descriptor names no panel, and everything it draws says
    // 176x220. Its gameplay screen is a fixed 176x205 composition - the shop
    // floor, the 확장/청소/홍보/정보/폐점 bar down the right, the HP and
    // TOTAL/TODAY strip and the MENU/PAUSE keys under it - and on a 240x320
    // panel that sat in the top left with the panel's own colour beside it.
    // Its menus are the other half of the story: those it lays out from the
    // height the screen reports, so the title screen's copyright and `OK` went
    // to row 311 and stayed there under the gameplay that followed, which is
    // the leftover menu text showing below the shop.
    (TitlePlatform::Ktf, "01025922", panel(176, 220)),
    // 월드오브드래곤: its descriptor names no panel, and it centres a 176x204
    // screen in whatever it is given - on a 240x320 panel its title and play
    // field sat at (32, 58) inside a white border, and on 176x220 eight rows
    // down with white above. A strip does not take the difference: the title
    // still centres in the 220 it is told and loses its bottom rows under it.
    // The 204 rows it draws are the panel.
    (TitlePlatform::Ktf, "0102FC8C", panel(176, 204)),
    // 초밥의달인3 (KTF PD004152): a Java title whose screens draw inside a
    // 240x296 clip - the bottom 24 rows are the handset's soft-key strip, which
    // it never touches - while some earlier screen fills the whole 240x320 with
    // its orange background. Our MIDP screen buffer keeps that orange under the
    // 24-row strip the later screens leave alone, so its load screen and menus
    // sat over an orange band. It redraws its screen whole every paint, so
    // wiping the buffer first costs it nothing and leaves that strip black
    // instead of a stale frame. The panel stays 240x320: the title clips to the
    // full height on other screens, so it is not drawn for a shorter one.
    (TitlePlatform::Ktf, "010346A2", clears_frame()),
    // 소울게이트: takes a 240x320 screen, composes every frame into a 320x240
    // off-screen buffer of its own, and copies that onto the screen a quarter
    // turn clockwise - the handset was meant to be turned sideways to play it.
    (TitlePlatform::Lgt, "000323B3", sideways()),
    // 질주쾌감스케쳐: the same, from its terms screen on - it lays every frame
    // out sideways for a handset held landscape, so the picture reaches the
    // upright panel a quarter turn off until it is turned back.
    (TitlePlatform::Lgt, "00031347", sideways()),
    // 학교가는 길: an org.kwis.msp.lwc title whose ProxyCard repaints partial
    // regions that leave the previous screen's own pixels standing - its comic
    // select screen kept the title band drawn over it, and the building screen
    // kept a red divider rule from the screen before. The regions it marks
    // never cover those, so nothing clears them until the scene is painted
    // whole. See TitleQuirks::repaints_whole_frame.
    (TitlePlatform::Lgt, "00023917", repaints_whole_frame()),
    (TitlePlatform::Skt, "3826345643", clip_includes_far_edge()),
    // 몬스터보이: composes its field map a tile at a time, clipping each 16x16
    // cell with `setClip(x, y, 15, 15)` before it blits the tile sheet. The
    // handset it was drawn for counted the far edge in, so 15 reached the
    // sixteenth pixel; on the exclusive-clip default each tile fell a pixel short
    // on its right and bottom and left the fill under the map showing through as
    // an orange grid over every screen. Counting the far edge in draws the whole
    // cell and the grid is gone.
    (TitlePlatform::Skt, "0052335225", clip_includes_far_edge()),
    // 동방사신기: drawn for a handset whose clip took in its far edge. It clips
    // the whole screen with `setClip(0, 0, 239, 319)` where 239 and 319 are the
    // last pixel, not a width and a height - and its field a tile at a time the
    // same way - so on the exclusive-clip default the screen lost its last row
    // and column and every tile fell a pixel short, which showed as the grid
    // over the field and the torn bands across the menu. Counting the far edge
    // in draws each region whole. It is sized to a 176x220 handset so its
    // screens fill the panel instead of leaving a margin.
    (TitlePlatform::Skt, "0052550560", panel(176, 220).with_clip_including_far_edge()),
    // 광개토대왕정벌기: drawn for a 176x216 handset. Size it to that panel so its
    // screens fill the display instead of sitting in the top-left of the default.
    (TitlePlatform::Skt, "0047856534", panel(176, 216)),
    // 인형뽑기타이쿤 (SK-VM): every full-screen background it ships is 176x202
    // (vil_map, school_bg, room_bg, intro_bg, dollshopbg, ...), so that is the
    // handset it was drawn for. On the 240x320 default it drew each 176-wide
    // screen into the top-left and left the 64 columns and 118 rows outside it
    // holding whatever the last screen had put there - the menu girl's arm stayed
    // down the right of the town, and the menu's own items spread down a screen
    // twice as tall as the art. Sizing it to 176x202 lands every screen whole and
    // matches the KTF build. Its descriptor (1.msd) carries no DD-ProgName, so the
    // id falls back to the filename stem "1", which is what the screen lookup and
    // the save layer both compute for it.
    (TitlePlatform::Skt, "1", panel(176, 202)),
    // 맛대맛: an adaptive SK-VM title - it reads the screen size once and lays
    // every screen out from it - but its art is a fixed size, so on the 240x320
    // default each screen's pieces anchored to the top and bottom edges with a
    // dead checkered band between them, which read as scattered graphics. It is
    // 240 wide (its titles and portraits fill that) and its layout closes up at
    // 240 tall, so size it to a 240x240 panel.
    (TitlePlatform::Skt, "0051017321", panel(240, 240)),
    // 타워오브바벨3 (Manastone, SK-VM): drawn for a 176-wide handset - its widest art is exactly 176 and its cutscenes fill
    // that panel edge to edge - so on the 240x320 default each scene sat centred
    // with a black border; the 176x220 panel it was made for fills the screen.
    (TitlePlatform::Skt, "0054563061", panel(176, 220)),
    // 썸머스케치: a 2003 ensony title laid out for a 120x160 handset. It takes
    // the screen size it is told and draws to it, but its art sits at a fixed
    // size, so on the 240x320 default its title, menu and scenes shrink into the
    // top of the screen with a dead band below. Sizing it to 120x160 fills the
    // panel - title, menu and scenes all reach every edge.
    (TitlePlatform::Skt, "0027765524", panel(120, 160)),
    // 얼라이브: drawn for a 176x220 handset - its title sky, menu and the city
    // under them, and every screen after, are laid out 176 wide and down to
    // row 220. On the 240x320 default it drew in the left 176 columns, left
    // what the previous screen had put in the rest, and split its title
    // between the top of the screen and the bottom.
    (TitlePlatform::Skt, "0174585654", panel(176, 220)),
    // 아슬아슬타워쿤: built for a 176-wide panel. Its Canvas init branches on
    // getWidth(): at 176 or under it stacks its two title images - title1
    // (176x64) over title0 (176x160) - to fill the whole screen with its world
    // map, and the menu draws over it. Told a wider screen it takes a second
    // path that lays title0 and title2 side by side across the top 160 rows
    // only and leaves the rest of its back buffer the white a mutable image
    // starts as, so on the 240x320 default the menu sat on a bare white band
    // below the map. Given the 176x220 panel it was drawn for, the map fills
    // the screen again.
    (TitlePlatform::Skt, "0054981375", panel(176, 220)),
    // 사고뭉치트윈즈: clips and clears a 120x144 play area centred on the
    // Canvas and fills the rest with a tiled pattern, so on the 240x320
    // default it played in a small box in the middle of the screen. 144 rows
    // is the Canvas of a 120x160 handset, the sixteen soft-key rows under it.
    (TitlePlatform::Skt, "0054532850", panel(120, 160)),
    // 모바일크래프트: a 2004 title laid out 120 wide and centred on the Canvas -
    // its title art, its menu and its battlefield all stand about 120x140 - so
    // on the 240x320 default it drew in a small square adrift in the middle of
    // the screen. The 120x160 handset it was made for is the panel that fills.
    (TitlePlatform::Skt, "3507010790", panel(120, 160)),
    // 로맨스소드: drawn for a 176-wide handset - its title art comes in 120- and
    // 176-wide variants (main_logo_120, main_logo_176) chosen off getWidth(). On
    // the 240x320 default it took the width>=240 branch, asked for a
    // main_logo_240 the jar never carried, and drew that null image every frame
    // - the paint threw and the screen stayed black. On the 176 panel it loads
    // the art it ships.
    (TitlePlatform::Skt, "0050378735", panel(176, 208)),
    // 레스토랑타이쿤2006: ships one asset set, in an img_176 folder, and picks
    // the folder off getWidth() - at 240 it asked for an img_240 that is not
    // there and drew into a small box adrift on the black default. Its full
    // background mbg176 is 176x202, so the 176 panel is the one it composes for.
    (TitlePlatform::Skt, "0052039193", panel(176, 208)),
    // 다크슬레이어2: lays itself out to whatever screen it is told, but its field
    // is a fixed 182x154 view with a status panel under it that together stand
    // 195 rows tall, and it centres that block on the Canvas. On the 240x320
    // default the block sat in the middle with a wide black band above and below
    // it; the title, the cutscenes and every other screen centre the same way
    // and leave the same bands. A 240x208 panel is the smallest the field block,
    // the title and the dialogue all fit, so told that size the game fills the
    // screen with no content lost - only the unused black is gone.
    (TitlePlatform::Skt, "0049884301", panel(240, 208)),
    // 바운티블루스: drawn for a 128-wide handset - its field, portraits and
    // dialogue art are 128 wide - and it lays the screen out from the Canvas
    // height (`getHeight() + 16`): a 144-row field and a 32-row status panel
    // under it that repeats the stage banner in a frame. On the 240x320
    // default the panel ran to the bottom of the screen as a dark red block;
    // at 128x160 it lost its lower half and the banner was cut through. 176
    // rows is the height its field and panel add up to, and the title, the
    // chapter screens and the dialogue boxes all sit inside it.
    //
    // It also clips the way 이터널사가 does: its two clip helpers pass
    // `w - 1, h - 1` when a flag it sets unconditionally in its canvas
    // constructor is on, and it cuts its 16-pixel field tiles out of their
    // strips that way. Read as MIDP reads it, each tile lost its last column
    // and row and the forest was a grid of black lines.
    (TitlePlatform::Skt, "0145741367", panel(128, 176).with_clip_including_far_edge()),
    // 삼국쟁패 패왕전기 (게임빌): draws a fixed-size battle field centred on the
    // Canvas from `getWidth()/2` and `getHeight()/2` offsets, so the play area
    // stays about 162 wide wherever it lands and only the margin around it
    // grows with the screen. On the 240x320 default the battle sat in a small
    // box in the middle with wide dead borders. Its menus scale to whatever
    // panel they are given, but 176 is the narrowest common SKT panel that
    // still holds the fixed field, so at 176x220 the battle fills the screen
    // and the field's centring leaves only a few pixels each side.
    (TitlePlatform::Skt, "0047375473", panel(176, 220)),
    // 컴투스프로야구2 (SK-VM): draws a fixed 176x220 screen and centres it on
    // `XDisplay.width/height`, so on the 240x320 default it sits at (32, 50)
    // with wide white borders (`refresh(32, 50, 176, 220)`). Given the 176x220
    // it was drawn for, the centring offset is zero and it fills the screen.
    (TitlePlatform::Skt, "0050230368", panel(176, 220)),
    // 엑스피드스노보드 (SKT MIDlet): laid out for a ~180x220 panel. On the
    // 240x320 default its player/mode-select screens drew the character portrait
    // twice - a second copy sliding off the right - because the extra width left
    // room its fixed layout filled with a stray repeat. Its height is 220 (a
    // shorter panel leaves the previous screen showing between its content and
    // the status bar); 180 wide keeps the right edge its 176-wide neighbour
    // clipped. Keyed by DD-ProgName, not the archive filename.
    (TitlePlatform::Skt, "0052018663", panel(216, 220)),
    // 크레이지버스 (COMO2D): draws a fixed-size bus interior centred on the
    // Canvas and fills the rest with its green (`fillRect(0, 0, lcdW, lcdH)`
    // then `drawImage(busBase, centerX, centerY, HCENTER|VCENTER)`). Its
    // Canvas init clamps its play field to 170x200 - `if (width > 170) { width
    // = 170 }`, `if (height > 200) { height = 200 }` - and keeps the bus centre
    // at the physical screen's own centre, so on the 240x320 default the bus
    // (its `busBase176206.png` skin is 186x206) sat in a small box in the
    // middle with a wide green border all round. The panel it was drawn for is
    // just larger than that 170x200 field: 176x208 leaves only a few pixels'
    // margin, so the bus fills the screen as the title's own screenshots show.
    // The 3D dancer is unaffected - its `setView` is built from the clamped
    // 170x200 field, the same at any panel size, so it keeps its size and only
    // recentres with the bus.
    (TitlePlatform::Skt, "0027684826", panel(176, 208)),
    // 대해적시대 (gametoilet, SeaDog): its `GameCanvas` reads its size from the
    // Canvas (`width = getWidth()`, `height = getHeight() + 16`) and centres its
    // artwork on the physical screen, clamping its play field to a height of 202
    // (`if (height > 202) margin = (height - 202) / 2`). On the 240x320 default
    // its title and world sat in the middle of a wide black border. 176x208 is
    // the 176-class panel it was drawn for, so the field fills the screen with
    // only a few rows' margin. Its id is the descriptor's `DD-ProgName`
    // (0055719339), not the archive's filename (0055719338).
    (TitlePlatform::Skt, "0055719339", panel(176, 208)),
    // 크레이지스노보드 (gametoilet, SnowBoard): same house engine as 대해적시대,
    // laying its 176-class field (its bounds are the 200/202/208 its Canvas
    // counts in) out centred on the screen. On the 240x320 default it drew in a
    // box in the middle; 176x208 is the panel it fills.
    (TitlePlatform::Skt, "0054300745", panel(176, 208)),
    // KMakerB (크레이지메이커): its Canvas lays out from `getWidth()` and
    // `getHeight() + 16`, branching on a 178-wide threshold onto a 168-wide
    // field and counting its rows in 220. On the 240x320 default its screens
    // sat centred with a wide margin; 176x220 is the panel the layout is
    // measured for.
    (TitlePlatform::Skt, "0050092169", panel(176, 220)),
    // com.softenter.p_te: its `PteCanvas` branches on the Canvas width - a
    // 176-wide screen takes a 208-row layout, a narrower one a 128x160 layout.
    // On the 240x320 default it took the 176 path and drew it centred in the
    // larger canvas with a border. 176x208 is that path's own panel.
    (TitlePlatform::Skt, "3500406052", panel(176, 208)),
    // MBC무한도전 (Challenge): drawn for a 128-wide handset - its backgrounds
    // (`bg1.png`, `title_logo.png`) are 128 wide and its Canvas branches
    // `if (width > 128)`, laying its ~120x142 field out from `getWidth()/2` and
    // `(getHeight() + 16)/2`. On the 240x320 default the field sat centred with
    // its content spread apart top and bottom over empty rows. 128x160 is the
    // 128-class panel it fills. Its id is the descriptor's `DD-ProgName`
    // (9000000008), not the archive's filename (1000000009).
    (TitlePlatform::Skt, "9000000008", panel(128, 160)),
    // STRIKERS1999 (WIPI/org.kwis.msp): its `S1999Card` draws the whole game
    // into a fixed 120-wide offscreen (`m_nScreenW = 120`, `m_nImgBuf =
    // createBlankImage(120, getHeight())`) and blits it centred at
    // `(getWidth() - 120) / 2`, taking its playfield height straight from
    // `getHeight()` with no clamp. On the 240x320 default the buffer was 120
    // wide by 320 tall, so the game sat in a centred column stretched to twice
    // its height - enemies spawning far above the ship, the intro formation
    // strung out down the middle. It is a 120-wide handset title, so 128x160
    // (the field centres with a 4-pixel margin) gives it a screen the right
    // shape. Elements are anchored to `getHeight()`, so they follow whatever
    // height the panel yields.
    (TitlePlatform::Skt, "0050608430", panel(128, 160)),
    // 신시티 (GameAppMain): drawn for a 176-wide handset - its title (`logo.png`
    // 176x202), dialogue box and status strip are all 176 wide, and it lays its
    // map and the year/money bar out down to row 208. On the 240x320 default the
    // map sat at the top with the bar stranded far below it over black. 176x208
    // is the panel it fills.
    (TitlePlatform::Skt, "0050978830", panel(176, 208)),
    // 츄리닝 (Churining): its backgrounds are 128-wide (`wishjar_bg.png` 128x140,
    // `main_bg.png` 120x144), so it is a 128-class title. On the 240x320 default
    // it drew in a box in the corner; 128x160 is the panel it fills.
    (TitlePlatform::Skt, "0050664330", panel(128, 160)),
    // Magical (마법사 타이쿤): its Canvas clamps its play field to 128x146
    // (`if (width > 128) width = 128`, `if (height > 146) height = 146`) and
    // centres it. On the 240x320 default it sat in a small box in the middle.
    // 128x160 is the 128-class panel that field fills.
    (TitlePlatform::Skt, "0048926642", panel(128, 160)),
    // 파파라치타이쿤 (papa): drawn for a 128-class handset - its artwork is 132
    // wide at most and its Canvas selects a layout by exact panel height, capping
    // it at a 160-row field (`if (height >= 160) field = 160`, with 143/145/148
    // variants for the shorter panels). On the 240x320 default its alley scene,
    // title and cursor scattered across the screen at coordinates meant for a
    // narrow one. 128x160 is the panel the 160-row layout is drawn for.
    (TitlePlatform::Skt, "0053919219", panel(128, 160)),
    // 미니스포츠클럽 (GAMEVIL, Mini): its GAMEVIL framework reads the screen from
    // `com.xce.lcdui.XDisplay.width`/`height2` and lays every element out against
    // it with no clamp, so on the 240x320 default its menu, logo, character art
    // and stat box scattered to the corners of the larger screen with wide gaps
    // between. It is a 176-wide GAMEVIL title (its title art centres with the
    // ~13% margin a 176 panel leaves in 240). Its height is the 200 its own code
    // counts in: it renders each screen into a `createImage(width, height)`
    // buffer and blits that whole buffer every frame, and it fills only 200 rows
    // of it, so a taller panel left the top rows the render never reached
    // showing the previous screen (its menu, a stale logo) through the gap.
    // 176x200 makes the buffer exactly the height the game paints.
    //
    // In play it also needs the screen wiped before each paint. Its match
    // screens are not composed into that full-height buffer: each sport
    // bottom-anchors a fixed picture (baseball's is 200x180, built by
    // `ImageLoader.loadBaseBack` and drawn `BOTTOM|HCENTER` at the screen's
    // bottom centre) and draws a score-and-distance strip over the ~20-row
    // band left above it. The game clears the whole screen only when a loading
    // screen passes (`loadingBar` fills it, `paintPlayInfo` does not), and
    // trusts the buffer to keep that clear under the strip's gaps. Ours instead
    // kept the menu it drew before the match, so the `MINI SPORTS CLUB` logo
    // showed between the score boxes. Wiping the buffer each paint gives the
    // match the blank band it composes over, and the menu and loading screens,
    // which fill the buffer whole, do not notice.
    (TitlePlatform::Skt, "0054401421", panel(176, 200).with_screen_cleared_each_paint()),
    // 물가에돌팅기기IQ (GAMEVIL 2006): the same GAMEVIL framework, reading
    // `XDisplay.width`/`height2` and placing its title, menu, stage bar and
    // puzzle field from the screen edges. On the 240x320 default the menu ran
    // off the right, the bars pinned to the far edges and the field sat small in
    // the middle. 176x220 is the panel the layout is measured for.
    (TitlePlatform::Skt, "0053630031", panel(176, 220)),
    // 에이지오브엠파이어2 (InFusio): a `Card` whose `keyNotify` looks the raw
    // event code up in `res/SKT_WIPI.raw`, a table keyed on the SK-VM handset's
    // own scancodes (up=1, left=2, right=5, down=6, select=8, clear=99, soft
    // keys 92/90). Handed the org.kwis codes this runtime uses by default, the
    // table had no row for the d-pad, select, clear or soft keys and dropped
    // them - only the digits, `*` and `#`, which reach it as ASCII either way,
    // did anything. See `TitleQuirks::keys_as_skvm_scancodes`.
    (TitlePlatform::Skt, "0051574505", skvm_scancode_keys()),
    // 센티멘탈러브+ (InFusio/잼버거): drawn for a 176x220 handset. On the
    // 240x320 default its logo and screens sit in the top-left with the panel's
    // own colour around them; 176x220 is the panel it was laid out for.
    (TitlePlatform::Skt, "0054448880", panel(176, 220)),
    // 포켓올림픽 (POCKETSPACE 2004): composes every screen at fixed coordinates
    // for a 176x220 handset and never reads the panel it is given. On the
    // 240x320 default its stadium, track and runner were stranded in the
    // top-left with the sky and field bands stretched apart; at 176x220 the
    // scene fills the panel and its footer lands on the bottom row.
    (TitlePlatform::Skt, "3503930101", panel(176, 220)),
    // 프린스메이커 온달편 (Muncle, SK-VM): drawn for a 128x128 handset - its
    // title art is 128x112 and its windows, bars and scenes 120 to 128 wide -
    // but it sizes its backdrops and anchors its message bar, menu icons and
    // map from getWidth/getHeight. On the 240x320 default those pieces spread
    // to the far edges and corners while the fixed-size art stayed small: the
    // title sat off to the right, the dialogue box ran the full height and the
    // map, icons and message bar were strewn around the schedule screen. Taller
    // 128-wide panels still left the menu floating in sky and a band of empty
    // rows between the schedule window and the message bar; at 128x128 the
    // menu, story, schedule and training scenes each fit together edge to edge.
    (TitlePlatform::Skt, "0054980867", panel(128, 128)),
    // 포키의 모험 (Funtory, SK-VM): drawn for a 176x220 handset - its title
    // art is `title_bg_176.png`, 176x220 - but it takes the ground strip and
    // the scrolling layers' edges from `getWidth`. On the 240x320 default the
    // fixed-size pictures sat centred in white borders while the ground and the
    // flying monsters ran on to the full width, so the scenes overlapped each
    // other and the screen's edge. At 176x220 every layer meets the edge.
    (TitlePlatform::Skt, "0050079226", panel(176, 220)),
    // 엑스맨(X-Men): sizes its screens from getWidth/getHeight, so it fits
    // whatever panel it is given, but was drawn for a 176x220 handset.
    (TitlePlatform::Skt, "0053594173", panel(176, 220)),
    // Chaos블레이드 (GAMEVIL, SK-VM): draws through `com.skt.m.Graphics2D` from
    // its own loop and keeps the screen graphics translated into its centred
    // 162x162 play area (at 39,79) frame to frame. Resetting the graphics after
    // each paint - which a MIDP Canvas needs - zeroed that translate from under
    // it, and one of its `translate(-39,-79)/translate(39,79)` pairs then
    // returned to the screen origin instead of the play area: a 162x162 white
    // fill it lays down before compositing the scene sat at (0,0) as a box in
    // the top-left corner the centred frame never reaches. See
    // `System::title_owns_graphics_state`.
    (TitlePlatform::Skt, "0027571859", owns_graphics_state()),
];

/// What to do differently for the title `aid` on `platform`.
///
/// Returns [`TitleQuirks::default`] - which asks for nothing - for every title
/// without an entry, which is nearly all of them.
/// Entries that hold only on a panel of one width, because one AID names
/// more than one title.
///
/// A publisher could ship different games under the same AID to handsets of
/// different sizes, and the table above cannot tell them apart. 0102A356 is
/// 만귀토벌전 on a 176x220 handset and 동방사신기 on a 240x320 one: the strip
/// the first is laid out under pushed the second a strip down its own panel,
/// cutting the top of its title screen and the `CLR:BACK` line off its
/// bottom, and put a status bar over a game that never had one.
const PANEL_QUIRKS: &[(TitlePlatform, &str, u32, TitleQuirks)] = &[
    // 만귀토벌전: lays every screen out below the strip and inside the rows
    // left under it, so the rows the strip takes off the top are also what
    // lines that layout up. Taken away with the rest of KTF's strips, its last
    // frame fell from 66 colours to 15 - the screen, not a band at the bottom.
    //
    // Sixteen rows rather than the twenty the size table gives a 176-wide
    // panel: the title clears 204 rows of its 220-row panel and puts everything
    // inside them, and 204 + 16 is the panel exactly. Under a twenty-row strip
    // its last four rows - the bottom of the portrait and of the KARMA gauge -
    // went off the end.
    (TitlePlatform::Ktf, "0102A356", 176, annunciator_of(16)),
];

/// [`title_quirks`] for a title running on a panel `width` wide, which also
/// answers the entries that hold only on a panel of that width - see
/// [`PANEL_QUIRKS`].
pub fn title_quirks_on_panel(platform: TitlePlatform, aid: &str, width: u32) -> TitleQuirks {
    for (entry_platform, entry_aid, entry_width, quirks) in PANEL_QUIRKS {
        if *entry_platform == platform && entry_aid.eq_ignore_ascii_case(aid) && *entry_width == width {
            return *quirks;
        }
    }

    title_quirks(platform, aid)
}

pub fn title_quirks(platform: TitlePlatform, aid: &str) -> TitleQuirks {
    for (entry_platform, entry_aid, quirks) in QUIRKS {
        if *entry_platform == platform && entry_aid.eq_ignore_ascii_case(aid) {
            return *quirks;
        }
    }

    TitleQuirks::default()
}

#[cfg(test)]
mod tests {
    use super::{TitlePlatform, TitleQuirks, title_quirks};

    /// One AID, two games: the strip is 만귀토벌전's on its 176-wide panel and
    /// nothing at all on 동방사신기's 240-wide one.
    #[test]
    fn an_aid_shared_by_two_titles_is_told_apart_by_its_panel() {
        let narrow = super::title_quirks_on_panel(TitlePlatform::Ktf, "0102A356", 176);
        assert!(narrow.expects_annunciator);
        assert_eq!(narrow.annunciator_rows, Some(16));

        assert_eq!(super::title_quirks_on_panel(TitlePlatform::Ktf, "0102A356", 240), TitleQuirks::default());
    }

    #[test]
    fn a_title_without_an_entry_asks_for_nothing() {
        assert_eq!(title_quirks(TitlePlatform::Lgt, "00025C2B"), TitleQuirks::default());
        assert_eq!(title_quirks(TitlePlatform::Skt, "00030F5B"), TitleQuirks::default());
    }

    /// 바운티블루스 needs both its panel and the wider clip.
    #[test]
    fn an_entry_can_carry_a_panel_and_a_clip_rule_together() {
        let quirks = title_quirks(TitlePlatform::Skt, "0145741367");

        assert_eq!(quirks.screen_size, Some((128, 176)));
        assert!(quirks.clip_includes_far_edge);
    }

    /// 미니스포츠클럽 needs its panel and a wipe before each paint.
    #[test]
    fn an_entry_can_carry_a_panel_and_a_per_paint_wipe_together() {
        let quirks = title_quirks(TitlePlatform::Skt, "0054401421");

        assert_eq!(quirks.screen_size, Some((176, 200)));
        assert!(quirks.clears_screen_each_paint);
    }

    /// The wipe is one title's, not every title's.
    #[test]
    fn a_title_without_the_wipe_rule_does_not_get_it() {
        assert!(!title_quirks(TitlePlatform::Skt, "0027684826").clears_screen_each_paint);
        assert!(!title_quirks(TitlePlatform::Skt, "0145741367").clears_screen_each_paint);
    }

    /// 학교가는 길 has to paint its scene whole; a title without the rule keeps
    /// painting only what it asked for.
    #[test]
    fn the_whole_frame_repaint_is_one_titles_and_not_every_titles() {
        assert!(title_quirks(TitlePlatform::Lgt, "00023917").repaints_whole_frame);
        assert!(!title_quirks(TitlePlatform::Lgt, "000323B3").repaints_whole_frame);
        assert!(!TitleQuirks::default().repaints_whole_frame);
    }

    /// Ids are assigned per carrier, so an entry must not answer for the same
    /// id on another platform.
    #[test]
    fn an_entry_answers_only_for_its_own_platform() {
        assert_eq!(title_quirks(TitlePlatform::Lgt, "00030F5B").screen_size, Some((240, 400)));
        assert_eq!(title_quirks(TitlePlatform::Ktf, "00030F5B").screen_size, None);
        assert_eq!(title_quirks(TitlePlatform::Skt, "00030F5B").screen_size, None);
    }

    /// Descriptors are not consistent about the case of an id.
    #[test]
    fn an_id_is_matched_whatever_case_it_is_written_in() {
        assert!(title_quirks(TitlePlatform::Lgt, "0002cb6a").expects_annunciator);
        assert!(title_quirks(TitlePlatform::Lgt, "0002CB6A").expects_annunciator);
    }

    /// 겟앰프드's descriptor names the handset's panel, not the area the title
    /// draws in, and everything it lays out is centred in the latter.
    #[test]
    fn a_title_can_name_a_shorter_panel_than_its_descriptor_does() {
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01031C0A").screen_size, Some((240, 296)));
        assert!(!title_quirks(TitlePlatform::Ktf, "01031C0A").expects_annunciator);

        // A panel taller than its descriptor's, and the whole of it: no strip
        // comes off the bottom.
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01035ACD").screen_size, Some((240, 320)));
        assert!(!title_quirks(TitlePlatform::Ktf, "01035ACD").expects_annunciator);
    }

    #[test]
    fn a_sideways_title_asks_for_nothing_else() {
        let quirks = title_quirks(TitlePlatform::Lgt, "000323B3");

        assert!(quirks.drawn_sideways);
        assert_eq!(quirks.screen_size, None);
        assert!(!quirks.expects_annunciator);
    }

    /// 다크슬레이어2 centres a fixed-height scene on the Canvas, so a panel sized
    /// to that scene wraps it with no black band rather than the 240x320 default.
    #[test]
    fn dark_slayer_2_asks_for_the_panel_its_scene_fills() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0049884301").screen_size, Some((240, 208)));
    }

    /// 모바일크래프트 is a 120-wide title centred on the Canvas, so it asks for the
    /// 120x160 handset it was drawn for rather than the 240x320 default.
    #[test]
    fn mobile_craft_asks_for_its_120_wide_handset() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "3507010790").screen_size, Some((120, 160)));
    }

    /// 동방사신기 sizes its clips to the far edge, so it counts it in, and it
    /// fills a 176x220 panel.
    #[test]
    fn dongbang_counts_the_clips_far_edge_in() {
        assert!(title_quirks(TitlePlatform::Skt, "0052550560").clip_includes_far_edge);
        assert_eq!(title_quirks(TitlePlatform::Skt, "0052550560").screen_size, Some((176, 220)));
    }

    /// 광개토대왕정벌기 fills a 176x216 panel.
    #[test]
    fn gwanggaeto_asks_for_its_176_wide_handset() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0047856534").screen_size, Some((176, 216)));
    }

    /// 맛대맛 lays out to a 240x240 panel.
    #[test]
    fn taste_vs_taste_asks_for_its_240x240_panel() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0051017321").screen_size, Some((240, 240)));
    }

    /// 인형뽑기타이쿤 (SK-VM, descriptor id "1") fills a 176x202 panel.
    #[test]
    fn pick_the_doll_skvm_asks_for_its_176x202_handset() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "1").screen_size, Some((176, 202)));
    }

    /// 썸머스케치 fills a 120x160 panel.
    #[test]
    fn summer_sketch_asks_for_its_120x160_handset() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0027765524").screen_size, Some((120, 160)));
    }

    /// 인형뽑기타이쿤 (KTF) keeps its 240x320 panel but crops the 24-row soft-key
    /// strip off the bottom of the frame.
    #[test]
    fn pick_the_doll_ktf_crops_its_softkey_strip() {
        assert_eq!(title_quirks(TitlePlatform::Ktf, "0102E32F").screen_size, Some((240, 320)));
        assert_eq!(title_quirks(TitlePlatform::Ktf, "0102E32F").present_crop_bottom, 24);
        assert_eq!(title_quirks(TitlePlatform::Ktf, "010366DB").present_crop_bottom, 24);
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01031E04").present_crop_bottom, 24);
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01033511").present_crop_bottom, 20);
        assert_eq!(title_quirks(TitlePlatform::Ktf, "010247AB").screen_size, Some((176, 220)));
        assert_eq!(title_quirks(TitlePlatform::Ktf, "010247AB").present_crop_bottom, 16);
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01038485").screen_size, Some((176, 220)));
        assert_eq!(title_quirks(TitlePlatform::Ktf, "01038485").present_crop_bottom, 10);
    }

    /// 프린스메이커 온달편 fills the 128x128 panel its art was drawn for.
    #[test]
    fn prince_maker_fills_its_128x128_panel() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0054980867").screen_size, Some((128, 128)));
    }

    /// 포키의 모험 fills the 176x220 panel its art was drawn for.
    #[test]
    fn poky_adventure_fills_its_176x220_panel() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0050079226").screen_size, Some((176, 220)));
    }

    /// 타워오브바벨3 fills the 176x220 panel it was drawn for.
    #[test]
    fn tower_of_babel_3_fills_its_176x220_panel() {
        assert_eq!(title_quirks(TitlePlatform::Skt, "0054563061").screen_size, Some((176, 220)));
    }

    /// An id appearing twice for one platform would make the table's answer
    /// depend on which entry came first.
    #[test]
    fn no_title_is_listed_twice() {
        for (index, (platform, aid, _)) in super::QUIRKS.iter().enumerate() {
            for (other_platform, other_aid, _) in &super::QUIRKS[index + 1..] {
                assert!(
                    !(platform == other_platform && aid.eq_ignore_ascii_case(other_aid)),
                    "{aid} is listed twice for {platform:?}"
                );
            }
        }
    }
}
