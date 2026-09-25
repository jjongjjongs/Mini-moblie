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
}

const fn panel(width: u32, height: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: Some((width, height)),
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
        clears_screen_each_paint: false,
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
}

const QUIRKS: &[(TitlePlatform, &str, TitleQuirks)] = &[
    // 미니게임 히어로즈2 터치: repaints a 240x80 sponsor banner along the
    // bottom of whatever height it is told, so its 320 rows of screen need a
    // 400-row panel underneath them.
    (TitlePlatform::Lgt, "00030F5B", panel(240, 400)),
    // 판타지나이트: without the strip its bottom 24 rows keep a stale band.
    (TitlePlatform::Lgt, "0002787C", annunciator()),
    // 프로야구 2009.
    (TitlePlatform::Lgt, "0002CB6A", annunciator()),
    // 지크2: centres a 296-row picture and then adds the strip itself.
    (TitlePlatform::Lgt, "0002A52B", annunciator()),
    // 알바타이쿤2: every screen it draws lands exactly one strip down.
    (TitlePlatform::Lgt, "0002D4D0", annunciator()),
    // 마구마구2011: the same, and it composes through its own off-screen
    // surface rather than the `MC_grp*` calls, so nothing but the strip's
    // height moves it. Told the whole 320-row panel it asks for a 240x320
    // surface, lays its scene out a strip down it, and flushes the lot: the
    // top 24 rows reach the screen black and the bottom 24 - the `CLR 메뉴`
    // and `TIME` bar under the diamond - fall off the end of the surface and
    // are not drawn at all. Told the 296 a strip leaves, it asks for 240x296
    // and fills it from its first row.
    (TitlePlatform::Lgt, "00030DD8", annunciator()),
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
    (TitlePlatform::Ktf, "0102A356", annunciator_of(16)),
    // 셔터2 데스트니: the same SDK and the same arithmetic - it clips every
    // screen to 176x204 on its 176x220 panel - and it answers the same way,
    // 57 colours down to 6 without the strip.
    (TitlePlatform::Ktf, "01037EBF", annunciator_of(16)),
    // 던전앤파이터 격투가: draws 296 rows into the 320 its descriptor asks for
    // and leaves the rest alone, so the panel is the 296 it draws.
    (TitlePlatform::Ktf, "0103BF27", panel(240, 296)),
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
    (TitlePlatform::Skt, "3826345643", clip_includes_far_edge()),
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
    // 엑스피드스노보드 (SKT MIDlet): laid out for 176x220. On the 240x320 default
    // its player/mode-select screens drew the character portrait twice - a
    // second copy sliding off the right - because the extra width left room its
    // fixed layout filled with a stray repeat. At 176x220 the portrait is single
    // and every screen fills. Keyed by DD-ProgName, not the archive filename.
    (TitlePlatform::Skt, "0052018663", panel(176, 220)),
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
];

/// What to do differently for the title `aid` on `platform`.
///
/// Returns [`TitleQuirks::default`] - which asks for nothing - for every title
/// without an entry, which is nearly all of them.
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
