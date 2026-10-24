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
/// [`ROUND`].
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
const STRUCK: u32 = 17200;
/// A monster's health; every blow takes [`BLOW`] of it.
const MONSTER_HEALTH: i16 = 300;
/// The damage of a blow: three fell a monster.
const BLOW: i16 = 100;
/// [`ENTER_AREA`]'s type for a dungeon.
const DUNGEON: u8 = 2;
/// [`ENTER_AREA`]'s type for a town.
const TOWN: u8 = 1;
/// Kind 3 is life on the map. 5000, sent on arriving, is answered
/// `[byte][u16 len][text]` (`0x1baee8`): the text, kept by
/// `network/a.a([BII)I`, is shown in a popup and must not be empty - the
/// popup reads it through a null otherwise.
const ARRIVED: (u32, u32) = (3, 5000);
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
/// The keepalive poll, answered with itself.
const POLL: (u32, u32) = (0, 10);

/// The quest state the title gives a quest whose reward has been granted.
const QUEST_FINISHED: u8 = 2;

/// The file the world keeps its [`Progress`] in, per character:
/// `[u16 count]` of `[u64 character][u64 gold][u16 count][u32 quest][u8
/// state]...`.
const PROGRESS_FILE: &str = "kodan_world";

/// What the world keeps for one character between sessions.
#[derive(Clone, Debug, Default, PartialEq)]
struct Progress {
    gold: u64,
    /// The quests and their states, as [`WORLD_STATE`] reports them.
    quests: Vec<(u32, u8)>,
}

/// The 코단 world server's side of one session.
#[derive(Default)]
pub(super) struct KodanWorld {
    /// Where [`PROGRESS_FILE`] lives: the title's own files. Without one,
    /// nothing outlives the session.
    files: Option<FilesystemOverlay>,
    logged_in: bool,
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
    /// The health of the 구역's monsters struck so far, by index.
    monsters: BTreeMap<u8, i16>,
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

        let replies = match (kind, command) {
            LOGIN if body == [0x1e] => {
                self.logged_in = true;
                vec![SlotRelay::message(kind, command, &[0])]
            }
            POLL if self.logged_in => vec![SlotRelay::message(kind, command, &[])],
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
            WORLD_STATE if self.logged_in => {
                let mut state = (self.progress.quests.len() as u16).to_be_bytes().to_vec();
                for (quest, quest_state) in &self.progress.quests {
                    state.push(0);
                    state.extend_from_slice(&quest.to_be_bytes());
                    state.push(*quest_state);
                }
                vec![SlotRelay::message(kind, command, &state)]
            }
            BELONGINGS if self.logged_in => vec![SlotRelay::message(kind, command, &self.belongings())],
            COUNTERS if self.logged_in => {
                let mut counters = vec![0; 8];
                counters.extend_from_slice(&self.progress.gold.to_be_bytes());
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
                }
                vec![SlotRelay::message(kind, command, &[])]
            }
            ROUND if self.logged_in => {
                // `[u16 map][x][y][direction]`: where the player stands.
                let mut replies = vec![SlotRelay::message(kind, command, &[1, 0, 0, 1])];
                if let [_, _, place @ ..] = body
                    && place.len() >= 3
                {
                    replies.push(SlotRelay::message(STEPPED.0, STEPPED.1, &self.step(&place[..3])));
                }
                replies
            }
            STRIKE if self.logged_in => match body.get(8) {
                Some(&target) if target as i8 >= 0 => vec![SlotRelay::message(kind, STRUCK, &self.struck(target))],
                _ => {
                    tracing::warn!("코단 world: a strike this does not read: {}", hex(body));
                    Vec::new()
                }
            },
            STEP | STEPPED | CLOSING_IN if self.logged_in => Vec::new(),
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
            ENTER_AREA if self.logged_in => {
                let place = &PLACES[self.here as usize];
                let mut entry = vec![0, if place.dungeon { DUNGEON } else { TOWN }];
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

    /// [`BELONGINGS`]: one bag holding the class's [`starting_weapon`],
    /// equipped in the weapon slot.
    fn belongings(&self) -> Vec<u8> {
        let item = starting_weapon(self.class);
        let mut belongings = vec![0, 1, 0, 1, item.len() as u8];
        belongings.extend_from_slice(&item);
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

    /// [`STRUCK`] for the player's blow on the monster `target`: it loses
    /// [`BLOW`] of its health, and falls once it has none. A monster's record
    /// (`0x1c38a6`) is `[index][i32 striker][skill][count][count bytes][i16
    /// most health][i16 health][state][i16 damage]` - state 1 a miss, 2 a hit
    /// whose damage is shown (`game/chars/j.a(BBI)V`) - then `[count]` and
    /// `[count]` of effects and whether it still stands: a fallen monster is
    /// laid down (`game/chars/m.e()V`).
    fn struck(&mut self, target: u8) -> Vec<u8> {
        const HIT: u8 = 2;

        let health = self.monsters.entry(target).or_insert(MONSTER_HEALTH);
        *health = (*health - BLOW).max(0);

        let mut news = self.area.to_be_bytes().to_vec();
        news.extend_from_slice(&[0; 4 + 3]);
        news.push(1);
        news.push(target);
        news.extend_from_slice(&(self.character as u32).to_be_bytes());
        news.extend_from_slice(&[0, 0]);
        news.extend_from_slice(&MONSTER_HEALTH.to_be_bytes());
        news.extend_from_slice(&health.to_be_bytes());
        news.push(HIT);
        news.extend_from_slice(&BLOW.to_be_bytes());
        news.extend_from_slice(&[0, 0, (*health > 0) as u8]);
        news.extend_from_slice(&[0, 0]);
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
    data
}

/// What [`encode`] wrote, or `None` when `data` is not that.
fn decode(data: &[u8]) -> Option<BTreeMap<u64, Progress>> {
    let mut cursor = data;
    let mut take = |count: usize| -> Option<u64> {
        let (head, rest) = cursor.split_at_checked(count)?;
        cursor = rest;
        Some(be(head))
    };

    let mut all = BTreeMap::new();
    for _ in 0..take(2)? {
        let character = take(8)?;
        let gold = take(8)?;
        let mut quests = Vec::new();
        for _ in 0..take(2)? {
            quests.push((take(4)? as u32, take(1)? as u8));
        }
        all.insert(character, Progress { gold, quests });
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

/// The weapon slot, one of the two `game/chars/g.J()Z` looks in.
const WEAPON_SLOT: u8 = 1;

/// The weapon a character of `class` starts with, as [`BELONGINGS`] carries an
/// item: the type `game/chars/g.aX()V` gives the class (types 0 to 5 being
/// weapons, `game/data/e`), its first model, equipped in [`WEAPON_SLOT`].
///
/// An item is read in layers, each class of it adding its own fields after
/// its parent's - `game/data/k`, `j`, `i` and the weapon's `e` - first the
/// item itself and then its state. The fields past the slot are left at
/// zero but for the durabilities, and the record is padded: the reader steps
/// to the next item by the record's length, not by what it read.
fn starting_weapon(class: u8) -> Vec<u8> {
    let kind = match class {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 4,
        _ => 5,
    };

    // `k`: type, model, count, the item's id, the slot it is worn in, and a
    // byte; then `j` (9), `i` (6) and `e` (2).
    let mut item = vec![kind, 0, 1];
    item.extend_from_slice(&1u64.to_be_bytes());
    item.extend_from_slice(&[WEAPON_SLOT, 0]);
    item.extend_from_slice(&[0; 9 + 6 + 2]);
    // The state: `k` (6), `j` (4), `i` (9) and `e` (4), the shorts holding the
    // durability little-endian.
    item.extend_from_slice(&[0; 6 + 4]);
    item.extend_from_slice(&[0, 0, 100, 0, 0, 0, 0, 100, 0]);
    item.extend_from_slice(&[100, 0, 0, 0]);
    item.extend_from_slice(&[0; 8]);
    item
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{KodanWorld, SlotRelay};

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
        // One bag of one item.
        assert_eq!(&belongings[..4], &[0, 1, 0, 1]);
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

    /// Travelling to a dungeon and entering it opens its party, on the
    /// dungeon's first 구역.
    #[test]
    fn a_dungeon_is_entered_through_its_party() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);

        // 수행의동굴.
        let travel = &ask(&mut world, 2, 1150, &[0, 3])[0][12..];
        assert_eq!(travel[0], 0, "travelled");
        assert_eq!(&ask(&mut world, 2, 1160, &[])[0][12..], &[0, 2, 0, 76]);
        assert_eq!(&ask(&mut world, 7, 10000, &[])[0][12..], &[0]);
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
            assert_eq!(&news[9..17], &[1, 6, 0, 1, 0x0d, 0x5b, 0, 0], "monster 6, struck by the player");
            assert_eq!(&news[17..19], &300i16.to_be_bytes());
            assert_eq!(&news[19..21], &health.to_be_bytes());
            assert_eq!(&news[21..24], &[2, 0, 100], "a hit of 100");
            assert_eq!(&news[24..], &[0, 0, (health > 0) as u8, 0, 0]);
        }

        let mut astray = strike;
        astray[8] = 0xff;
        assert!(ask(&mut world, 7, 17110, &astray).is_empty());
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
        let counters = &ask(&mut world, 8, 23000, &[0])[0][12..];
        assert_eq!(counters.len(), 24);
        assert_eq!(&counters[8..16], &500u64.to_be_bytes());
    }

    /// The progress file reads back what was written, and a file that is not
    /// one reads as nothing.
    #[test]
    fn progress_survives_the_file() {
        use alloc::collections::BTreeMap;

        use super::{Progress, decode, encode};

        let mut all = BTreeMap::new();
        all.insert(
            0x10d5b,
            Progress {
                gold: 500,
                quests: vec![(1, 2), (7, 1)],
            },
        );
        all.insert(3, Progress::default());
        assert_eq!(decode(&encode(&all)), Some(all));

        assert_eq!(decode(&[0, 1, 0, 0]), None);
        assert_eq!(decode(&[0, 0]), Some(BTreeMap::new()));
    }

    /// Nothing is answered before the login.
    #[test]
    fn nothing_before_the_login() {
        let mut world = KodanWorld::default();
        assert_eq!(ask(&mut world, 1, 1010, &[0]), vec![] as Vec<Vec<u8>>);
    }
}
