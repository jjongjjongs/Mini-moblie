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
}

const fn panel(width: u32, height: u32) -> TitleQuirks {
    TitleQuirks {
        screen_size: Some((width, height)),
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: false,
    }
}

const fn annunciator() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: true,
        annunciator_rows: None,
        drawn_sideways: false,
    }
}

const fn sideways() -> TitleQuirks {
    TitleQuirks {
        screen_size: None,
        expects_annunciator: false,
        annunciator_rows: None,
        drawn_sideways: true,
    }
}

/// Every title this runtime knows something about, and what it knows.
///
/// The reasoning behind each entry is at the function that reads it - the
/// screen size at `LgtEmulator::screen_size`, the status strip at
/// `wie_lgt`'s `title_expects_annunciator`, the quarter turn at
/// `wie_backend::present`.
///
/// No KTF title is listed for a status strip. Three were - 던전앤파이터 격투가,
/// 만귀토벌전 and 셔터2 데스트니 - and are not any more: KTF titles are shown
/// the whole panel, the way LGT titles that ask for no strip are. 만귀토벌전
/// and 셔터2 lay their screens out inside 204 of their 220 rows and leave the
/// rest holding whatever the frame before put there; that band is accepted
/// rather than papered over with a strip the handset does not show.
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
    // 겟앰프드: its descriptor says 240*320, but every full-screen picture it
    // carries - title, menu, each map - is 240x296, and it centres its popup
    // frame in whatever height the screen reports. Told 320 it put the frame at
    // `(320 - 168) / 2 = 76`, twelve rows below the text it had laid out for
    // `(296 - 168) / 2 = 64`, so a notice's title sat across the bottom of its
    // own title bar and its first line across the bottom of the message box.
    (TitlePlatform::Ktf, "01031C0A", panel(240, 296)),
    // 소울게이트: takes a 240x320 screen, composes every frame into a 320x240
    // off-screen buffer of its own, and copies that onto the screen a quarter
    // turn clockwise - the handset was meant to be turned sideways to play it.
    (TitlePlatform::Lgt, "000323B3", sideways()),
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
