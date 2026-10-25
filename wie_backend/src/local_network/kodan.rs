//! 오즈-천공의 기사단's 코단 world server, answered in process.
//!
//! 월드접속's server list (see [`super::relay`]) offers one server, and the
//! title opens a relay session naming it once the player starts. That session
//! speaks the same slot messages - `[u32be length][u32be kind][u32be
//! command][body]` - in the title's world mode (`network/d.a` = 5), whose
//! requests `network/a.c(II)[B` builds and whose answers the dispatcher
//! `network/a.b(I[BI)V` reads, switching on the kind and then the command.
//!
//! This is the world server's side of that conversation, walked one request
//! at a time from the title's own readers.
//!
//! # What the world remembers
//!
//! The character itself - level, experience, belongings - is the title's own
//! save. What the server kept is what the title asks for on every login and
//! takes as the whole truth: the quests done, which [`WORLD_STATE`] reports
//! after the title has cleared its own table, and the gold, which
//! [`COUNTERS`] states. Forgotten, every login would replay 이스한트's opening
//! scene and empty the purse, so the world keeps both per character in
//! [`PROGRESS_FILE`], among the title's own files, where a save export
//! carries it along.

use alloc::{collections::BTreeMap, vec, vec::Vec};
use core::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

use encoding_rs::EUC_KR;

use super::relay::{MESSAGE_WELCOME, SlotRelay, hex};
use crate::system::FilesystemOverlay;

/// The login every session opens with, `[0x1e]`, answered `[0x00]`.
const LOGIN: (u32, u32) = (1, 1000);
/// The client's version and character summary. Its reader (`0x1ba1a4`) takes
/// no body: it clears a flag and sends the next request, command 1020.
const VERSION: (u32, u32) = (1, 1010);
/// The character entering the world, answered with [`SESSION`].
const ENTER: (u32, u32) = (1, 1020);
/// `[len][name]`: the character's name as the world knows it, kept by
/// `network/b.a([B)V` and put on the player (`0x1ba18e`), whose name the
/// story's dialogue writes into its lines - so it must be the name, not
/// arbitrary bytes. Its reader (`0x1ba11a`) then asks [`SESSION_ACK`].
const SESSION: u32 = 1024;
/// The character's summary, its class first. Answered with one byte, which
/// the reader (`0x1ba0cc`) keeps on the player before asking for kind 11,
/// command 32450.
const SESSION_ACK: (u32, u32) = (1, 1026);
/// Kind 11 is the world's own bookkeeping. 32450 is answered with a `u16`
/// count of `[byte][u32 quest][byte state]` entries (`game/engine/f.a([BI)V`),
/// which the title puts in its quest table (`game/engine/f.c(SBB)V`) after
/// clearing it; its reader then asks kind 8, command 20000.
const WORLD_STATE: (u32, u32) = (11, 32450);
/// Kind 8 is the character's belongings. 20000 is read by `network/a.f([BI)V`
/// as a byte, then a count of `[bag][count][len-prefixed item]` groups, each
/// item made by `game/data/k.a([BBBI)` and put on the player by
/// `game/chars/g.aj()V` when it names an equipment slot. The reader then asks
/// kind 8, command 23000.
///
/// The player is drawn in a dungeon only when armed: `game/chars/g.D()V` picks
/// the battle sprites only when `g.J()Z` finds a weapon in slot 1 or 6, and
/// otherwise the town ones, which entering a dungeon has just released
/// (`base/e.j()V`) - leaving a shadow where the player stands. So the world
/// hands every character the weapon its class starts with
/// ([`starting_weapon`]), equipped.
const BELONGINGS: (u32, u32) = (8, 20000);
/// Three `u64`s, kept in `network/b.a`, the second the gold held; the reader
/// then asks kind 2, command 1102.
const COUNTERS: (u32, u32) = (8, 23000);
/// 네트워크 창고's 올리기, a thing of the single game's inventory carried
/// into the world, asked first for the id it is given there: the body is the
/// thing's `[kind][model]`, and the reader (`0x1c00c4`) puts the `u64` answer
/// on the thing and asks [`STORE`].
const STORE_ID: (u32, u32) = (8, 20210);
/// [`STORE_ID`]'s second half: `[u64 id][kind][model][count]` and the
/// thing's state, 28 bytes (`0x1b7cc2`). The reader (`0x1c014a`) adds the
/// things answered to the world's inventory as [`BUY`]'s do (`network/a.b([BI)I`:
/// `[count]` of `[bag][u32 len][item]`) and takes the thing out of the
/// single game's. The title (`wie_ktf`'s `oz`) has moved the thing itself
/// into the world's inventory as it sent this, so none is answered.
const STORE: (u32, u32) = (8, 20200);
/// 네트워크 창고's 내리기: the world's picked thing put into the single
/// game's inventory and out of the world's (`0x1c04cc`), reading nothing.
/// Like [`THROW_AWAY`] and [`LET_GO`] it is asked with `[0][0]`.
const TAKE_OUT: (u32, u32) = (8, 20300);
/// The world's picked thing thrown away (`0x1c003e`), reading nothing.
const THROW_AWAY: (u32, u32) = (8, 20100);
/// The world's picked thing let go of for gold (`0x1c01c6`): the reader
/// takes `[u64 gold][u64]`, the first two of [`COUNTERS`] backwards.
/// The body is `[0][0]` and names nothing; the thing is the one
/// [`HELD_FILE`] says was picked, and the player is given its price over
/// [`LET_GO_SHARE`]: all of it.
const LET_GO: (u32, u32) = (8, 20150);
const LET_GO_SHARE: u64 = 1;
/// The 코단 map (`0x1ba2c4`): `[byte][len][title][u16 here][u16 count]` and
/// that many places, `[i16 id][i8 type][len][name][i16 x][i16 y][count][i16
/// neighbour...]`, kept as `network/h` records. `here` is the place the
/// player stands at (`network/b.c`); the map (`game/d`) draws each place at
/// `(50 + x, 120 + y)` with a path to each neighbour, and the arrows move the
/// cursor along those paths. The reader then asks kind 2, command 1114.
const PLACE: (u32, u32) = (2, 1102);
/// `[count][u16 index...]`, marking those of [`PLACE`]'s records (`0x1ba6f8`);
/// the reader then asks kind 2, command 1120.
const PLACE_MARKS: (u32, u32) = (2, 1114);
/// `[len][message]`, a message shown on entering (none here); its reader
/// (`0x1ba7d4`) then puts the title into the world (`base/d.a(…, 1, 5, 100)`).
const ENTER_WORLD: (u32, u32) = (2, 1120);
/// 입장 on the 코단 map, `[u16 place]` - the cursor's (`network/b.d`).
/// Answered `[len][message][u64]` (`0x1ba87c`), the `u64` a time the title
/// keeps; a place other than the one the player stands at then asks whether
/// to travel there, [`TRAVEL`], and the one stood at goes on to
/// [`ENTER_AREA`].
const AREA: (u32, u32) = (2, 1130);
/// 정보 on the 코단 map, `[u16 place]`, answered `[u16 len][text]`
/// (`0x1ba936`), shown in the map's box.
const AREA_INFO: (u32, u32) = (2, 1140);
/// Travelling to the cursor's place, `[u16 place]`. Answered `[0][u64
/// gold][len][message]` (`0x1ba9ae`): the player now stands there and holds
/// that gold. A non-zero first byte refuses, with `[u16 len][text]`.
const TRAVEL: (u32, u32) = (2, 1150);
/// Entering the place stood at: `[0][type][u16 map]` (`0x1baba6`) puts the
/// title on map `p/nt/0/<map>`. Type 2 is a dungeon, which the title first
/// opens a party for (kind 7, command 10000); any other enters at once. A
/// non-zero first byte is a refusal with a message.
const ENTER_AREA: (u32, u32) = (2, 1160);
/// Kind 7 is a dungeon's party. 10000, asked once [`ENTER_AREA`] names a
/// dungeon, is answered `[byte]` (`0x1bd858`), which the title keeps before
/// opening the party screen and asking kind 7, command 10400.
const PARTY_OPEN: (u32, u32) = (7, 10000);
/// The party screen's contents (`network/a.h([BI)V`): `[len][title][i16][i16]
/// [count]`, at most two groups, each `[i16 id][count]` members of
/// `[len][name][byte]`.
const PARTY: (u32, u32) = (7, 10400);
/// Joining a room from [`PARTY`]'s list, `[u16 room]`. No reader takes the
/// command back: the server answers with [`ROOM`].
const JOIN_ROOM: (u32, u32) = (7, 11000);
/// The room joined (`network/a.g([BI)V`): `[i16 room][i32 leader][i32
/// player][count]`, the player's own slot filled from the player, then `count`
/// others of `[i32][len][name][byte class][i16 level]`. Only the leader
/// starts the dungeon (`0x16c76a`), so the player leads.
const ROOM: u32 = 14000;
/// The leader starting the dungeon, answered with itself (`0x1bef10`):
/// `[i16 map][i32][i32][count]`, then each member's `[i32 id]` - the
/// player's own followed by 23 bytes the title skips, another's by what it
/// looks like - after which the title loads the map.
const DUNGEON_START: (u32, u32) = (7, 14060);
/// The dungeon's map loaded (kind 9, `0x1c05aa`): no body; the title resets
/// the player for the dungeon and asks [`DUNGEON_INFO`].
const DUNGEON_LOADED: (u32, u32) = (9, 30000);
/// The dungeon's terms (`0x1bdd38`): `[3 bytes][u64 minutes][i16 count]`
/// entries the title keeps, then `[count]` members' `[i32 id][i16]`; the
/// title then asks kind 7, command 15000.
const DUNGEON_INFO: (u32, u32) = (7, 14600);
/// The time a dungeon gives, in minutes.
const DUNGEON_MINUTES: u64 = 30;
/// The 구역 ready, `[u16 map]`; its reader (`network/a.f()V`) asks
/// [`ROUND`]. The world reads the 구역's [`Ground`] then, and from there on
/// its monsters close in on the player, a step every [`POLL`].
const AREA_READY: (u32, u32) = (7, 15000);
/// A 구역's round (`0x1be1d0`): `[flag]`, `[count]` of `[i32][byte][byte]
/// [byte]` (`network/a.a(IBBB)V`), `[count]` members' `[i32 id][byte][byte]
/// [byte][count][bytes]`, then a byte that, non-zero, starts the 구역
/// (`game/engine/f.k()V`).
const ROUND: (u32, u32) = (7, 15100);
/// A step in a 구역, `[x][y][direction]`: 17000 setting off, 17050 arriving.
/// The server tells the party `[i32 id][x][y][direction]` under the same
/// command (`0x1bf8a4`), which places that member and shows it
/// (`game/chars/g.b(Z)V`). The walker is not told its own steps: told late,
/// one snaps it back to where it set off, which it reports as another
/// arrival, and the two bounce it between two tiles for good. It is placed
/// once, when its [`ROUND`] starts - until then it is a shadow.
const STEP: (u32, u32) = (7, 17000);
const STEPPED: (u32, u32) = (7, 17050);
/// The player standing in a 구역 (`game/chars/g.Y()V`), `[x][y][direction]`.
/// The title reads nothing back for it.
const STANDING: (u32, u32) = (7, 17010);
/// The player closing on a monster to strike (`0x1b7480`), `[i16][x][y]
/// [direction][2 bytes][count][count bytes][i32][target][byte]`. The title
/// reads nothing back for it.
const CLOSING_IN: (u32, u32) = (7, 17100);
/// The player striking in a 구역 (`network/a.c(II)[B`, `0x1b79b0`), built by
/// `game/chars/g.a(BBBBBBB)V`: `[i16][x][y][direction][3 bytes][target]
/// [count][skill]`, the target being the monster's index in the 구역 or -1.
/// The server tells the party what came of it through [`STRUCK`].
const STRIKE: (u32, u32) = (7, 17110);
/// A 구역's news (`network/a.k([BI)V`): `[u16 map]`; on the player's own map
/// four lists of `[count]` monster moves (`0x1c4e04`), then three lists of
/// `[count]` members' news (`0x1c3134`), `[count]` monsters struck
/// ([`KodanWorld::struck`]), and two lists of `[count]` members' skills.
const STRUCK: (u32, u32) = (7, 17200);
/// How near, in steps, the player must come before a monster closes in.
const MONSTER_SIGHT: u8 = 7;
/// The monster kinds that never move: the 결계 that bar a 구역's way on
/// (`emd/1/<kind>`, named 열병사막결계, 결계, 딥던전결계, 결계, 반란군결계).
const BARRIERS: [u8; 5] = [114, 119, 123, 128, 130];
/// A monster's health; every blow takes [`BLOW`] of it.
const MONSTER_HEALTH: i16 = 300;
/// The damage of a blow: three fell a monster.
const BLOW: i16 = 100;
/// [`ENTER_AREA`]'s type for a dungeon the title fights by its offline rules
/// (`wie_ktf`'s `runtime/oz.rs`), which is every dungeon entered from the
/// map: the server's rules for its monsters went with it. 2 would be one the
/// server runs, as a party's is.
const FOUGHT_BY_THE_TITLE: u8 = 0xff;
/// [`ENTER_AREA`]'s type for a town.
const TOWN: u8 = 1;
/// Kind 3 is life on the map. 5000, sent on arriving, is answered
/// `[byte][u16 len][text]` (`0x1baee8`): the text, kept by
/// `network/a.a([BII)I`, is shown in a popup and must not be empty - the
/// popup reads it through a null otherwise.
const ARRIVED: (u32, u32) = (3, 5000);
/// A quest taken from its giver, sent by the script op that builds `[1][0]
/// [script byte][u32 quest][rest]` (`0x143cc0`). Its reader,
/// `game/engine/h.a([BI)V`, takes `[result][step][u32 quest]` and, on success,
/// `[count][records][count][item triples]`, then marks the quest taken at
/// that step (`game/engine/f.c(SBB)V` with state 1). Until it is answered the
/// title sits on TRANSMITTING.
const QUEST_TAKEN: (u32, u32) = (11, 32000);
/// A story event's reward, asked for at its end by the script op that builds
/// `[0][3 script bytes][u32 quest][u64 experience][u64 gold][tail]`
/// (`0x143f38`). Its reader, `game/engine/h.b([BI)V`, takes
/// `[result][u32 quest]` and, on success, `[count][flag pairs][count][item
/// triples][u64 experience gained][u64 gold held][count][records][count][item
/// triples]`: the experience is added to the player's (`game/chars/g.b(J)V`),
/// the gold replaces what the player holds, and the quest is marked done
/// (`game/engine/f.c(SBB)V` with state 2). Until it is answered the title
/// sits on TRANSMITTING.
const QUEST_DONE: (u32, u32) = (11, 32100);
/// Sent after a reward that brought experience; its reader only lets the
/// title go on.
const QUEST_SYNC: (u32, u32) = (11, 32150);
/// Sent by a story event's script once its scene is over (`0x13ff88`). Its
/// reader takes no body and asks [`PLACE_AGAIN`].
const SCENE_OVER: (u32, u32) = (6, 7010);
/// Its reader (`0x1bafd4`) takes no body and asks [`PLACE`] again, which
/// walks the place's requests once more.
const PLACE_AGAIN: (u32, u32) = (3, 5100);
/// A shop's wares (`game/l.b(B)V`, `0x17169a`), body `[aisle]`: 0 the
/// weapons, 2 everything else. Its reader (`0x1bb0f4`) takes `[count]` and
/// that many `[kind][len][name][u16 count]`, the kind choosing each ware's
/// icon; with none it shows its own notice that nothing is sold. Otherwise
/// it asks [`WARE`] for the first, and the arrows page through the rest.
const SHOP: (u32, u32) = (3, 5150);
/// The armour's wares, body `[kind]` from the menu of the armour's kinds
/// (`game/o.m(I)V`, `0x18660a`): 경갑, 중갑, 천, 방패 and 반지. No reader
/// takes it; it is answered as [`SHOP`], and the wares then asked as
/// [`ARMOUR_WARE`].
const ARMOUR_SHOP: (u32, u32) = (3, 5160);
/// One ware, `[kind][index]`, asked for each the arrows reach
/// (`game/l.c(I)V`).
/// Its reader (`0x1bb082`) takes the item as [`BELONGINGS`] carries one and
/// shows it.
const WARE: (u32, u32) = (3, 5200);
/// [`WARE`] for the armour, which no reader takes either; answered as
/// [`WARE`].
const ARMOUR_WARE: (u32, u32) = (3, 5210);
/// Buying, `[kind][index][count]` (`game/l.d(I)V`). Its reader (`0x1bb70c`)
/// takes `[result][len][message]`, shown in a popup, and for result 1 the
/// belongings changed (`network/a.a([BI)I`: a count of `[bag][u32 len][item]`
/// added, `game/g.a([BBI)V`, then a count of those replacing the ones where
/// they lie, `game/g.b([BBI)V`) and `[counter][u64]`, one of [`COUNTERS`]'s.
const BUY: (u32, u32) = (3, 5220);
/// [`BUY`] for the armour; answered as [`BUY`].
const BUY_ARMOUR: (u32, u32) = (3, 5230);
/// [`BUY`]'s popups, EUC-KR: 구입하였습니다. and 골드가 부족합니다.
const BOUGHT: &[u8] = &[0xb1, 0xb8, 0xc0, 0xd4, 0xc7, 0xcf, 0xbf, 0xb4, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e];
const NOT_ENOUGH_GOLD: &[u8] = &[
    0xb0, 0xf1, 0xb5, 0xe5, 0xb0, 0xa1, 0x20, 0xba, 0xce, 0xc1, 0xb7, 0xc7, 0xd5, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e,
];
/// [`COUNTERS`]' gold.
const GOLD_COUNTER: u8 = 1;
/// The shop's 캐쉬충전, asked without a body. Its reader (`0x1bb442`) takes
/// `[count]` of `[byte][len][name][u64][u64]` offers, and the screen it opens
/// reads past an empty list; so it is offered [`NO_CASH`], for nothing.
const CASH_OFFERS: (u32, u32) = (3, 5240);
/// 서비스 종료, EUC-KR.
const NO_CASH: &[u8] = &[0xbc, 0xad, 0xba, 0xf1, 0xbd, 0xba, 0x20, 0xc1, 0xbe, 0xb7, 0xe1];
/// Two of the shop's screens bought with cash, asked without a body; their
/// readers (`0x1bb620`, and `0x1bb5de` for 가방구매) take the cash held, a
/// `u64`, and open the screen.
const CASH_HELD: (u32, u32) = (3, 5250);
const BAGS: (u32, u32) = (3, 5260);
/// A bag bought on [`BAGS`]' screen (`game/o.p(I)V`). Its reader
/// (`0x1bc76a`) takes the bags held - kept as [`BELONGINGS`]' first byte
/// sets them - and the cash left, a `u64`. There is no cash, so the bag is
/// given, up to [`MOST_BAGS`].
const BAG_BOUGHT: (u32, u32) = (3, 5270);
/// The bags a character holds: one to start with, as offline (`game/g`,
/// `0x1642b8`) - with none the belongings show no bag at all - and four at
/// most, as many as the belongings' row of bags has room for.
const FIRST_BAGS: u8 = 1;
const MOST_BAGS: u8 = 4;
/// What a shop sells, `[len][kind][model][u32 price LE][len][name][rest]`
/// each.
const WARES_FILE: &str = "itm/forshop";
/// The title's rings, of which [`WARES_FILE`] has none:
/// `[u16 len][u32 price LE][len][name][rest]` each, the ring's model its
/// place in the file and the rest's fourth byte its grade.
const RINGS_FILE: &str = "itm/11";
/// The grade of the rings sold: the least there is, as the most of the
/// other wares are of the least of theirs.
const SOLD_RING_GRADE: u8 = 1;
/// Asked from the in-game menu (`game/o.g(B)V`, `0x17e330`) without a body.
/// Its reader (`0x1bc15c`) takes a `u64`, keeps it, and opens the screen the
/// option leads to.
const MENU_COUNT: (u32, u32) = (3, 5700);
/// 스킬초기화 and 스탯초기화, confirmed from the screens [`MENU_COUNT`] and
/// [`MENU_AMOUNT`] open; the title has already reset the points itself.
/// Their reader (`0x1bc10e`) takes the cash left, a `u64`, and goes back to
/// the screen.
const SKILLS_RESET: (u32, u32) = (3, 5710);
const STATS_RESET: (u32, u32) = (3, 5910);
/// The character's points as spent on leaving its screen, `[0][count]...`.
/// Its reader (`0x1bb830`) takes no body and lets the title go on.
const POINTS_SPENT: (u32, u32) = (3, 5110);
/// The channels, asked with the 1 key in a dungeon (`game/k.a(I)V`,
/// `0x1703c8`) without a body. Its reader (`0x1bad4c`) takes `[count]` and
/// that many three-byte records, then lets the title go on.
const CHANNELS: (u32, u32) = (3, 5010);
/// Asked from the in-game menu (`game/o.g(B)V` with 4, `0x17e1ec`) without a
/// body. Its reader (`0x1bc282`) takes a `u64`, keeps it, and opens the screen
/// the option leads to.
const MENU_AMOUNT: (u32, u32) = (3, 5900);
/// The quests and their states again, asked without a body; read as
/// [`WORLD_STATE`] is (`0x1c0d5c`), with nothing asked after.
const QUESTS: (u32, u32) = (11, 32300);
/// One of [`QUESTS`] chosen, `[u32 quest][byte]`. Its reader
/// (`game/a.a([BI)V`) takes `[len][title][u16 len][text][state]`, then for
/// state 0 two `u32`s more; the title and the text are EUC-KR. The world
/// tells them from [`QUESTS_TOLD`].
const QUEST_TOLD: (u32, u32) = (11, 32400);
/// 퀘스트포기 in [`QUEST_TOLD`]'s window, `[u32 quest]...` (`game/c.e()V`).
/// Its reader (`0x1c0ea8`) takes no body and closes the window; the quest is
/// forgotten, to be taken again.
const QUEST_DROPPED: (u32, u32) = (11, 32200);
/// Each quest's title and what its giver says on giving it, as the 코단
/// maps' scripts (`p/nt/0`) have them: the title from the line the title
/// shows on taking the quest (`x<title>y 퀘스트 입수!`), paired with the
/// quest by the order of the quests the map's script marks (op `0x3f`), and
/// `#NAME` for the character's name.
const QUESTS_TOLD: &[(u32, &str, &str)] = &[
    (1, "불길한 징조", "모르피스: 이스한트의 수비대장 하스펠이다. 잊지 않도록."),
    (2, "진퇴양난", "아비게일: 실례군요. 제가 무슨 어린앤줄 아십니까."),
    (3, "이방인의 증명", "쿱: 열병사막에서 사막 선인장 30마리를 처치하는 일이다."),
    (4, "덜렁이 카야", "카야: 열병사막에서 두더지 모피 20개! #NAME씨, 화이팅~"),
    (5, "사막의 전란", "모르피스: 이런데서 꾸물거릴 시간이 없다."),
    (6, "전언", "모르피스: 아무래도 수정 동굴쪽 일이 신경쓰이는군."),
    (7, "신뢰의 조건", "쿱: 이번에는 열병사막에서 모래망령 30마리를 처치하는 일이다!"),
    (8, "실종된 하스펠", "아비게일: #NAME씨? 괜찮으십니까? 이런... 굉장한 부상이로군요."),
    (9, "안전지대 확보", "쿱: 잊지마라, 루비동굴에 있는 기형박쥐 10마리를 처치하는 일이다."),
    (10, "카야, 대위기!", "카야: 자, 이 사랑스런 카야를 위해 부지런히 봉사해주세요! 냐하하핫-"),
    (11, "엘메스터 급행", "아비게일: 정말이지, #NAME씨는 늘 이런 식인겁니까."),
    (12, "기도하는 자", "레이나: 그럼, 힘드시겠지만 꼭 부탁드려요."),
    (13, "망자의 귀향", "레이나: 엘메스터에 계시는 시님을 꼭 만나봐 주세요."),
    (14, "견본전달", "카야: 우우우, 연구재료까지 유디트님께 넘겨야 한다는게 왠지 분해요."),
    (15, "히르켄 정찰", "발두인: 결과는 어떠한가?"),
    (16, "동요의 원인", "시: 히르켄 산맥의 레드 서큐버스 30마리만 제거해주게."),
    (17, "불꽃의 잔당", "유디트: 아앙- 힘내라구?!"),
    (18, "순례자의 흔적", "흑기사: 딥던젼의 헤츨링들만 확인하면 정답이 나오겠지."),
    (19, "심상재현", "흑기사: 비홀더 시신경을 30개만 모아와라. 조각 연결정도는 가능하겠지."),
    (20, "숨겨진 의도", "시: 일단 수비방진 해체까지는 어떻게든 해볼 수 있게 되었군."),
    (21, "발두인의 호출", "흑기사: 자넨 지금 여기서 나와 노닥거릴 시간이 없을텐데?"),
    (22, "유디트의 지적", "발두인: 정규군 출진 전까지는 얼마간 시간이 있겠지."),
    (
        23,
        "성사집행자",
        "유디트: 아 참, 메테오라에 가서 뭔가 알게되거든 나중에 나한테도 좀 일러줘.",
    ),
    (24, "메테오라 결전", "시: 트야덴은 강한 자이다. 방심하지 않는게 좋아."),
    (25, "후방교란", "아비게일: 죽지 않게 알아서 잘 하세요. 상급기사 30 명이라니. 무모해."),
    (26, "간섭차단", "모르피스: 메테오라 요새의 엘프마도사 녀석들을 처리하고 와라."),
    (27, "흑기사", "모르피스: 대체 언제 버릴테냐, 그 꾸물대는 버릇은..."),
];
/// The keepalive poll, answered with itself.
const POLL: (u32, u32) = (0, 10);
/// A fallen player's choices (`game/k.a(I)V`, 부활대기중). 입구부활, rising
/// at the dungeon's entrance, is answered `[u16 map][x][y]` (`0x1bf754`):
/// the title goes to `map` if it is not there and stands the player at `x`,
/// `y`.
const REVIVE: (u32, u32) = (7, 17130);
/// 미션포기, giving the dungeon up. Its answer has no body; the title asks
/// [`GIVEN_UP`] next (`0x1c0596`).
const GIVE_UP: (u32, u32) = (9, 30100);
/// Answered without a body; the title asks [`LEAVE_DUNGEON`] (`0x1c0d80`).
const GIVEN_UP: (u32, u32) = (11, 32500);
/// Answered without a body (`0x1bee70`); the title goes back to the dungeon's
/// party rooms, [`PARTY`].
const LEAVE_DUNGEON: (u32, u32) = (7, 14400);
/// Leaving the party rooms. Answered without a body (`0x1bd960`); the title
/// asks [`BELONGINGS`] and goes back to the map.
const LEAVE_ROOMS: (u32, u32) = (7, 10600);

/// The quest state the title gives a quest taken, and one whose reward has
/// been granted.
const QUEST_IN_HAND: u8 = 1;
const QUEST_FINISHED: u8 = 2;

/// The file the world keeps its [`Progress`] in, per character:
/// `[u16 count]` of `[u64 character][u64 gold][u16 count][u32 quest][u8
/// state]...`, then what each bought: `[u16 count]` of `[u64 character][u16
/// count][kind][model]...`. A file from before things were sold ends after
/// the quests.
const PROGRESS_FILE: &str = "kodan_world";
/// What the player holds and its gold, which the title's runtime writes
/// (`wie_ktf`'s 오즈 changes) off the title's own belongings (`network/b.a`)
/// and gold (`network/b.a[1]`) before a request whenever they changed - the
/// things a dungeon fought by the title leaves, and what was worn, used and
/// thrown away, the server never hears of: `[u64 gold][u16 count]` and that
/// many [`Held`]; the id of the thing picked among them, all ones for none;
/// and each thing's state, `[u16 len][state]` in the same order - what the
/// title's tables do not make of it, which the server keeps unread. Taken on
/// the next request once the session's [`COUNTERS`] are answered, and
/// removed; before then it is what the title held before it was told.
const HELD_FILE: &str = "kodan_held";
/// The states [`HELD_FILE`] last told, `[u16 count]` of `[u64 id][u16
/// len][state]`, written as [`BELONGINGS`] is answered for the title to put
/// back on the things it makes.
const STATES_FILE: &str = "kodan_states";

/// What the world keeps for one character between sessions.
#[derive(Clone, Debug, Default, PartialEq)]
struct Progress {
    gold: u64,
    /// The quests and their states, as [`WORLD_STATE`] reports them.
    quests: Vec<(u32, u8)>,
    /// What was bought, `(kind, model)`, which [`BELONGINGS`] hands back.
    bought: Vec<(u8, u8)>,
    /// The bags bought on [`BAGS`]' screen, beyond [`FIRST_BAGS`].
    bags: u8,
    /// What the player held when [`HELD_FILE`] last told, which
    /// [`BELONGINGS`] then hands back instead of what was bought; `None`
    /// until it has told.
    held: Option<Vec<Held>>,
    /// What each thing held is beyond its [`Held`], by id, as [`HELD_FILE`]
    /// last told it; handed back in [`STATES_FILE`].
    states: BTreeMap<u64, Vec<u8>>,
}

/// A thing the player holds, as [`HELD_FILE`] has it: `[kind][model][count]
/// [slot][bag][u64 id]`, the title's own `game/data/k` fields `l`, `m`, `n`,
/// `o`, `p` and `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    kind: u8,
    model: u8,
    count: u8,
    slot: u8,
    bag: u8,
    id: u64,
}

impl Held {
    const LEN: usize = 13;

    /// The gold and the things [`HELD_FILE`] holds, or `None` when `data` is
    /// not that.
    fn parse_all(data: &[u8]) -> Option<(u64, Vec<Self>)> {
        let gold = be(data.get(..8)?);
        let count = be(data.get(8..10)?) as usize;
        let records = data.get(10..10 + count * Self::LEN)?;
        let held = records.chunks_exact(Self::LEN).map(Self::parse).collect();
        Some((gold, held))
    }

    /// The id of the thing picked, which follows the things.
    fn picked(data: &[u8]) -> Option<u64> {
        let count = be(data.get(8..10)?) as usize;
        let picked = be(data.get(10 + count * Self::LEN..10 + count * Self::LEN + 8)?);
        (picked != u64::MAX).then_some(picked)
    }

    /// Each thing's state, which follows the one picked; none when `data`
    /// does not have them all.
    fn states(data: &[u8]) -> Option<Vec<Vec<u8>>> {
        let count = be(data.get(8..10)?) as usize;
        let mut at = 10 + count * Self::LEN + 8;
        let mut states = Vec::with_capacity(count);
        for _ in 0..count {
            let len = be(data.get(at..at + 2)?) as usize;
            states.push(data.get(at + 2..at + 2 + len)?.to_vec());
            at += 2 + len;
        }
        Some(states)
    }

    fn parse(record: &[u8]) -> Self {
        Self {
            kind: record[0],
            model: record[1],
            count: record[2],
            slot: record[3],
            bag: record[4],
            id: be(&record[5..13]),
        }
    }

    fn write(&self, data: &mut Vec<u8>) {
        data.extend_from_slice(&[self.kind, self.model, self.count, self.slot, self.bag]);
        data.extend_from_slice(&self.id.to_be_bytes());
    }
}

/// The directions a step is told in.
const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 3;
const RIGHT: u8 = 4;

/// A 구역 as the title's `nt` map file has it: `[4 bytes][len][name][len]
/// [name][layers][width][height]`, a tile and then a ground byte for every
/// spot - anything but 0 barring the way - then `[4 bytes][count]` monster
/// kinds and `[count]` monsters of `[kind][x][y]`, which the title numbers in
/// that order (`game/engine/i.j()V`).
struct Ground {
    width: u8,
    height: u8,
    ground: Vec<u8>,
    monsters: Vec<Placed>,
    /// Where one comes in from outside: a dungeon's first 구역 marks it
    /// `[9][x][y]` just before its doors, `[0x18][u16 map][x][y][direction]`
    /// each.
    entrance: Option<(u8, u8)>,
}

/// A [`Ground`]'s mark of where one comes in, and of a door.
const ENTRANCE: u8 = 9;
const DOOR: u8 = 0x18;

/// A monster of a [`Ground`] and where it stands.
#[derive(Clone, Copy)]
struct Placed {
    kind: u8,
    x: u8,
    y: u8,
}

impl Ground {
    fn parse(data: &[u8]) -> Option<Self> {
        let mut at = 4;
        at += 1 + *data.get(at)? as usize;
        at += 1 + *data.get(at)? as usize;
        let [layers, width, height] = *data.get(at..at + 3)? else {
            return None;
        };
        // Only the plain one-layer maps a dungeon's 구역 are.
        if layers != 0 {
            return None;
        }
        let spots = width as usize * height as usize;
        at += 3 + spots;
        let ground = data.get(at..at + spots)?.to_vec();
        at += spots;
        if data.get(at..at + 4)? != [0; 4] {
            return None;
        }
        at += 4;
        at += 1 + *data.get(at)? as usize;
        let count = *data.get(at)? as usize;
        at += 1;
        let monsters = data
            .get(at..at + 3 * count)?
            .chunks(3)
            .map(|monster| Placed {
                kind: monster[0],
                x: monster[1],
                y: monster[2],
            })
            .collect();
        at += 3 * count;
        let entrance = data
            .get(at..)?
            .windows(4)
            .find(|mark| mark[0] == ENTRANCE && mark[1] < width && mark[2] < height && mark[3] == DOOR)
            .map(|mark| (mark[1], mark[2]));

        Some(Self {
            width,
            height,
            ground,
            monsters,
            entrance,
        })
    }

    /// Whether a monster can stand at `x`, `y`.
    fn open(&self, x: i16, y: i16) -> bool {
        (0..self.width as i16).contains(&x) && (0..self.height as i16).contains(&y) && self.ground[y as usize * self.width as usize + x as usize] == 0
    }
}

/// The 코단 world server's side of one session.
#[derive(Default)]
pub(super) struct KodanWorld {
    /// Where [`PROGRESS_FILE`] lives: the title's own files. Without one,
    /// nothing outlives the session.
    files: Option<FilesystemOverlay>,
    logged_in: bool,
    /// Whether the title has been told this session's belongings and gold,
    /// after which what [`HELD_FILE`] says is what it holds.
    synced: bool,
    /// The last id [`STORE_ID`] handed out this session.
    given_id: u64,
    /// The id of the thing [`HELD_FILE`] last said was picked.
    picked: Option<u64>,
    /// The character [`ENTER`] names: `[byte][u64 id][len][name]`
    /// (`0x1b5e68`), the id being the player's own (`game/chars/g.a()J`).
    character_id: u16,
    character_name: Vec<u8>,
    /// The whole id [`ENTER`] names, which [`PROGRESS_FILE`] is keyed by.
    character: u64,
    progress: Progress,
    /// The [`PLACES`] index the player stands at on the 코단 map.
    here: u16,
    /// The character's class, the first byte of [`SESSION_ACK`]
    /// (`game/engine/h.r`).
    class: u8,
    /// The 구역 [`AREA_READY`] last named.
    area: u16,
    /// The 구역's ground and its monsters where they stand now, read from the
    /// title's own map of it.
    ground: Option<Ground>,
    /// Where the player stands in the 구역.
    player: (u8, u8),
    /// The polls answered, which pace the monsters' blows.
    polls: u32,
    /// The health of the 구역's monsters struck so far, by index.
    monsters: BTreeMap<u8, i16>,
    /// The wares [`SHOP`] last listed, which [`WARE`] and [`BUY`] name by
    /// index.
    shelf: Vec<Ware>,
}

impl KodanWorld {
    /// A session whose progress is kept in `files`.
    pub(super) fn new(files: Option<FilesystemOverlay>) -> Self {
        Self { files, ..Self::default() }
    }

    /// The messages answering one request. A request this does not know yet is
    /// logged and left unanswered rather than dropping the session, so the
    /// title's next one can be seen.
    pub(super) fn respond(&mut self, payload: &[u8]) -> Option<Vec<Vec<u8>>> {
        const HEADER: usize = 12;

        if payload.len() < HEADER || u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize != payload.len() {
            return None;
        }
        let kind = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let command = u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]);
        let body = &payload[HEADER..];

        if self.logged_in {
            self.take_held();
        }

        let replies = match (kind, command) {
            LOGIN if body == [0x1e] => {
                self.logged_in = true;
                vec![SlotRelay::message(kind, command, &[0])]
            }
            POLL if self.logged_in => {
                let mut replies = vec![SlotRelay::message(kind, command, &[])];
                self.polls = self.polls.wrapping_add(1);
                let (moves, mut blows) = self.monsters_close_in();
                // A monster beside the player swings every other poll.
                if self.polls % 2 == 1 {
                    blows.clear();
                }
                if !moves.is_empty() || !blows.is_empty() {
                    replies.push(SlotRelay::message(STRUCK.0, STRUCK.1, &self.moved(&moves, &blows)));
                }
                replies
            }
            VERSION if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
            ENTER if self.logged_in => {
                if let [_, a, b, c, d, e, f, g, h, len, name @ ..] = body
                    && name.len() >= *len as usize
                {
                    self.character = u64::from_be_bytes([*a, *b, *c, *d, *e, *f, *g, *h]);
                    // A place's records carry an `i16` id: the low half.
                    self.character_id = self.character as u16;
                    self.character_name = name[..*len as usize].to_vec();
                    self.progress = self.load().remove(&self.character).unwrap_or_default();
                    self.synced = false;
                }
                let mut session = vec![self.character_name.len() as u8];
                session.extend_from_slice(&self.character_name);
                vec![SlotRelay::message(kind, SESSION, &session)]
            }
            SESSION_ACK if self.logged_in => {
                if let [class, ..] = body {
                    self.class = *class;
                }
                vec![SlotRelay::message(kind, command, &[1])]
            }
            WORLD_STATE | QUESTS if self.logged_in => {
                let mut state = (self.progress.quests.len() as u16).to_be_bytes().to_vec();
                for (quest, quest_state) in &self.progress.quests {
                    state.push(0);
                    state.extend_from_slice(&quest.to_be_bytes());
                    state.push(*quest_state);
                }
                vec![SlotRelay::message(kind, command, &state)]
            }
            BELONGINGS if self.logged_in => {
                self.hand_back_states();
                vec![SlotRelay::message(kind, command, &self.belongings())]
            }
            COUNTERS if self.logged_in => {
                self.synced = true;
                let mut counters = vec![0; 8];
                counters.extend_from_slice(&self.progress.gold.to_be_bytes());
                counters.extend_from_slice(&[0; 8]);
                vec![SlotRelay::message(kind, command, &counters)]
            }
            STORE_ID if self.logged_in => {
                let id = self.next_id().max(self.given_id + 1);
                self.given_id = id;
                vec![SlotRelay::message(kind, command, &id.to_be_bytes())]
            }
            STORE if self.logged_in => match *body {
                [b0, b1, b2, b3, b4, b5, b6, b7, kind_stored, model, count, ..] => {
                    let id = u64::from_be_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
                    let bag = bag_of(kind_stored);
                    let count = if bag == 0 { 1 } else { count.max(1) };
                    if let Some(held) = &mut self.progress.held
                        && held.iter().all(|thing| thing.id != id)
                    {
                        held.push(Held {
                            kind: kind_stored,
                            model,
                            count,
                            slot: NOT_WORN,
                            bag,
                            id,
                        });
                        self.save();
                    }
                    // The title has put the thing among the world's
                    // belongings itself, as it was; none are added.
                    vec![SlotRelay::message(kind, command, &[0])]
                }
                _ => {
                    tracing::warn!("코단 world: no such thing to store: {}", hex(body));
                    Vec::new()
                }
            },
            TAKE_OUT | THROW_AWAY if self.logged_in => {
                self.part_with_picked();
                vec![SlotRelay::message(kind, command, &[])]
            }
            LET_GO if self.logged_in => {
                if let Some(thing) = self.part_with_picked() {
                    let price = self.price_of(thing.kind, thing.model) as u64 * thing.count.max(1) as u64;
                    self.progress.gold += price / LET_GO_SHARE;
                    self.save();
                }
                let mut counters = self.progress.gold.to_be_bytes().to_vec();
                counters.extend_from_slice(&[0; 8]);
                vec![SlotRelay::message(kind, command, &counters)]
            }
            PLACE if self.logged_in => vec![SlotRelay::message(kind, command, &self.place())],
            PLACE_MARKS if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            ENTER_WORLD if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            AREA if self.logged_in => vec![SlotRelay::message(kind, command, &[0; 9])],
            AREA_INFO if self.logged_in => {
                let place = Self::place_named(body);
                let mut info = (place.name.len() as u16).to_be_bytes().to_vec();
                info.extend_from_slice(place.name);
                vec![SlotRelay::message(kind, command, &info)]
            }
            TRAVEL if self.logged_in => {
                if let [high, low, ..] = body
                    && (u16::from_be_bytes([*high, *low]) as usize) < PLACES.len()
                {
                    self.here = u16::from_be_bytes([*high, *low]);
                }
                let mut travel = vec![0];
                travel.extend_from_slice(&self.progress.gold.to_be_bytes());
                travel.push(0);
                vec![SlotRelay::message(kind, command, &travel)]
            }
            ARRIVED if self.logged_in => {
                let mut arrival = vec![0];
                arrival.extend_from_slice(&(MESSAGE_WELCOME.len() as u16).to_be_bytes());
                arrival.extend_from_slice(MESSAGE_WELCOME);
                vec![SlotRelay::message(kind, command, &arrival)]
            }
            QUEST_DROPPED if self.logged_in => {
                let quest = be(body.get(..4).unwrap_or(&[0; 4])) as u32;
                let before = self.progress.quests.len();
                self.progress.quests.retain(|(known, state)| *known != quest || *state != QUEST_IN_HAND);
                if self.progress.quests.len() != before {
                    self.save();
                }
                vec![SlotRelay::message(kind, command, &[])]
            }
            QUEST_TOLD if self.logged_in => {
                let quest = be(body.get(..4).unwrap_or(&[0; 4])) as u32;
                let (title, text) = QUESTS_TOLD
                    .iter()
                    .find(|(known, _, _)| *known == quest)
                    .map(|(_, title, text)| (*title, *text))
                    .unwrap_or(("?", ""));
                let state = self
                    .progress
                    .quests
                    .iter()
                    .find(|(known, _)| *known == quest)
                    .map_or(QUEST_IN_HAND, |(_, state)| *state);

                let title = EUC_KR.encode(title).0;
                let text = EUC_KR.encode(text).0;
                let mut told = vec![title.len() as u8];
                told.extend_from_slice(&title);
                let said = with_name(&text, &self.character_name);
                told.extend_from_slice(&(said.len() as u16).to_be_bytes());
                told.extend_from_slice(&said);
                told.push(state);
                vec![SlotRelay::message(kind, command, &told)]
            }
            QUEST_TAKEN if self.logged_in => match body.get(3..7) {
                Some(quest) => {
                    let quest = be(quest) as u32;
                    if !self.progress.quests.iter().any(|(known, _)| *known == quest) {
                        self.progress.quests.push((quest, QUEST_IN_HAND));
                        self.save();
                    }

                    let mut taken = vec![0, 0];
                    taken.extend_from_slice(&quest.to_be_bytes());
                    taken.extend_from_slice(&[0, 0]);
                    vec![SlotRelay::message(kind, command, &taken)]
                }
                None => {
                    tracing::warn!("코단 world: a quest request this does not read: {}", hex(body));
                    Vec::new()
                }
            },
            QUEST_DONE if self.logged_in => match body {
                [_, _, _, _, rest @ ..] if rest.len() >= 20 => {
                    let quest = be(&rest[..4]) as u32;
                    let experience = be(&rest[4..12]);
                    let gold = be(&rest[12..20]);

                    self.progress.gold = self.progress.gold.saturating_add(gold);
                    match self.progress.quests.iter_mut().find(|(known, _)| *known == quest) {
                        Some((_, state)) => *state = QUEST_FINISHED,
                        None => self.progress.quests.push((quest, QUEST_FINISHED)),
                    }
                    self.save();

                    let mut reward = vec![0];
                    reward.extend_from_slice(&quest.to_be_bytes());
                    reward.extend_from_slice(&[0, 0]);
                    reward.extend_from_slice(&experience.to_be_bytes());
                    reward.extend_from_slice(&self.progress.gold.to_be_bytes());
                    reward.extend_from_slice(&[0, 0]);
                    vec![SlotRelay::message(kind, command, &reward)]
                }
                _ => {
                    tracing::warn!("코단 world: a reward request this does not read: {}", hex(body));
                    Vec::new()
                }
            },
            PARTY_OPEN if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            JOIN_ROOM if self.logged_in => {
                let mut room = body.get(..2).unwrap_or(&[0, 0]).to_vec();
                room.extend_from_slice(&(self.character as u32).to_be_bytes());
                room.extend_from_slice(&(self.character as u32).to_be_bytes());
                room.push(0);
                vec![SlotRelay::message(kind, ROOM, &room)]
            }
            DUNGEON_START if self.logged_in => {
                let mut start = PLACES[self.here as usize].map.to_be_bytes().to_vec();
                start.extend_from_slice(&[0; 8]);
                start.push(1);
                start.extend_from_slice(&(self.character as u32).to_be_bytes());
                start.extend_from_slice(&[0; 23]);
                vec![SlotRelay::message(kind, command, &start)]
            }
            DUNGEON_LOADED if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
            DUNGEON_INFO if self.logged_in => {
                let mut info = vec![0, 0, 0];
                info.extend_from_slice(&DUNGEON_MINUTES.to_be_bytes());
                info.extend_from_slice(&[0, 0, 0]);
                vec![SlotRelay::message(kind, command, &info)]
            }
            AREA_READY if self.logged_in => {
                if let [high, low, ..] = body {
                    self.area = u16::from_be_bytes([*high, *low]);
                    self.monsters.clear();
                    self.ground = self.ground_of(self.area);
                }
                vec![SlotRelay::message(kind, command, &[])]
            }
            ROUND if self.logged_in => {
                // `[u16 map][x][y][direction]`: where the player stands.
                let mut replies = vec![SlotRelay::message(kind, command, &[1, 0, 0, 1])];
                if let [_, _, place @ ..] = body
                    && place.len() >= 3
                {
                    self.player = (place[0], place[1]);
                    replies.push(SlotRelay::message(STEPPED.0, STEPPED.1, &self.step(&place[..3])));
                }
                replies
            }
            STRIKE if self.logged_in => match body.get(8) {
                Some(&target) if target as i8 >= 0 => {
                    vec![SlotRelay::message(
                        STRUCK.0,
                        STRUCK.1,
                        &self.struck(target, body.get(10).copied().unwrap_or(0)),
                    )]
                }
                _ => {
                    tracing::warn!("코단 world: a strike this does not read: {}", hex(body));
                    Vec::new()
                }
            },
            STEP | STEPPED | STANDING if self.logged_in => {
                if let [x, y, ..] = body {
                    self.player = (*x, *y);
                }
                Vec::new()
            }
            CLOSING_IN if self.logged_in => Vec::new(),
            PARTY if self.logged_in => {
                let mut party = vec![0, 0, 0, 0, 0, 1];
                party.extend_from_slice(&self.character_id.to_be_bytes());
                party.push(1);
                party.push(self.character_name.len() as u8);
                party.extend_from_slice(&self.character_name);
                party.push(4);
                vec![SlotRelay::message(kind, command, &party)]
            }
            QUEST_SYNC | SCENE_OVER | PLACE_AGAIN if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
            CHANNELS if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            CASH_OFFERS if self.logged_in => {
                let mut offers = vec![1, 0, NO_CASH.len() as u8];
                offers.extend_from_slice(NO_CASH);
                offers.extend_from_slice(&[0; 16]);
                vec![SlotRelay::message(kind, command, &offers)]
            }
            CASH_HELD | BAGS if self.logged_in => vec![SlotRelay::message(kind, command, &[0; 8])],
            BAG_BOUGHT if self.logged_in => {
                if self.bags() < MOST_BAGS {
                    self.progress.bags += 1;
                    self.save();
                }
                let mut bags = vec![self.bags()];
                bags.extend_from_slice(&[0; 8]);
                vec![SlotRelay::message(kind, command, &bags)]
            }
            SHOP | ARMOUR_SHOP if self.logged_in => {
                let aisle = if command == ARMOUR_SHOP.1 {
                    match body.first() {
                        Some(make @ 0..=2) => Aisle::Armour(*make),
                        Some(3) => Aisle::Shields,
                        _ => Aisle::Rings,
                    }
                } else if body.first() == Some(&0) {
                    Aisle::Weapons
                } else {
                    Aisle::Rest
                };
                self.shelf = self.wares().into_iter().filter(|ware| ware.is_in(aisle)).collect();
                let mut kinds: Vec<u8> = self.shelf.iter().map(|ware| ware.kind).collect();
                kinds.dedup();
                let mut list = vec![kinds.len() as u8];
                for kind in kinds {
                    let name = kind_name(kind);
                    list.push(kind);
                    list.push(name.len() as u8);
                    list.extend_from_slice(name);
                    list.extend_from_slice(&(self.shelf.iter().filter(|ware| ware.kind == kind).count() as u16).to_be_bytes());
                }
                vec![SlotRelay::message(kind, SHOP.1, &list)]
            }
            WARE | ARMOUR_WARE if self.logged_in => {
                // The armour's carry the kind of armour first.
                let asked = if command == ARMOUR_WARE.1 { body.get(1..) } else { Some(body) };
                match asked.and_then(|asked| self.ware(asked)) {
                    Some(ware) => vec![SlotRelay::message(kind, WARE.1, &ware.record(1, 0))],
                    None => {
                        tracing::warn!("코단 world: no such ware: {}", hex(body));
                        Vec::new()
                    }
                }
            }
            BUY | BUY_ARMOUR if self.logged_in => {
                let asked = if command == BUY_ARMOUR.1 { body.get(1..).unwrap_or(&[]) } else { body };
                let ware = self.ware(asked).cloned();
                let count = asked.get(2).copied().unwrap_or(1).max(1);
                match ware {
                    Some(ware) => {
                        let price = ware.price as u64 * count as u64;
                        let mut bought = Vec::new();
                        if price > self.progress.gold {
                            bought.push(0);
                            bought.push(NOT_ENOUGH_GOLD.len() as u8);
                            bought.extend_from_slice(NOT_ENOUGH_GOLD);
                        } else {
                            self.progress.gold -= price;
                            bought.push(1);
                            bought.push(BOUGHT.len() as u8);
                            bought.extend_from_slice(BOUGHT);
                            // The things added - a piece of equipment each, the
                            // rest heaped together - and none changed.
                            let id = self.next_id();
                            let items: Vec<Vec<u8>> = if bag_of(ware.kind) == 0 {
                                (0..count).map(|index| ware.record(1, id + index as u64)).collect()
                            } else {
                                vec![ware.record(count, id)]
                            };
                            self.progress.bought.extend((0..count).map(|_| (ware.kind, ware.model)));
                            if let Some(held) = &mut self.progress.held {
                                let bag = bag_of(ware.kind);
                                let thing = |count, id| Held {
                                    kind: ware.kind,
                                    model: ware.model,
                                    count,
                                    slot: NOT_WORN,
                                    bag,
                                    id,
                                };
                                if bag == 0 {
                                    held.extend((0..count).map(|index| thing(1, id + index as u64)));
                                } else {
                                    held.push(thing(count, id));
                                }
                            }
                            bought.push(items.len() as u8);
                            for item in items {
                                bought.push(bag_of(ware.kind));
                                bought.extend_from_slice(&(item.len() as u32).to_be_bytes());
                                bought.extend_from_slice(&item);
                            }
                            bought.push(0);
                            self.save();
                            bought.push(GOLD_COUNTER);
                            bought.extend_from_slice(&self.progress.gold.to_be_bytes());
                        }
                        vec![SlotRelay::message(kind, BUY.1, &bought)]
                    }
                    None => {
                        tracing::warn!("코단 world: no such ware to buy: {}", hex(body));
                        Vec::new()
                    }
                }
            }
            POINTS_SPENT if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
            MENU_COUNT | MENU_AMOUNT | SKILLS_RESET | STATS_RESET if self.logged_in => vec![SlotRelay::message(kind, command, &[0; 8])],
            REVIVE if self.logged_in => {
                let map = PLACES[self.here as usize].map;
                let (x, y) = self.ground_of(map).and_then(|ground| ground.entrance).unwrap_or_default();
                let mut revive = map.to_be_bytes().to_vec();
                revive.extend_from_slice(&[x, y]);
                vec![SlotRelay::message(kind, command, &revive)]
            }
            GIVE_UP | GIVEN_UP | LEAVE_DUNGEON | LEAVE_ROOMS if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
            ENTER_AREA if self.logged_in => {
                let place = &PLACES[self.here as usize];
                let mut entry = vec![0, if place.dungeon { FOUGHT_BY_THE_TITLE } else { TOWN }];
                entry.extend_from_slice(&place.map.to_be_bytes());
                vec![SlotRelay::message(kind, command, &entry)]
            }
            _ => {
                tracing::warn!("코단 world: no answer yet for kind {kind} command {command}: {}", hex(body));
                Vec::new()
            }
        };

        Some(replies)
    }

    /// [`BELONGINGS`], `[bags][count]` of `[bag][count][len][item]...`: what
    /// the player held when [`HELD_FILE`] last told, each in its bag; before
    /// it has told, the class's [`starting_weapon`] equipped in the weapon
    /// slot and everything bought, the equipment with it and the rest in a
    /// bag of its own.
    fn belongings(&self) -> Vec<u8> {
        if let Some(held) = &self.progress.held {
            let mut bags: BTreeMap<u8, Vec<Vec<u8>>> = BTreeMap::new();
            for thing in held {
                let item = item_record(thing.kind, thing.model, thing.count, thing.id, thing.slot);
                bags.entry(thing.bag).or_default().push(item);
            }
            return self.belongings_in(bags.into_iter().collect());
        }

        let mut bags: [Vec<Vec<u8>>; 2] = [vec![starting_weapon(self.class)], Vec::new()];
        // The rest heaped by what they are, at the id the first was bought
        // with.
        let mut heaps: Vec<(u8, u8, u8, u64)> = Vec::new();
        for (index, (kind, model)) in self.progress.bought.iter().enumerate() {
            let id = FIRST_BOUGHT_ID + index as u64;
            if bag_of(*kind) == 0 {
                bags[0].push(item_record(*kind, *model, 1, id, NOT_WORN));
                continue;
            }
            match heaps.iter_mut().find(|heap| (heap.0, heap.1) == (*kind, *model) && heap.2 < u8::MAX) {
                Some(heap) => heap.2 += 1,
                None => heaps.push((*kind, *model, 1, id)),
            }
        }
        bags[1].extend(
            heaps
                .into_iter()
                .map(|(kind, model, count, id)| item_record(kind, model, count, id, NOT_WORN)),
        );

        self.belongings_in((0..).zip(bags).filter(|(_, items)| !items.is_empty()).collect())
    }

    /// [`BELONGINGS`] of the items in `bags`, by bag.
    fn belongings_in(&self, bags: Vec<(u8, Vec<Vec<u8>>)>) -> Vec<u8> {
        let mut belongings = vec![self.bags()];
        belongings.push(bags.len() as u8);
        for (bag, items) in bags {
            belongings.extend_from_slice(&[bag, items.len() as u8]);
            for item in items {
                belongings.push(item.len() as u8);
                belongings.extend_from_slice(&item);
            }
        }
        belongings
    }

    /// Every character's [`Progress`] in [`PROGRESS_FILE`]; none when there
    /// are no files or no such file, or it does not read.
    fn load(&self) -> BTreeMap<u64, Progress> {
        let Some(files) = &self.files else {
            return BTreeMap::new();
        };
        let Some(size) = ready(files.size(PROGRESS_FILE)).flatten() else {
            return BTreeMap::new();
        };
        let mut data = vec![0; size];
        if ready(files.read(PROGRESS_FILE, 0, size, &mut data)).flatten() != Some(size) {
            tracing::warn!("코단 world: {PROGRESS_FILE} did not read");
            return BTreeMap::new();
        }

        decode(&data).unwrap_or_else(|| {
            tracing::warn!("코단 world: {PROGRESS_FILE} does not parse: {}", hex(&data));
            BTreeMap::new()
        })
    }

    /// Writes this character's [`Progress`] into [`PROGRESS_FILE`], keeping
    /// every other character's.
    fn save(&self) {
        let Some(files) = &self.files else {
            return;
        };
        let mut all = self.load();
        all.insert(self.character, self.progress.clone());
        let data = encode(&all);

        let written = ready(async {
            files.truncate(PROGRESS_FILE, 0).await;
            files.write(PROGRESS_FILE, 0, &data).await
        });
        if written != Some(data.len()) {
            tracing::warn!("코단 world: {PROGRESS_FILE} was not written");
        }
    }

    /// [`STRUCK`] for the player's blow with `skill` on the monster `target`:
    /// it loses [`BLOW`] of its health, and falls once it has none. A
    /// monster's record
    /// (`0x1c38a6`) is `[index][i32 striker][skill][count][count bytes][i16
    /// most health][i16 health][state][i16 damage]` - state 1 a miss, 2 a hit
    /// whose damage is shown (`game/chars/j.a(BBI)V`) - then `[count]` and
    /// `[count]` of effects and whether it still stands: a fallen monster is
    /// laid down (`game/chars/m.e()V`).
    fn struck(&mut self, target: u8, skill: u8) -> Vec<u8> {
        const HIT: u8 = 2;

        let health = self.monsters.entry(target).or_insert(MONSTER_HEALTH);
        *health = (*health - BLOW).max(0);

        let mut news = self.area.to_be_bytes().to_vec();
        news.extend_from_slice(&[0; 4 + 3]);
        news.push(1);
        news.push(target);
        news.extend_from_slice(&(self.character as u32).to_be_bytes());
        news.extend_from_slice(&[skill, 0]);
        news.extend_from_slice(&MONSTER_HEALTH.to_be_bytes());
        news.extend_from_slice(&health.to_be_bytes());
        news.push(HIT);
        news.extend_from_slice(&BLOW.to_be_bytes());
        news.extend_from_slice(&[0, 0, (*health > 0) as u8]);
        news.extend_from_slice(&[0, 0]);
        news
    }

    /// The 구역 `map`'s ground, from the title's `nt/<hundreds>/<map>` file.
    /// The bags the character holds, [`BELONGINGS`]' first byte.
    fn bags(&self) -> u8 {
        FIRST_BAGS.saturating_add(self.progress.bags).min(MOST_BAGS)
    }

    /// Takes [`HELD_FILE`] if the title wrote it.
    fn take_held(&mut self) {
        let Some(data) = self.file(HELD_FILE) else {
            return;
        };
        if let Some(files) = &self.files {
            ready(files.remove(HELD_FILE));
        }
        self.hold(&data);
    }

    /// What [`HELD_FILE`]'s `data` says the player holds and its gold, kept
    /// once the title has been told this session's.
    fn hold(&mut self, data: &[u8]) {
        if !self.synced {
            return;
        }
        let Some((gold, held)) = Held::parse_all(data) else {
            tracing::warn!("코단 world: {HELD_FILE} does not parse: {}", hex(data));
            return;
        };
        self.picked = Held::picked(data);
        let states: BTreeMap<u64, Vec<u8>> = Held::states(data)
            .map(|states| {
                held.iter()
                    .zip(states)
                    .filter(|(_, state)| !state.is_empty())
                    .map(|(thing, state)| (thing.id, state))
                    .collect()
            })
            .unwrap_or_default();
        if self.progress.gold != gold || self.progress.held.as_ref() != Some(&held) || self.progress.states != states {
            tracing::info!("코단 world: the player holds {} things and {gold} gold", held.len());
            self.progress.gold = gold;
            self.progress.held = Some(held);
            self.progress.states = states;
            self.save();
        }
    }

    /// Takes the thing [`HELD_FILE`] last said was picked out of what the
    /// player holds, and gives it back. The title tells what it holds anyway
    /// with its next message; this keeps the world right should none come.
    fn part_with_picked(&mut self) -> Option<Held> {
        let picked = self.picked.take()?;
        let held = self.progress.held.as_mut()?;
        let index = held.iter().position(|thing| thing.id == picked)?;
        let thing = held.remove(index);
        self.progress.states.remove(&thing.id);
        self.save();
        Some(thing)
    }

    /// What the thing `kind`, `model` is bought for: the `u32` its
    /// `itm/<kind>` record starts with, the records `[u16 len][u32 price
    /// LE]...` one a model.
    fn price_of(&self, kind: u8, model: u8) -> u32 {
        let Some(data) = self.file(&alloc::format!("itm/{kind:02}")) else {
            return 0;
        };
        let mut at = 0;
        for _ in 0..model {
            let Some(&[high, low]) = data.get(at..at + 2) else {
                return 0;
            };
            at += 2 + u16::from_be_bytes([high, low]) as usize;
        }
        match data.get(at + 2..at + 6) {
            Some(&[a, b, c, d]) => u32::from_le_bytes([a, b, c, d]),
            _ => 0,
        }
    }

    /// Writes [`STATES_FILE`] with the states of the things held.
    fn hand_back_states(&self) {
        let Some(files) = &self.files else {
            return;
        };
        let held = self.progress.held.iter().flatten();
        let states: Vec<(u64, &Vec<u8>)> = held.filter_map(|thing| Some((thing.id, self.progress.states.get(&thing.id)?))).collect();
        if states.is_empty() {
            return;
        }
        let mut data = (states.len() as u16).to_be_bytes().to_vec();
        for (id, state) in states {
            data.extend_from_slice(&id.to_be_bytes());
            data.extend_from_slice(&(state.len() as u16).to_be_bytes());
            data.extend_from_slice(state);
        }
        let written = ready(async {
            files.truncate(STATES_FILE, 0).await;
            files.write(STATES_FILE, 0, &data).await
        });
        if written != Some(data.len()) {
            tracing::warn!("코단 world: {STATES_FILE} was not written");
        }
    }

    /// The id a thing bought now is given: past every one bought and held.
    fn next_id(&self) -> u64 {
        let held = self.progress.held.iter().flatten().map(|thing| thing.id + 1).max().unwrap_or(0);
        (FIRST_BOUGHT_ID + self.progress.bought.len() as u64).max(held)
    }

    /// The ware `[kind][index]` names: the shelf's `index`th of that kind.
    fn ware(&self, asked: &[u8]) -> Option<&Ware> {
        let [kind, index, ..] = *asked else {
            return None;
        };
        self.shelf.iter().filter(|ware| ware.kind == kind).nth(index as usize)
    }

    /// Everything [`WARES_FILE`] lists, and the rings of
    /// [`SOLD_RING_GRADE`]; none of either when its file does not read.
    fn wares(&self) -> Vec<Ware> {
        let mut wares = self.file(WARES_FILE).map(|data| Ware::parse_all(&data)).unwrap_or_default();
        wares.extend(self.file(RINGS_FILE).map(|data| Ware::parse_rings(&data)).unwrap_or_default());
        wares
    }

    /// The title's file at `path`, if there is one and it reads.
    fn file(&self, path: &str) -> Option<Vec<u8>> {
        let files = self.files.as_ref()?;
        let size = ready(files.size(path)).flatten()?;
        let mut data = vec![0; size];
        (ready(files.read(path, 0, size, &mut data)).flatten() == Some(size)).then_some(data)
    }

    fn ground_of(&self, map: u16) -> Option<Ground> {
        let files = self.files.as_ref()?;
        let path = alloc::format!("nt/{}/{:03}", map / 100, map);
        let size = ready(files.size(&path)).flatten()?;
        let mut data = vec![0; size];
        if ready(files.read(&path, 0, size, &mut data)).flatten() != Some(size) {
            return None;
        }
        let ground = Ground::parse(&data);
        if ground.is_none() {
            tracing::warn!("코단 world: {path} does not parse");
        }
        ground
    }

    /// One step for every monster that sees the player and is not yet beside
    /// it, and a blow from every one that is: `[index][x][y][direction]`,
    /// from where it stands.
    fn monsters_close_in(&mut self) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
        let Some(ground) = &mut self.ground else {
            return (Vec::new(), Vec::new());
        };
        let (px, py) = self.player;
        let mut moves = Vec::new();
        let mut blows = Vec::new();
        for index in 0..ground.monsters.len() {
            let monster = ground.monsters[index];
            if BARRIERS.contains(&monster.kind) || self.monsters.get(&(index as u8)) == Some(&0) {
                continue;
            }
            let (dx, dy) = (px as i16 - monster.x as i16, py as i16 - monster.y as i16);
            let distance = dx.unsigned_abs() + dy.unsigned_abs();
            let across = (dx.signum(), 0, if dx > 0 { RIGHT } else { LEFT });
            let along = (0, dy.signum(), if dy > 0 { DOWN } else { UP });
            if distance == 1 {
                let facing = if dx != 0 { across.2 } else { along.2 };
                blows.push([index as u8, monster.x, monster.y, facing]);
                continue;
            }
            if distance == 0 || distance > MONSTER_SIGHT as u16 {
                continue;
            }

            let ways = if dx.abs() >= dy.abs() { [across, along] } else { [along, across] };
            for (step_x, step_y, direction) in ways {
                if step_x == 0 && step_y == 0 {
                    continue;
                }
                let (x, y) = (monster.x as i16 + step_x, monster.y as i16 + step_y);
                let taken = (x, y) == (px as i16, py as i16)
                    || ground
                        .monsters
                        .iter()
                        .enumerate()
                        .any(|(other, at)| (at.x as i16, at.y as i16) == (x, y) && self.monsters.get(&(other as u8)) != Some(&0));
                if taken || !ground.open(x, y) {
                    continue;
                }
                moves.push([index as u8, monster.x, monster.y, direction]);
                ground.monsters[index].x = x as u8;
                ground.monsters[index].y = y as u8;
                break;
            }
        }
        (moves, blows)
    }

    /// [`STRUCK`] with nothing but monsters stepping and striking
    /// (`0x1c4e04`): a step `[index][x][y][direction][0]` puts the monster at
    /// `x`, `y` and walks it on (`game/chars/m.c(BB)V`), a blow
    /// `[index][x][y][direction]` swings it that way (`game/chars/m.d(BB)V`
    /// with 0).
    fn moved(&self, moves: &[[u8; 4]], blows: &[[u8; 4]]) -> Vec<u8> {
        let mut news = self.area.to_be_bytes().to_vec();
        news.push(moves.len() as u8);
        for step in moves {
            news.extend_from_slice(step);
            news.push(0);
        }
        news.push(blows.len() as u8);
        for blow in blows {
            news.extend_from_slice(blow);
        }
        news.extend_from_slice(&[0; 2 + 3 + 1 + 2]);
        news
    }

    /// A [`STEP`] told back: the player's id before `[x][y][direction]`.
    fn step(&self, place: &[u8]) -> Vec<u8> {
        let mut step = (self.character as u32).to_be_bytes().to_vec();
        step.extend_from_slice(place);
        step
    }

    /// [`PLACE`]: the 코단 map, with the player at [`Self::here`].
    fn place(&self) -> Vec<u8> {
        let mut place = vec![0, 0];
        place.extend_from_slice(&self.here.to_be_bytes());
        place.extend_from_slice(&(PLACES.len() as u16).to_be_bytes());
        for (index, entry) in PLACES.iter().enumerate() {
            place.extend_from_slice(&(index as u16).to_be_bytes());
            place.push(entry.dungeon as u8);
            place.push(entry.name.len() as u8);
            place.extend_from_slice(entry.name);
            place.extend_from_slice(&entry.x.to_be_bytes());
            place.extend_from_slice(&entry.y.to_be_bytes());

            let neighbours = PATHS
                .iter()
                .filter_map(|(a, b)| match index as u16 {
                    i if i == *a => Some(*b),
                    i if i == *b => Some(*a),
                    _ => None,
                })
                .collect::<Vec<u16>>();
            place.push(neighbours.len() as u8);
            for neighbour in neighbours {
                place.extend_from_slice(&neighbour.to_be_bytes());
            }
        }
        place
    }

    /// The place a `[u16 place]` request names, or the first when it names
    /// none.
    fn place_named(body: &[u8]) -> &'static Place {
        match body {
            [high, low, ..] => PLACES.get(u16::from_be_bytes([*high, *low]) as usize).unwrap_or(&PLACES[0]),
            _ => &PLACES[0],
        }
    }
}

/// One place on the 코단 map.
struct Place {
    /// EUC-KR.
    name: &'static [u8],
    /// The `p/nt/0` map entering lands on: a town's square, a dungeon's first
    /// 구역.
    map: u16,
    dungeon: bool,
    x: i16,
    y: i16,
}

/// The 코단 map's places, the towns and the dungeons the title carries maps
/// for, laid out over the continent the map shows. The first is where a
/// session starts.
const PLACES: &[Place] = &[
    // 이스한트, 중앙
    Place {
        name: &[0xc0, 0xcc, 0xbd, 0xba, 0xc7, 0xd1, 0xc6, 0xae],
        map: 1,
        dungeon: false,
        x: 45,
        y: 55,
    },
    // 엘메스터, 성기사의 광장
    Place {
        name: &[0xbf, 0xa4, 0xb8, 0xde, 0xbd, 0xba, 0xc5, 0xcd],
        map: 6,
        dungeon: false,
        x: 110,
        y: 40,
    },
    // 수련자의사막
    Place {
        name: &[0xbc, 0xf6, 0xb7, 0xc3, 0xc0, 0xda, 0xc0, 0xc7, 0xbb, 0xe7, 0xb8, 0xb7],
        map: 72,
        dungeon: true,
        x: 15,
        y: 80,
    },
    // 수행의동굴
    Place {
        name: &[0xbc, 0xf6, 0xc7, 0xe0, 0xc0, 0xc7, 0xb5, 0xbf, 0xb1, 0xbc],
        map: 76,
        dungeon: true,
        x: 60,
        y: 92,
    },
    // 과거의폐허
    Place {
        name: &[0xb0, 0xfa, 0xb0, 0xc5, 0xc0, 0xc7, 0xc6, 0xf3, 0xc7, 0xe3],
        map: 80,
        dungeon: true,
        x: 25,
        y: 20,
    },
    // 열병사막
    Place {
        name: &[0xbf, 0xad, 0xba, 0xb4, 0xbb, 0xe7, 0xb8, 0xb7],
        map: 11,
        dungeon: true,
        x: 5,
        y: 48,
    },
    // 루비동굴
    Place {
        name: &[0xb7, 0xe7, 0xba, 0xf1, 0xb5, 0xbf, 0xb1, 0xbc],
        map: 19,
        dungeon: true,
        x: 88,
        y: 78,
    },
    // 히르켄산맥
    Place {
        name: &[0xc8, 0xf7, 0xb8, 0xa3, 0xc4, 0xcb, 0xbb, 0xea, 0xb8, 0xc6],
        map: 40,
        dungeon: true,
        x: 72,
        y: 10,
    },
    // 딥던전
    Place {
        name: &[0xb5, 0xf6, 0xb4, 0xf8, 0xc0, 0xfc],
        map: 29,
        dungeon: true,
        x: 135,
        y: 88,
    },
    // 메테오라 요새
    Place {
        name: &[0xb8, 0xde, 0xc5, 0xd7, 0xbf, 0xc0, 0xb6, 0xf3, 0x20, 0xbf, 0xe4, 0xbb, 0xf5],
        map: 54,
        dungeon: true,
        x: 140,
        y: 12,
    },
];

/// The paths between [`PLACES`], by index: 이스한트 to the beginners'
/// dungeons and to 엘메스터, 엘메스터 to the harder ones.
const PATHS: &[(u16, u16)] = &[
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (0, 5),
    (2, 5),
    (4, 5),
    (1, 6),
    (1, 7),
    (1, 8),
    (1, 9),
    (3, 6),
    (4, 7),
    (6, 8),
    (7, 9),
];

/// A big-endian number of up to eight bytes.
fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |value, byte| (value << 8) | *byte as u64)
}

/// [`PROGRESS_FILE`]'s contents for `all`.
fn encode(all: &BTreeMap<u64, Progress>) -> Vec<u8> {
    let mut data = (all.len() as u16).to_be_bytes().to_vec();
    for (character, progress) in all {
        data.extend_from_slice(&character.to_be_bytes());
        data.extend_from_slice(&progress.gold.to_be_bytes());
        data.extend_from_slice(&(progress.quests.len() as u16).to_be_bytes());
        for (quest, state) in &progress.quests {
            data.extend_from_slice(&quest.to_be_bytes());
            data.push(*state);
        }
    }
    data.extend_from_slice(&(all.len() as u16).to_be_bytes());
    for (character, progress) in all {
        data.extend_from_slice(&character.to_be_bytes());
        data.extend_from_slice(&(progress.bought.len() as u16).to_be_bytes());
        for (kind, model) in &progress.bought {
            data.extend_from_slice(&[*kind, *model]);
        }
    }
    data.extend_from_slice(&(all.len() as u16).to_be_bytes());
    for (character, progress) in all {
        data.extend_from_slice(&character.to_be_bytes());
        data.push(progress.bags);
    }
    let held: Vec<(&u64, &Vec<Held>)> = all
        .iter()
        .filter_map(|(character, progress)| Some((character, progress.held.as_ref()?)))
        .collect();
    data.extend_from_slice(&(held.len() as u16).to_be_bytes());
    for (character, things) in held {
        data.extend_from_slice(&character.to_be_bytes());
        data.extend_from_slice(&(things.len() as u16).to_be_bytes());
        for thing in things {
            thing.write(&mut data);
        }
    }
    data.extend_from_slice(&(all.len() as u16).to_be_bytes());
    for (character, progress) in all {
        data.extend_from_slice(&character.to_be_bytes());
        data.extend_from_slice(&(progress.states.len() as u16).to_be_bytes());
        for (id, state) in &progress.states {
            data.extend_from_slice(&id.to_be_bytes());
            data.extend_from_slice(&(state.len() as u16).to_be_bytes());
            data.extend_from_slice(state);
        }
    }
    data
}

/// What [`encode`] wrote, or `None` when `data` is not that.
fn decode(data: &[u8]) -> Option<BTreeMap<u64, Progress>> {
    let mut cursor = data;
    let mut take_bytes = |count: usize| -> Option<&[u8]> {
        let (head, rest) = cursor.split_at_checked(count)?;
        cursor = rest;
        Some(head)
    };

    let mut all = BTreeMap::new();
    for _ in 0..be(take_bytes(2)?) {
        let character = be(take_bytes(8)?);
        let gold = be(take_bytes(8)?);
        let mut quests = Vec::new();
        for _ in 0..be(take_bytes(2)?) {
            quests.push((be(take_bytes(4)?) as u32, be(take_bytes(1)?) as u8));
        }
        all.insert(
            character,
            Progress {
                gold,
                quests,
                ..Progress::default()
            },
        );
    }
    // A file from before things were sold ends here.
    let Some(characters) = take_bytes(2).map(be) else {
        return Some(all);
    };
    for _ in 0..characters {
        let character = be(take_bytes(8)?);
        let mut bought = Vec::new();
        for _ in 0..be(take_bytes(2)?) {
            bought.push((be(take_bytes(1)?) as u8, be(take_bytes(1)?) as u8));
        }
        all.entry(character).or_default().bought = bought;
    }
    // A file from before bags were bought ends here.
    let Some(characters) = take_bytes(2).map(be) else {
        return Some(all);
    };
    for _ in 0..characters {
        let character = be(take_bytes(8)?);
        all.entry(character).or_default().bags = be(take_bytes(1)?) as u8;
    }
    // A file from before what was held was told ends here.
    let Some(characters) = take_bytes(2).map(be) else {
        return Some(all);
    };
    for _ in 0..characters {
        let character = be(take_bytes(8)?);
        let mut held = Vec::new();
        for _ in 0..be(take_bytes(2)?) {
            held.push(Held {
                kind: be(take_bytes(1)?) as u8,
                model: be(take_bytes(1)?) as u8,
                count: be(take_bytes(1)?) as u8,
                slot: be(take_bytes(1)?) as u8,
                bag: be(take_bytes(1)?) as u8,
                id: be(take_bytes(8)?),
            });
        }
        all.entry(character).or_default().held = Some(held);
    }
    // A file from before what each thing held is ends here.
    let Some(characters) = take_bytes(2).map(be) else {
        return Some(all);
    };
    for _ in 0..characters {
        let character = be(take_bytes(8)?);
        let mut states = BTreeMap::new();
        for _ in 0..be(take_bytes(2)?) {
            let id = be(take_bytes(8)?);
            let len = be(take_bytes(2)?) as usize;
            states.insert(id, take_bytes(len)?.to_vec());
        }
        all.entry(character).or_default().states = states;
    }
    Some(all)
}

/// Runs `future` to its end when it needs no waiting. The title's files sit
/// behind async calls, but every backend answers them at once from the disk;
/// a connection's `write` is not async and cannot wait, so a file call that
/// would have to is skipped rather than blocked on.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => Some(output),
        Poll::Pending => None,
    }
}

/// A thing a shop sells, as [`WARES_FILE`] has it.
#[derive(Clone, Debug, PartialEq)]
struct Ware {
    kind: u8,
    model: u8,
    price: u32,
    name: Vec<u8>,
    /// For a piece of armour, what it is made of: 0 leather, 1 metal, 2
    /// cloth.
    make: Option<u8>,
    /// For a ring, what it gives.
    ring: Option<Ring>,
}

/// What a ring (`game/data/c`, a `game/data/j`) gives that its record has to
/// say: `j`'s five abilities (`j.c`), which [`RINGS_FILE`] tells after the
/// name as `[least][most]` pairs six bytes in. The title rolls each between
/// the two when it makes a ring from its tables, but a record's abilities
/// (`j.a([BIZ)I`) are put over the roll; the ring sold has the most of each.
/// Its own `a`, `b` and `a` the record does not hold, and stay the tables'.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ring {
    abilities: [u8; 5],
}

impl Ring {
    fn parse(rest: &[u8]) -> Option<Self> {
        let pairs = rest.get(6..16)?;
        let most = |index: usize| pairs[index * 2].max(pairs[index * 2 + 1]);
        Some(Self {
            abilities: [most(0), most(1), most(2), most(3), most(4)],
        })
    }
}

/// Where in a shop a [`Ware`] is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Aisle {
    Weapons,
    /// Armour worn on the body, by [`Ware::make`].
    Armour(u8),
    Shields,
    Rings,
    Rest,
}

impl Ware {
    fn parse_all(data: &[u8]) -> Vec<Self> {
        let mut wares = Vec::new();
        let mut at = 0;
        while let Some(&len) = data.get(at) {
            let Some(record) = data.get(at + 1..at + 1 + len as usize) else {
                break;
            };
            if let Some(ware) = Self::parse(record) {
                wares.push(ware);
            }
            at += 1 + len as usize;
        }
        wares
    }

    /// The rings of [`SOLD_RING_GRADE`] in [`RINGS_FILE`].
    fn parse_rings(data: &[u8]) -> Vec<Self> {
        const RING: u8 = 11;

        let mut rings = Vec::new();
        let mut at = 0;
        let mut model = 0;
        while let Some(&[high, low]) = data.get(at..at + 2) {
            let len = u16::from_be_bytes([high, low]) as usize;
            let Some(record) = data.get(at + 2..at + 2 + len) else {
                break;
            };
            if let [a, b, c, d, name_len, ..] = *record {
                let name = record.get(5..5 + name_len as usize);
                let rest = record.get(5 + name_len as usize..).unwrap_or_default();
                if let (Some(name), Some(&SOLD_RING_GRADE)) = (name, rest.get(3)) {
                    rings.push(Self {
                        kind: RING,
                        model,
                        price: u32::from_le_bytes([a, b, c, d]),
                        name: name.to_vec(),
                        make: None,
                        ring: Ring::parse(rest),
                    });
                }
            }
            at += 2 + len;
            model += 1;
        }
        rings
    }

    /// `[kind][model][u32 price LE][len][name][rest]`, the rest of a piece
    /// of armour holding what it is made of 22 bytes in.
    fn parse(record: &[u8]) -> Option<Self> {
        let [kind, model, a, b, c, d, len, ..] = *record else {
            return None;
        };
        let name = record.get(7..7 + len as usize)?.to_vec();
        let rest = &record[7 + len as usize..];
        let make = if (6..=9).contains(&kind) { rest.get(21).copied() } else { None };

        Some(Self {
            kind,
            model,
            price: u32::from_le_bytes([a, b, c, d]),
            name,
            make,
            ring: None,
        })
    }

    /// The record of `count` of this ware with the id `id`, not worn.
    fn record(&self, count: u8, id: u64) -> Vec<u8> {
        match &self.ring {
            Some(ring) => ring_record(self.model, id, NOT_WORN, ring),
            None => item_record(self.kind, self.model, count, id, NOT_WORN),
        }
    }

    fn is_in(&self, aisle: Aisle) -> bool {
        match aisle {
            Aisle::Weapons => self.kind <= 5,
            Aisle::Armour(make) => (6..=9).contains(&self.kind) && self.make == Some(make),
            Aisle::Shields => self.kind == 10,
            Aisle::Rings => self.kind == 11,
            Aisle::Rest => self.kind >= 12,
        }
    }
}

/// `text` with each `#NAME` in it the character's `name`.
fn with_name(text: &[u8], name: &[u8]) -> Vec<u8> {
    const MARK: &[u8] = b"#NAME";

    let mut said = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.windows(MARK.len()).position(|window| window == MARK) {
        said.extend_from_slice(&rest[..at]);
        said.extend_from_slice(name);
        rest = &rest[at + MARK.len()..];
    }
    said.extend_from_slice(rest);
    said
}

/// The bag a thing of `kind` is kept in: equipment in the first, the rest in
/// the second.
fn bag_of(kind: u8) -> u8 {
    if kind <= 11 { 0 } else { 1 }
}

/// The ids things bought are given, by the order they were bought in; the
/// starting weapon is 1.
const FIRST_BOUGHT_ID: u64 = 2;

/// What a shop calls the wares of `kind`, EUC-KR.
fn kind_name(kind: u8) -> &'static [u8] {
    match kind {
        0 => &[0xb0, 0xcb],                            // 검
        1 => &[0xb5, 0xb5, 0xb3, 0xa2],                // 도끼
        2 => &[0xb5, 0xd0, 0xb1, 0xe2],                // 둔기
        3 => &[0xc3, 0xd1],                            // 총
        4 => &[0xb4, 0xdc, 0xb0, 0xcb],                // 단검
        5 => &[0xc1, 0xf6, 0xc6, 0xce, 0xc0, 0xcc],    // 지팡이
        6 => &[0xc5, 0xf5, 0xb1, 0xb8],                // 투구
        7 => &[0xb0, 0xa9, 0xbf, 0xca],                // 갑옷
        8 => &[0xc7, 0xcf, 0xc0, 0xc7],                // 하의
        9 => &[0xc0, 0xe5, 0xb0, 0xa9],                // 장갑
        10 => &[0xb9, 0xe6, 0xc6, 0xd0],               // 방패
        11 => &[0xb9, 0xdd, 0xc1, 0xf6],               // 반지
        0x15 => &[0xb0, 0xe1, 0xc1, 0xa4],             // 결정
        0x18 => &[0xbb, 0xfd, 0xb8, 0xed, 0xbc, 0xf6], // 생명수
        0x19 => &[0xc6, 0xf7, 0xb5, 0xb5, 0xc1, 0xd6], // 포도주
        0x1b => &[0xbe, 0xe0, 0xc7, 0xb0],             // 약품
        _ => &[0xb1, 0xe2, 0xc5, 0xb8],                // 기타
    }
}

/// An item as [`BELONGINGS`] carries one: `count` of `kind` and `model`, worn
/// in `slot` ([`NOT_WORN`] for none).
///
/// An item is read in layers, each class of it adding its own fields after
/// its parent's - `game/data/k`, then for equipment `j` and `i`, and for a
/// weapon `e` (`game/data/k.a(BB)`: kinds 0 to 5 are weapons, 6 to 10
/// armour) - first the item itself and then its state. The fields past the
/// slot are left at zero but for the durabilities, and the record is padded:
/// the reader steps to the next item by the record's length, not by what it
/// read.
fn item_record(kind: u8, model: u8, count: u8, id: u64, slot: u8) -> Vec<u8> {
    let weapon = kind <= 5;
    let equipment = kind <= 10;

    // `k`: kind, model, count, the item's id, the slot it is worn in, and a
    // byte; then `j` (9), `i` (6) and `e` (2).
    let mut item = vec![kind, model, count];
    item.extend_from_slice(&id.to_be_bytes());
    item.extend_from_slice(&[slot, 0]);
    if equipment {
        item.extend_from_slice(&[0; 9 + 6]);
    }
    if weapon {
        item.extend_from_slice(&[0; 2]);
    }
    // The state: `k` (6), `j` (4), `i` (9) and `e` (4), the shorts holding the
    // durability little-endian.
    item.extend_from_slice(&[0; 6]);
    if equipment {
        item.extend_from_slice(&[0; 4]);
        item.extend_from_slice(&[0, 0, 100, 0, 0, 0, 0, 100, 0]);
    }
    if weapon {
        item.extend_from_slice(&[100, 0, 0, 0]);
    }
    item.extend_from_slice(&[0; 8]);
    item
}

/// A ring's record (`game/data/c`): `k`'s part as [`item_record`]'s, then
/// `j`'s five abilities, all `k.a([BIZ)I` reads of a ring; the rest the
/// title takes from its tables (see `wie_ktf`'s `oz`).
fn ring_record(model: u8, id: u64, slot: u8, ring: &Ring) -> Vec<u8> {
    const RING: u8 = 11;

    let mut item = vec![RING, model, 1];
    item.extend_from_slice(&id.to_be_bytes());
    item.extend_from_slice(&[slot, 0]);
    item.extend_from_slice(&ring.abilities);
    item.extend_from_slice(&[0; 8]);
    item
}

/// The weapon slot, one of the two `game/chars/g.J()Z` looks in.
const WEAPON_SLOT: u8 = 1;
/// The slot of a thing not worn, -1 as `game/data/k.<init>(BB)` leaves it.
/// Slot 0 is worn: what is in it is drawn on the character
/// (`resource/b.a(Lgame/chars/g;Lgame/chars/f;IZ)V`) as armour, which a ring
/// is not.
const NOT_WORN: u8 = 0xff;

/// The weapon a character of `class` starts with, as [`BELONGINGS`] carries an
/// item: the type `game/chars/g.aX()V` gives the class (types 0 to 5 being
/// weapons, `game/data/e`), its first model, equipped in [`WEAPON_SLOT`].
fn starting_weapon(class: u8) -> Vec<u8> {
    let kind = match class {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 4,
        _ => 5,
    };
    item_record(kind, 0, 1, 1, WEAPON_SLOT)
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{DOOR, DOWN, FOUGHT_BY_THE_TITLE, GIVE_UP, GIVEN_UP, Ground, KodanWorld, LEAVE_DUNGEON, LEAVE_ROOMS, LEFT, RIGHT, SlotRelay};

    fn ask(world: &mut KodanWorld, kind: u32, command: u32, body: &[u8]) -> Vec<Vec<u8>> {
        world.respond(&SlotRelay::message(kind, command, body)).unwrap()
    }

    fn header(message: &[u8]) -> (u32, u32) {
        (
            u32::from_be_bytes([message[4], message[5], message[6], message[7]]),
            u32::from_be_bytes([message[8], message[9], message[10], message[11]]),
        )
    }

    /// The walk the title makes from 시작하기 to standing in 이스한트 is answered
    /// at every step, each answer under the kind and command its reader takes.
    #[test]
    fn the_walk_into_the_world_is_answered() {
        let mut world = KodanWorld::default();

        let steps: &[((u32, u32), &[u8], (u32, u32))] = &[
            ((1, 1000), &[0x1e], (1, 1000)),
            ((1, 1010), &[5, b'1', b'.', b'0', b'.', b'0', 0, 0], (1, 1010)),
            ((1, 1020), &[0, 0, 0, 0, 0, 0, 1, 0x0d, 0x5b, 5, 0x20, 0xba, 0xc0, 0xc7, 0xfc], (1, 1024)),
            ((1, 1026), &[4], (1, 1026)),
            ((11, 32450), &[], (11, 32450)),
            ((8, 20000), &[], (8, 20000)),
            ((8, 23000), &[0], (8, 23000)),
            ((2, 1102), &[], (2, 1102)),
            ((2, 1114), &[], (2, 1114)),
            ((2, 1120), &[], (2, 1120)),
            ((2, 1130), &[0, 0], (2, 1130)),
            ((2, 1160), &[], (2, 1160)),
            ((3, 5000), &[], (3, 5000)),
        ];
        for ((kind, command), body, answer) in steps {
            let replies = ask(&mut world, *kind, *command, body);
            assert_eq!(replies.len(), 1, "{kind}/{command}");
            assert_eq!(header(&replies[0]), *answer, "{kind}/{command}");
        }
    }

    /// A character comes armed with its class's weapon, worn in the weapon
    /// slot, so the battle sprites it is drawn with in a dungeon are loaded.
    #[test]
    fn a_character_is_armed_for_its_class() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 1, 1026, &[4, 0, 0x0b]);

        let belongings = &ask(&mut world, 8, 20000, &[])[0][12..];
        // One bag held; a group of one item.
        assert_eq!(&belongings[..4], &[1, 1, 0, 1]);
        let item = &belongings[5..];
        assert_eq!(item.len(), belongings[4] as usize);
        // Class 4 starts with type 5, model 0, worn in slot 1.
        assert_eq!(&item[..3], &[5, 0, 1]);
        assert_eq!(item[11], 1);
    }

    /// The session answer is the character's name, which the dialogue uses.
    #[test]
    fn the_session_carries_the_name() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        let session = &ask(&mut world, 1, 1020, &[0, 0, 0, 0, 0, 0, 0, 0, 7, 3, b'a', b'b', b'c'])[0];
        assert_eq!(&session[12..], &[3, b'a', b'b', b'c']);
    }

    /// The 코단 map lists every place, the player at the first, each with the
    /// paths to its neighbours both ways.
    #[test]
    fn the_map_lists_the_places() {
        use super::{PATHS, PLACES};

        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        let map = &ask(&mut world, 2, 1102, &[])[0][12..];
        // No title, standing at the first place, then every place.
        assert_eq!(&map[..4], &[0, 0, 0, 0]);
        assert_eq!(u16::from_be_bytes([map[4], map[5]]) as usize, PLACES.len());

        let mut cursor = &map[6..];
        let mut paths = 0;
        for (index, place) in PLACES.iter().enumerate() {
            assert_eq!(u16::from_be_bytes([cursor[0], cursor[1]]) as usize, index);
            assert_eq!(cursor[2], place.dungeon as u8);
            let name = cursor[3] as usize;
            assert_eq!(&cursor[4..4 + name], place.name);
            cursor = &cursor[4 + name + 4..];
            let neighbours = cursor[0] as usize;
            paths += neighbours;
            cursor = &cursor[1 + 2 * neighbours..];
        }
        assert!(cursor.is_empty());
        assert_eq!(paths, 2 * PATHS.len());
    }

    /// Travelling to a dungeon and entering it lands on its first 구역, as an
    /// area the title fights by its own rules.
    #[test]
    fn a_dungeon_is_entered_to_be_fought_by_the_title() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        // 수행의동굴.
        let travel = &ask(&mut world, 2, 1150, &[0, 3])[0][12..];
        assert_eq!(travel[0], 0, "travelled");
        assert_eq!(&ask(&mut world, 2, 1160, &[])[0][12..], &[0, FOUGHT_BY_THE_TITLE, 0, 76]);
        assert_eq!(&ask(&mut world, 7, 10000, &[])[0][12..], &[0]);
    }

    /// The menus' list and count are answered empty.
    #[test]
    fn menus_are_answered_empty() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        assert_eq!(&ask(&mut world, 3, 5150, &[0])[0][12..], &[0]);
        assert_eq!(&ask(&mut world, 3, 5010, &[])[0][12..], &[0]);
        assert_eq!(&ask(&mut world, 3, 5700, &[])[0][12..], &[0; 8]);
        assert_eq!(&ask(&mut world, 3, 5900, &[])[0][12..], &[0; 8]);
        assert_eq!(&ask(&mut world, 3, 5710, &[0x14, 1])[0][12..], &[0; 8]);
        assert_eq!(&ask(&mut world, 3, 5910, &[0, 0x12])[0][12..], &[0; 8]);
        assert_eq!(&ask(&mut world, 3, 5110, &[0, 4])[0][12..], &[] as &[u8]);
        assert_eq!(ask(&mut world, 8, 20000, &[])[0][12], 1, "one bag to start with");
        assert_eq!(&ask(&mut world, 3, 5270, &[])[0][12..], &[2, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(ask(&mut world, 3, 5270, &[])[0][12], 3);
        assert_eq!(ask(&mut world, 3, 5270, &[])[0][12], 4);
        assert_eq!(ask(&mut world, 3, 5270, &[])[0][12], 4, "no more than the most");
        assert_eq!(ask(&mut world, 8, 20000, &[])[0][12], 4);
    }

    /// A fallen player rises on the dungeon's first 구역 - at its entrance when
    /// the map can be read - or gives the dungeon up, back to its rooms and
    /// out to the map.
    #[test]
    fn a_fallen_player_rises_or_gives_up() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 2, 1150, &[0, 3]);

        assert_eq!(&ask(&mut world, 7, 17130, &[])[0][12..], &[0, 76, 0, 0]);
        for (kind, command) in [GIVE_UP, GIVEN_UP, LEAVE_DUNGEON, LEAVE_ROOMS] {
            assert_eq!(ask(&mut world, kind, command, &[])[0].len(), 12, "{command} has no body");
        }
    }

    /// A first 구역's entrance is the mark just before its doors.
    #[test]
    fn a_ground_reads_its_entrance() {
        let mut map = a_ground_file();
        map.extend_from_slice(&[0, 0, 1, 9, 2, 1, DOOR, 0x49, 0, 1, 2, 1, 0]);
        assert_eq!(Ground::parse(&map).expect("parses").entrance, Some((2, 1)));
    }

    /// Joining a room makes the player its leader, and starting it lands on
    /// the dungeon's first 구역 with the player as its one member, through to
    /// the 구역's round.
    #[test]
    fn a_dungeon_starts_with_the_player_leading() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 1, 1020, &[0, 0, 0, 0, 0, 0, 1, 0x0d, 0x5b, 3, b'a', b'b', b'c']);
        ask(&mut world, 2, 1150, &[0, 3]);

        let room = &ask(&mut world, 7, 11000, &[0x0d, 0x5b])[0];
        assert_eq!(&room[8..12], &14000u32.to_be_bytes(), "answered as the room");
        let room = &room[12..];
        assert_eq!(&room[..2], &[0x0d, 0x5b]);
        assert_eq!(&room[2..6], &room[6..10], "the player leads");
        assert_eq!(room[10], 0, "no one else");

        let start = &ask(&mut world, 7, 14060, &[])[0][12..];
        assert_eq!(&start[..2], &76u16.to_be_bytes(), "수행의동굴 1구역");
        assert_eq!(start[10], 1);
        assert_eq!(&start[11..15], &room[2..6]);
        assert_eq!(start.len(), 15 + 23);

        assert_eq!(ask(&mut world, 9, 30000, &[]).len(), 1);
        let info = &ask(&mut world, 7, 14600, &[])[0][12..];
        assert_eq!(&info[3..11], &30u64.to_be_bytes());
        assert_eq!(ask(&mut world, 7, 15000, &[0, 0x4c]).len(), 1);
        let round = ask(&mut world, 7, 15100, &[0, 0x4c, 4, 4, 2]);
        assert_eq!(&round[0][12..], &[1, 0, 0, 1]);
        // The player placed where it stands.
        assert_eq!(&round[1][8..12], &17050u32.to_be_bytes());
        assert_eq!(&round[1][12..], &[0, 1, 0x0d, 0x5b, 4, 4, 2]);

        assert!(ask(&mut world, 7, 17000, &[5, 4, 4]).is_empty(), "the walker is not told its own steps");
    }

    /// A monster struck loses health on the 구역's map and falls on the third
    /// blow; a blow at no one is not answered.
    #[test]
    fn a_monster_falls_to_three_blows() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 1, 1020, &[0, 0, 0, 0, 0, 0, 1, 0x0d, 0x5b, 3, b'a', b'b', b'c']);
        ask(&mut world, 7, 15000, &[0, 0x4c]);

        let strike = [0, 0, 0x0d, 6, 2, 0xff, 0xff, 0xff, 6, 1, 9, 0];
        for health in [200i16, 100, 0] {
            let news = &ask(&mut world, 7, 17110, &strike)[0];
            assert_eq!(&news[8..12], &17200u32.to_be_bytes());
            let news = &news[12..];
            assert_eq!(&news[..9], &[0, 0x4c, 0, 0, 0, 0, 0, 0, 0], "the map, then nothing but the blow");
            assert_eq!(
                &news[9..17],
                &[1, 6, 0, 1, 0x0d, 0x5b, 9, 0],
                "monster 6, struck by the player with skill 9"
            );
            assert_eq!(&news[17..19], &300i16.to_be_bytes());
            assert_eq!(&news[19..21], &health.to_be_bytes());
            assert_eq!(&news[21..24], &[2, 0, 100], "a hit of 100");
            assert_eq!(&news[24..], &[0, 0, (health > 0) as u8, 0, 0]);
        }

        let mut astray = strike;
        astray[8] = 0xff;
        assert!(ask(&mut world, 7, 17110, &astray).is_empty());
    }

    /// A 구역 three spots wide with a wall down the middle row's left, a 결계
    /// and two monsters.
    fn a_ground_file() -> Vec<u8> {
        let mut map = vec![0, 0, 0, 0, 1, b'a', 1, b'b', 0, 4, 3];
        map.extend_from_slice(&[0; 12]);
        map.extend_from_slice(&[0, 0, 0, 0, 0x40, 0x40, 0, 0, 0, 0, 0, 0]);
        map.extend_from_slice(&[0, 0, 0, 0, 2, 119, 141, 3, 119, 3, 0, 141, 3, 2, 141, 0, 0]);
        map
    }

    fn a_ground() -> Ground {
        Ground::parse(&a_ground_file()).expect("parses")
    }

    #[test]
    fn a_ground_reads_its_walls_and_monsters() {
        let ground = a_ground();
        assert_eq!((ground.width, ground.height), (4, 3));
        assert!(!ground.open(0, 1) && !ground.open(1, 1) && ground.open(2, 1));
        assert!(!ground.open(-1, 0) && !ground.open(4, 0) && !ground.open(0, 3));
        let monsters: Vec<_> = ground.monsters.iter().map(|monster| (monster.kind, monster.x, monster.y)).collect();
        assert_eq!(monsters, [(119, 3, 0), (141, 3, 2), (141, 0, 0)]);
    }

    /// A monster that sees the player steps toward it round a wall, one beside
    /// it swings, and a 결계 stays put.
    #[test]
    fn monsters_close_in_on_the_player() {
        let mut world = KodanWorld {
            area: 0x4c,
            ground: Some(a_ground()),
            player: (2, 2),
            ..KodanWorld::default()
        };

        let (moves, blows) = world.monsters_close_in();
        assert_eq!(moves, [[2, 0, 0, RIGHT]], "across first, the wall being below");
        assert_eq!(blows, [[1, 3, 2, LEFT]]);

        let news = world.moved(&moves, &blows);
        assert_eq!(news, [0, 0x4c, 1, 2, 0, 0, RIGHT, 0, 1, 1, 3, 2, LEFT, 0, 0, 0, 0, 0, 0, 0, 0]);

        let (moves, _) = world.monsters_close_in();
        assert_eq!(moves, [[2, 1, 0, RIGHT]]);
        let (moves, _) = world.monsters_close_in();
        assert_eq!(moves, [[2, 2, 0, DOWN]]);
        let (moves, blows) = world.monsters_close_in();
        assert!(moves.is_empty(), "beside the player now");
        assert_eq!(blows.len(), 2);

        world.monsters.insert(2, 0);
        assert_eq!(world.monsters_close_in().1.len(), 1, "a fallen monster does nothing");
    }

    /// 입장 lands on the map 이스한트 is, and the arrival popup has text.
    #[test]
    fn arriving_lands_on_a_map_with_a_message() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        assert_eq!(&ask(&mut world, 2, 1160, &[])[0][12..], &[0, 1, 0, 1]);

        let arrival = &ask(&mut world, 3, 5000, &[])[0][12..];
        let len = u16::from_be_bytes([arrival[1], arrival[2]]) as usize;
        assert!(len > 0);
        assert_eq!(arrival.len(), 3 + len);
    }

    /// A story event's reward is granted: the quest it names is done, the
    /// experience is handed on and the gold is added to what is held.
    #[test]
    fn a_reward_is_granted() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        // What 이스한트's opening scene sends at its end.
        let request = [
            0x00, 0x23, 0x17, 0x01, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0x03, 0xe8, 0, 0, 0, 0, 0, 0, 0x01, 0xf4, 0x00, 0x18, 0x00, 0x0a, 0x00,
        ];
        let reward = &ask(&mut world, 11, 32100, &request)[0][12..];
        assert_eq!(&reward[..5], &[0, 0, 0, 0, 1], "granted, quest 1");
        assert_eq!(&reward[5..7], &[0, 0], "no flags, no items");
        assert_eq!(&reward[7..15], &1000u64.to_be_bytes());
        assert_eq!(&reward[15..23], &500u64.to_be_bytes());
        assert_eq!(&reward[23..], &[0, 0]);

        // A second reward's gold is added to the first.
        let reward = &ask(&mut world, 11, 32100, &request)[0][12..];
        assert_eq!(&reward[15..23], &1000u64.to_be_bytes());

        assert_eq!(ask(&mut world, 11, 32150, &[]).len(), 1);
        assert_eq!(ask(&mut world, 6, 7010, &[]).len(), 1);
        assert_eq!(ask(&mut world, 3, 5100, &[]).len(), 1);
    }

    /// A quest taken is answered for and kept as taken, until its reward
    /// finishes it.
    #[test]
    fn a_quest_is_taken() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        // What taking quest 3 sent on a handset.
        let request = [1, 0, 1, 0, 0, 0, 3, 1, 0, 0, 0, 0x74, 0, 0, 0, 0x1e, 0];
        assert_eq!(&ask(&mut world, 11, 32000, &request)[0][12..], &[0, 0, 0, 0, 0, 3, 0, 0]);
        assert_eq!(&ask(&mut world, 11, 32450, &[])[0][12..], &[0, 1, 0, 0, 0, 0, 3, 1]);

        // Taken again, it is not listed twice.
        ask(&mut world, 11, 32000, &request);
        assert_eq!(&ask(&mut world, 11, 32450, &[])[0][12..14], &[0, 1]);

        let reward = [0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        ask(&mut world, 11, 32100, &reward);
        assert_eq!(&ask(&mut world, 11, 32450, &[])[0][12..], &[0, 1, 0, 0, 0, 0, 3, 2]);
    }

    /// After a reward, the login's bookkeeping reports the quest finished and
    /// the gold held.
    #[test]
    fn the_login_reports_what_was_granted() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        assert_eq!(&ask(&mut world, 11, 32450, &[])[0][12..], &[0, 0], "nothing yet");

        let request = [
            0, 0x23, 0x17, 0x01, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0x03, 0xe8, 0, 0, 0, 0, 0, 0, 0x01, 0xf4, 0,
        ];
        ask(&mut world, 11, 32100, &request);

        assert_eq!(&ask(&mut world, 11, 32450, &[])[0][12..], &[0, 1, 0, 0, 0, 0, 1, 2]);
        assert_eq!(&ask(&mut world, 11, 32300, &[])[0][12..], &[0, 1, 0, 0, 0, 0, 1, 2]);
        let counters = &ask(&mut world, 8, 23000, &[0])[0][12..];
        assert_eq!(counters.len(), 24);
        assert_eq!(&counters[8..16], &500u64.to_be_bytes());
    }

    /// The progress file reads back what was written, and a file that is not
    /// one reads as nothing.
    #[test]
    fn progress_survives_the_file() {
        use alloc::collections::BTreeMap;

        use super::{Held, Progress, decode, encode};

        let mut all = BTreeMap::new();
        all.insert(
            0x10d5b,
            Progress {
                gold: 500,
                quests: vec![(1, 2), (7, 1)],
                bought: vec![],
                bags: 0,
                held: None,
                states: BTreeMap::new(),
            },
        );
        all.insert(3, Progress::default());
        all.insert(
            4,
            Progress {
                gold: 100,
                quests: vec![],
                bought: vec![(0, 2), (0x18, 0)],
                bags: 1,
                held: Some(vec![Held {
                    kind: 11,
                    model: 3,
                    count: 1,
                    slot: 0xff,
                    bag: 0,
                    id: 9,
                }]),
                states: BTreeMap::from([(9, vec![1, b'c', 7, 0, 0xff])]),
            },
        );
        assert_eq!(decode(&encode(&all)), Some(all));

        // A file from before things were sold.
        let old = [0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0];
        assert_eq!(decode(&old).unwrap()[&3].gold, 7);

        assert_eq!(decode(&[0, 1, 0, 0]), None);
        assert_eq!(decode(&[0, 0]), Some(BTreeMap::new()));
    }

    /// `[len][kind][model][u32 price LE][len][name][rest]`, as the shop's
    /// file has its wares.
    fn ware_record(kind: u8, model: u8, price: u32, name: &[u8], rest: &[u8]) -> Vec<u8> {
        let mut record = vec![kind, model];
        record.extend_from_slice(&price.to_le_bytes());
        record.push(name.len() as u8);
        record.extend_from_slice(name);
        record.extend_from_slice(rest);
        record.insert(0, record.len() as u8);
        record
    }

    /// The shop's file is read into wares, each in its aisle: a piece of
    /// armour by what it is made of.
    #[test]
    fn wares_are_shelved() {
        use super::{Aisle, Ware};

        let mut file = ware_record(0, 2, 800, b"sword", &[0; 36]);
        let mut cloth = [0; 28];
        cloth[21] = 2;
        file.extend(ware_record(6, 0, 3300, b"hood", &cloth));
        file.extend(ware_record(10, 0, 100, b"buckler", &[0; 28]));
        file.extend(ware_record(0x18, 0, 400, b"water", &[0; 8]));

        let wares = Ware::parse_all(&file);
        assert_eq!(wares.len(), 4);
        assert_eq!((wares[0].kind, wares[0].model, wares[0].price), (0, 2, 800));
        assert_eq!(wares[1].make, Some(2));
        assert!(wares[0].is_in(Aisle::Weapons));
        assert!(wares[1].is_in(Aisle::Armour(2)) && !wares[1].is_in(Aisle::Armour(0)));
        assert!(wares[2].is_in(Aisle::Shields));
        assert!(wares[3].is_in(Aisle::Rest));
    }

    /// The rings are read from their own file, only those of the grade sold,
    /// each the model of its place in the file.
    #[test]
    fn rings_are_shelved() {
        use super::{Aisle, Ware};

        let ring = |price: u32, name: &[u8], grade: u8| {
            let mut record = price.to_le_bytes().to_vec();
            record.push(name.len() as u8);
            record.extend_from_slice(name);
            record.extend_from_slice(&[0, 0, 20, grade, 0, 1]);
            let mut ring = (record.len() as u16).to_be_bytes().to_vec();
            ring.extend(record);
            ring
        };
        let mut file = ring(8000, b"amethyst", 1);
        file.extend(ring(16000, b"valour", 2));
        file.extend(ring(8000, b"grit", 1));

        let rings = Ware::parse_rings(&file);
        assert_eq!(rings.len(), 2);
        assert_eq!((rings[0].kind, rings[0].model, rings[0].price), (11, 0, 8000));
        assert_eq!((rings[1].model, &rings[1].name[..]), (2, &b"grit"[..]));
        assert!(rings.iter().all(|ring| ring.is_in(Aisle::Rings)));
    }

    /// A ware is named by its kind and its place among the shelf's of that
    /// kind; buying it takes its price, adds it to the belongings and keeps
    /// it, and is refused without the gold.
    #[test]
    fn a_ware_is_bought() {
        use super::Ware;

        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        let ware = |kind, model, price| Ware {
            kind,
            model,
            price,
            name: b"x".to_vec(),
            make: None,
            ring: None,
        };
        world.shelf = vec![ware(0, 2, 800), ware(1, 2, 400), ware(0, 3, 1400)];
        world.progress.gold = 1000;

        // The second sword.
        let shown = &ask(&mut world, 3, 5200, &[0, 1])[0];
        assert_eq!(&shown[8..12], &5200u32.to_be_bytes());
        assert_eq!(&shown[12..15], &[0, 3, 1]);

        let refused = &ask(&mut world, 3, 5220, &[0, 1, 1])[0][12..];
        assert_eq!(refused[0], 0);
        assert_eq!(world.progress.gold, 1000);

        let bought = &ask(&mut world, 3, 5220, &[1, 0, 1])[0][12..];
        assert_eq!(bought[0], 1);
        let added = 2 + bought[1] as usize;
        assert_eq!(&bought[added..added + 2], &[1, 0], "one thing, in the first bag");
        let len = u32::from_be_bytes(bought[added + 2..added + 6].try_into().unwrap()) as usize;
        assert_eq!(&bought[added + 6..added + 8], &[1, 2]);
        assert_eq!(&bought[added + 6 + len..], &[0, 1, 0, 0, 0, 0, 0, 0, 2, 0x58]);
        assert_eq!(world.progress.gold, 600);
        assert_eq!(world.progress.bought, vec![(1, 2)]);

        // The belongings: the weapon the class starts with, and the axe.
        let belongings = &ask(&mut world, 8, 20000, &[])[0][12..];
        assert_eq!(&belongings[..4], &[1, 1, 0, 2]);
    }

    /// What the title says the player holds is kept once the session's
    /// belongings were told, handed back as the belongings, and added to by
    /// what is bought.
    #[test]
    fn what_is_held_is_kept() {
        use super::{Held, Ware};

        let thing = |kind, model, count, slot, bag, id| Held {
            kind,
            model,
            count,
            slot,
            bag,
            id,
        };
        let held = vec![thing(5, 0, 1, 1, 0, 1), thing(11, 0, 1, 0xff, 0, 2), thing(0x18, 0, 3, 0xff, 1, 7)];
        let mut told = 1234u64.to_be_bytes().to_vec();
        told.extend_from_slice(&(held.len() as u16).to_be_bytes());
        for thing in &held {
            thing.write(&mut told);
        }
        assert_eq!(Held::parse_all(&told), Some((1234, held.clone())));
        assert_eq!(Held::parse_all(&told[..told.len() - 1]), None);

        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        // What the title held before it was told is not kept.
        world.hold(&told);
        assert_eq!(world.progress.held, None);
        ask(&mut world, 8, 23000, &[]);
        world.hold(&told);
        assert_eq!(world.progress.gold, 1234);
        assert_eq!(world.progress.held.as_ref(), Some(&held));

        // One bag, two groups: the staff worn and the ring, then the water.
        let belongings = &ask(&mut world, 8, 20000, &[])[0][12..];
        assert_eq!(&belongings[..4], &[1, 2, 0, 2]);
        let len = belongings[4] as usize;
        assert_eq!(&belongings[5..8], &[5, 0, 1]);
        assert_eq!(&belongings[8..16], &1u64.to_be_bytes());
        assert_eq!(belongings[16], 1, "worn");
        let ring = 5 + len;
        assert_eq!(belongings[ring + 1 + 11], 0xff, "not worn");
        let water = ring + 1 + belongings[ring] as usize;
        assert_eq!(&belongings[water..water + 2], &[1, 1]);
        assert_eq!(&belongings[water + 3..water + 6], &[0x18, 0, 3]);

        // A ring bought after the water is given the id past it.
        world.shelf = vec![Ware {
            kind: 11,
            model: 4,
            price: 1000,
            name: b"x".to_vec(),
            make: None,
            ring: None,
        }];
        ask(&mut world, 3, 5230, &[4, 11, 0, 1]);
        assert_eq!(world.progress.gold, 234);
        assert_eq!(world.progress.held.as_ref().unwrap().last(), Some(&thing(11, 4, 1, 0xff, 0, 8)));
    }

    /// 네트워크 창고's 올리기 gives the thing an id and adds it to the world's
    /// inventory; 내리기 takes out the thing the title says was picked.
    #[test]
    fn the_storage_takes_and_gives() {
        use super::Held;

        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 8, 23000, &[]);
        let staff = Held {
            kind: 5,
            model: 0,
            count: 1,
            slot: 1,
            bag: 0,
            id: 1,
        };
        let mut told = 0u64.to_be_bytes().to_vec();
        told.extend_from_slice(&1u16.to_be_bytes());
        staff.write(&mut told);
        world.hold(&told);

        let id = &ask(&mut world, 8, 20210, &[7, 4])[0][12..];
        assert_eq!(id, &2u64.to_be_bytes());
        assert_eq!(&ask(&mut world, 8, 20210, &[7, 4])[0][12..], &3u64.to_be_bytes(), "never the same twice");

        let mut stored = 2u64.to_be_bytes().to_vec();
        stored.extend_from_slice(&[7, 4, 1]);
        stored.resize(28, 0);
        // The title moved the armour itself; nothing is added.
        assert_eq!(&ask(&mut world, 8, 20200, &stored)[0][12..], &[0]);
        let armour = Held {
            kind: 7,
            model: 4,
            count: 1,
            slot: 0xff,
            bag: 0,
            id: 2,
        };
        assert_eq!(world.progress.held.as_deref(), Some(&[staff, armour][..]));
        // Told again with the armour among the world's, it is not held twice.
        ask(&mut world, 8, 20200, &stored);
        assert_eq!(world.progress.held.as_deref(), Some(&[staff, armour][..]));

        // Taken out with the armour picked.
        let mut told = 0u64.to_be_bytes().to_vec();
        told.extend_from_slice(&2u16.to_be_bytes());
        staff.write(&mut told);
        armour.write(&mut told);
        told.extend_from_slice(&2u64.to_be_bytes());
        told.extend_from_slice(&[0, 0, 0, 3, 1, 2, 3]);
        assert_eq!(Held::picked(&told), Some(2));
        assert_eq!(Held::states(&told), Some(vec![vec![], vec![1, 2, 3]]));
        world.hold(&told);
        assert_eq!(world.progress.states.get(&2), Some(&vec![1, 2, 3]));
        assert!(ask(&mut world, 8, 20300, &[0, 0])[0][12..].is_empty());
        assert_eq!(world.progress.held.as_deref(), Some(&[staff][..]));

        // What a thing is goes with it, and is gone with it.
        assert!(world.progress.states.is_empty());

        // Let go of with nothing picked, for nothing.
        let sold = &ask(&mut world, 8, 20150, &[0, 0])[0][12..];
        assert_eq!(sold, &[0; 16]);
        assert_eq!(world.progress.held.as_deref(), Some(&[staff][..]));
    }

    /// A quest chosen is told with its title and its giver's words, the
    /// character's name in them; one given up is forgotten.
    #[test]
    fn a_quest_is_told_and_given_up() {
        use encoding_rs::EUC_KR;

        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        world.character_name = b"oz".to_vec();
        world.progress.quests = vec![(1, 2), (8, 1)];

        let told = &ask(&mut world, 11, 32400, &[0, 0, 0, 8, 0])[0][12..];
        let title = EUC_KR.encode("실종된 하스펠").0;
        assert_eq!(told[0] as usize, title.len());
        assert_eq!(&told[1..1 + title.len()], &title[..]);
        let at = 1 + title.len();
        let len = u16::from_be_bytes([told[at], told[at + 1]]) as usize;
        let text = &told[at + 2..at + 2 + len];
        assert!(text.windows(2).any(|window| window == b"oz"));
        assert_eq!(&told[at + 2 + len..], &[1]);

        assert_eq!(&ask(&mut world, 11, 32200, &[0, 0, 0, 8, 0])[0][12..], &[] as &[u8]);
        assert_eq!(world.progress.quests, vec![(1, 2)], "a quest done stays done");
    }

    /// Nothing is answered before the login.
    #[test]
    fn nothing_before_the_login() {
        let mut world = KodanWorld::default();
        assert_eq!(ask(&mut world, 1, 1010, &[0]), vec![] as Vec<Vec<u8>>);
    }
}
