//! The shop server 렙업만이살길1 dials to buy 돼지.
//!
//! The title (KTF `010100DK`) opens a plain socket to `211.113.45.131:9002`
//! when the player buys 돼지 (its points), sends one to a friend, or registers
//! a ranking. That server has been gone for years. The connect never
//! completes, `SocketConnectCB` never runs, and the shop sits on `구매중
//! 입니다.` for good.
//!
//! # The protocol
//!
//! Which request goes out is picked by the network state byte the shop sets,
//! in the dispatcher at `0x11d06a`. Each request is a fixed-size struct with
//! no length in front of it. It opens with a little-endian `u32` command,
//! then the handset's `PHONENUMBER` and `MIN`, twelve bytes each:
//!
//! | state  | builder    | command | size | what it is |
//! |--------|------------|---------|------|------------|
//! | `0x16` | `0x11d424` | `0x26`  | 40   | buying 돼지 |
//! | `0x15` | `0x11d2fc` | `0x23`  | 52   | a gift      |
//! | `0x14` | `0x11d4cc` | `0x22`  | 56   | the ranking |
//!
//! ```text
//!   +0x00  u32   command
//!   +0x04  [12]  PHONENUMBER
//!   +0x10  [12]  MIN
//!   +0x1c  u32   the result, in the answer
//!   +0x20  ...   the request's own fields
//! ```
//!
//! After the write (`0x11d628`) the title reads four bytes, a little-endian
//! `u32` size taken by `memcpy` at `0x11d7cc`. It then reads that many more
//! (`0x11d850`) and hands them to the handler for the state it is in:
//!
//! - `0x16`, `0x11d9fc`: copies forty bytes over the request it sent and looks
//!   at `+0x1c`. Zero shows the purchase going through and calls `0x108344`,
//!   which adds the pack's 돼지 from the title's own table to the count at
//!   `+0x88`. 6, 7 and anything else are the three failures it knows.
//! - `0x15`, `0x11da70`: copies fifty-two bytes the same way. 1 is the one
//!   failure. Anything else sends the gift.
//!
//! Neither reads anything of the answer but the result. So a purchase or a
//! gift is answered with the struct it sent, its result cleared, behind the
//! size.
//!
//! The ranking's handler, `0x11dac8`, takes a 324-byte record of rank,
//! character data and gifts waiting. None of that is knowable now, so the
//! ranking is not answered: its bytes are written to the trace and nothing is
//! read back, which is where the title was before.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "211.113.45.131";
const PORT: u16 = 9002;

const PURCHASE: u32 = 0x26;
const PURCHASE_SIZE: usize = 40;
const GIFT: u32 = 0x23;
const GIFT_SIZE: usize = 52;
const RANKING: u32 = 0x22;
const RANKING_SIZE: usize = 56;

/// Where the answer carries its result, and the value both handlers go on
/// from.
const RESULT_AT: usize = 0x1c;
const GRANTED: u32 = 0;

/// Answers 렙업만이살길1's shop server.
pub struct LevelUpEndpoint;

impl LocalEndpoint for LevelUpEndpoint {
    fn name(&self) -> &str {
        "levelup(211.113.45.131:9002)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(LevelUpConnection::default())
    }
}

#[derive(Default)]
struct LevelUpConnection {
    /// What the title has written that is not yet a whole request.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl LocalConnection for LevelUpConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while self.pending.len() >= 4 {
            let command = u32::from_le_bytes(self.pending[..4].try_into().unwrap());
            let size = match command {
                PURCHASE => PURCHASE_SIZE,
                GIFT => GIFT_SIZE,
                RANKING => RANKING_SIZE,
                _ => {
                    tracing::info!("levelup: no reply shaped for {} bytes {:02x?}", self.pending.len(), self.pending);
                    self.pending.clear();
                    return;
                }
            };

            if self.pending.len() < size {
                return;
            }

            let request: Vec<u8> = self.pending.drain(..size).collect();
            self.answer(command, request);
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() {
            return LocalRead::Pending;
        }

        let take = out.len().min(self.outgoing.len());
        out[..take].copy_from_slice(&self.outgoing[..take]);
        self.outgoing.drain(..take);

        LocalRead::Data(take)
    }

    fn readable(&self) -> bool {
        !self.outgoing.is_empty()
    }
}

impl LevelUpConnection {
    fn answer(&mut self, command: u32, mut request: Vec<u8>) {
        if command == RANKING {
            tracing::info!("levelup: no reply shaped for the ranking, {} bytes {:02x?}", request.len(), request);
            return;
        }

        tracing::info!("levelup: granting command {command:#x}, {} bytes {:02x?}", request.len(), request);

        request[RESULT_AT..RESULT_AT + 4].copy_from_slice(&GRANTED.to_le_bytes());
        self.outgoing.extend_from_slice(&(request.len() as u32).to_le_bytes());
        self.outgoing.extend_from_slice(&request);
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{LevelUpConnection, LevelUpEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    fn request(command: u32, size: usize) -> Vec<u8> {
        let mut request = vec![0u8; size];
        request[..4].copy_from_slice(&command.to_le_bytes());
        request[4..15].copy_from_slice(b"01046119269");
        request[0x10..0x1b].copy_from_slice(b"01046119269");
        // Whatever the title left where the result goes.
        request[0x1c..0x20].copy_from_slice(&7u32.to_le_bytes());
        request[0x20..0x24].copy_from_slice(&3u32.to_le_bytes());
        request
    }

    fn read_all(connection: &mut LevelUpConnection) -> Vec<u8> {
        let mut out = vec![0u8; 512];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn only_the_shop_server_is_answered() {
        assert!(LevelUpEndpoint.accepts("socket", "211.113.45.131", 9002));
        assert!(!LevelUpEndpoint.accepts("socket", "211.113.45.131", 9003));
        assert!(!LevelUpEndpoint.accepts("socket", "211.234.100.70", 9002));
    }

    #[test]
    fn a_purchase_is_answered_with_itself_granted_behind_its_size() {
        let mut connection = LevelUpConnection::default();
        let purchase = request(0x26, 40);

        // Written in two pieces, answered once whole.
        connection.write(&purchase[..10]);
        assert!(!connection.readable());
        connection.write(&purchase[10..]);

        let answer = read_all(&mut connection);
        assert_eq!(answer.len(), 44);
        assert_eq!(u32::from_le_bytes(answer[..4].try_into().unwrap()), 40);

        let body = &answer[4..];
        assert_eq!(u32::from_le_bytes(body[0x1c..0x20].try_into().unwrap()), 0);
        assert_eq!(&body[..0x1c], &purchase[..0x1c]);
        assert_eq!(&body[0x20..], &purchase[0x20..]);
    }

    #[test]
    fn a_gift_is_granted_the_same_way() {
        let mut connection = LevelUpConnection::default();
        connection.write(&request(0x23, 52));

        let answer = read_all(&mut connection);
        assert_eq!(answer.len(), 56);
        assert_eq!(u32::from_le_bytes(answer[4 + 0x1c..4 + 0x20].try_into().unwrap()), 0);
    }

    #[test]
    fn the_ranking_and_strangers_are_left_unanswered() {
        let mut connection = LevelUpConnection::default();
        connection.write(&request(0x22, 56));
        assert!(!connection.readable());

        connection.write(&request(0x99, 40));
        assert!(!connection.readable());
    }
}
