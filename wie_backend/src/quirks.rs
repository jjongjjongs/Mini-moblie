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
}

const fn panel(width: u32, height: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: Some((width, height)),
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
    }
}

const fn annunciator() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: true,
        annunciator_rows: None,
        drawn_sideways: false,
        clip_includes_far_edge: false,
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
    }
}

const fn sideways() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: true,
        clip_includes_far_edge: false,
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
    // 소울게이트: takes a 240x320 screen, composes every frame into a 320x240
    // off-screen buffer of its own, and copies that onto the screen a quarter
    // turn clockwise - the handset was meant to be turned sideways to play it.
    (TitlePlatform::Lgt, "000323B3", sideways()),
    (TitlePlatform::Skt, "3826345643", clip_includes_far_edge()),
    // 사고뭉치트윈즈: clips and clears a 120x144 play area centred on the
    // Canvas and fills the rest with a tiled pattern, so on the 240x320
    // default it played in a small box in the middle of the screen. 144 rows
    // is the Canvas of a 120x160 handset, the sixteen soft-key rows under it.
    (TitlePlatform::Skt, "0054532850", panel(120, 160)),
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
