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

use alloc::{vec, vec::Vec};

use super::relay::{MESSAGE_WELCOME, SlotRelay, hex};

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
/// Answered with one byte, which the reader (`0x1ba0cc`) keeps on the player
/// before asking for kind 11, command 32450.
const SESSION_ACK: (u32, u32) = (1, 1026);
/// Kind 11 is the world's own bookkeeping. 32450 is answered with a `u16`
/// count of `[byte][u32][byte]` entries (`game/engine/f.a([BI)V`), none here,
/// and its reader then asks kind 8, command 20000.
const WORLD_STATE: (u32, u32) = (11, 32450);
/// Kind 8 is the character's belongings. 20000 is read by `network/a.f([BI)V`
/// as a byte, then a count of `[byte][count][len-prefixed entries]` groups -
/// none here - and its reader then asks kind 8, command 23000.
const BELONGINGS: (u32, u32) = (8, 20000);
/// Three `u64`s, kept in `network/b.a`; the reader then asks kind 2, command
/// 1102.
const COUNTERS: (u32, u32) = (8, 23000);
/// `[byte][len][bytes][u16][u16 count][entries]` (`0x1ba2c4`), the entries
/// `network/h` records - none here; the reader then asks kind 2, command 1114.
const PLACE: (u32, u32) = (2, 1102);
/// `[count][u16 index...]`, marking those of [`PLACE`]'s records (`0x1ba6f8`);
/// the reader then asks kind 2, command 1120.
const PLACE_MARKS: (u32, u32) = (2, 1114);
/// `[len][message]`, a message shown on entering (none here); its reader
/// (`0x1ba7d4`) then puts the title into the world (`base/d.a(…, 1, 5, 100)`).
const ENTER_WORLD: (u32, u32) = (2, 1120);
/// `[len][message][u64]` (`0x1ba87c`), the `u64` a time the title keeps.
const AREA: (u32, u32) = (2, 1130);
/// 입장 from the 코단 map: `[0][byte][u16 map]` (`0x1baba6`) puts the title in
/// the world on that map; a non-zero first byte is a refusal with a message.
const ENTER_AREA: (u32, u32) = (2, 1160);
/// The map 입장 lands on: `p/nt/0/001`, 이스한트 중앙.
const START_MAP: u16 = 1;
/// Kind 3 is life on the map. 5000, sent on arriving, is answered
/// `[byte][u16 len][text]` (`0x1baee8`): the text, kept by
/// `network/a.a([BII)I`, is shown in a popup and must not be empty - the
/// popup reads it through a null otherwise.
const ARRIVED: (u32, u32) = (3, 5000);
/// The keepalive poll, answered with itself.
const POLL: (u32, u32) = (0, 10);

/// The 코단 world server's side of one session.
#[derive(Default)]
pub(super) struct KodanWorld {
    logged_in: bool,
    /// The character [`ENTER`] names: `[byte][u64 id][len][name]`
    /// (`0x1b5e68`), the id being the player's own (`game/chars/g.a()J`).
    character_id: u16,
    character_name: Vec<u8>,
}

impl KodanWorld {
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
                if let [_, _, _, _, _, _, _, high, low, len, name @ ..] = body
                    && name.len() >= *len as usize
                {
                    // A place's records carry an `i16` id: the low half.
                    self.character_id = u16::from_be_bytes([*high, *low]);
                    self.character_name = name[..*len as usize].to_vec();
                }
                let mut session = vec![self.character_name.len() as u8];
                session.extend_from_slice(&self.character_name);
                vec![SlotRelay::message(kind, SESSION, &session)]
            }
            SESSION_ACK if self.logged_in => vec![SlotRelay::message(kind, command, &[1])],
            WORLD_STATE if self.logged_in => vec![SlotRelay::message(kind, command, &[0, 0])],
            BELONGINGS if self.logged_in => vec![SlotRelay::message(kind, command, &[0, 0])],
            COUNTERS if self.logged_in => vec![SlotRelay::message(kind, command, &[0; 24])],
            PLACE if self.logged_in => vec![SlotRelay::message(kind, command, &self.place())],
            PLACE_MARKS if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            ENTER_WORLD if self.logged_in => vec![SlotRelay::message(kind, command, &[0])],
            AREA if self.logged_in => vec![SlotRelay::message(kind, command, &[0; 9])],
            ARRIVED if self.logged_in => {
                let mut arrival = vec![0];
                arrival.extend_from_slice(&(MESSAGE_WELCOME.len() as u16).to_be_bytes());
                arrival.extend_from_slice(MESSAGE_WELCOME);
                vec![SlotRelay::message(kind, command, &arrival)]
            }
            ENTER_AREA if self.logged_in => {
                let mut entry = vec![0, 1];
                entry.extend_from_slice(&START_MAP.to_be_bytes());
                vec![SlotRelay::message(kind, command, &entry)]
            }
            _ => {
                tracing::warn!("코단 world: no answer yet for kind {kind} command {command}: {}", hex(body));
                Vec::new()
            }
        };

        Some(replies)
    }

    /// [`PLACE`]: no title, the player's own index, and the players here -
    /// only the player, whose record the world screen reads by that index.
    fn place(&self) -> Vec<u8> {
        const OWN_INDEX: u16 = 0;
        const CLASS: u8 = 4;
        const X: u16 = 100;
        const Y: u16 = 100;

        let mut place = vec![0, 0];
        place.extend_from_slice(&OWN_INDEX.to_be_bytes());
        place.extend_from_slice(&1u16.to_be_bytes());

        place.extend_from_slice(&self.character_id.to_be_bytes());
        place.push(CLASS);
        place.push(self.character_name.len() as u8);
        place.extend_from_slice(&self.character_name);
        place.extend_from_slice(&X.to_be_bytes());
        place.extend_from_slice(&Y.to_be_bytes());
        place.push(0);

        place
    }
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

    /// The session answer is the character's name, which the dialogue uses.
    #[test]
    fn the_session_carries_the_name() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        let session = &ask(&mut world, 1, 1020, &[0, 0, 0, 0, 0, 0, 0, 0, 7, 3, b'a', b'b', b'c'])[0];
        assert_eq!(&session[12..], &[3, b'a', b'b', b'c']);
    }

    /// The players here are the player alone, under the id and name 1020 gave,
    /// at the index the world screen reads the player's own record by.
    #[test]
    fn the_place_holds_the_player() {
        let mut world = KodanWorld::default();
        ask(&mut world, 1, 1000, &[0x1e]);
        ask(&mut world, 1, 1020, &[0, 0, 0, 0, 0, 0, 0, 0, 7, 3, b'a', b'b', b'c']);

        let place = &ask(&mut world, 2, 1102, &[])[0][12..];
        // No title, own index 0, one player.
        assert_eq!(&place[..6], &[0, 0, 0, 0, 0, 1]);
        // Id 7, then the name behind the class byte.
        assert_eq!(&place[6..8], &[0, 7]);
        assert_eq!(&place[9..13], &[3, b'a', b'b', b'c']);
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

    /// Nothing is answered before the login.
    #[test]
    fn nothing_before_the_login() {
        let mut world = KodanWorld::default();
        assert_eq!(ask(&mut world, 1, 1010, &[0]), vec![] as Vec<Vec<u8>>);
    }
}
