//! The game server 파워풀프로야구2009 (SKT `OP00115712`) dials to turn its free
//! trial into the full game.
//!
//! The title opens a plain socket of its own (`0x2f1a6`) to
//! `121.254.169.55:15022` when the player chooses `정식판 전환`. The server has
//! been gone for years: the connect never completes and the shop sits on its
//! connecting screen until it gives up.
//!
//! # The protocol
//!
//! Every message either way is a little-endian `u32` length counting the whole
//! message, a little-endian `u32` command, and a body of little-endian `u32`s
//! and fixed-size strings. The builder is `0x2e3dc`; the reader (`0x2efe6`)
//! gathers a whole message by its length and hands its command to `0x2e83c`.
//!
//! The conversion runs with the title's mode (`0x10112ec`) at 3, set by
//! `0x241c6`, and goes:
//!
//! - `0x1000001`, the login: four numbers and the handset's MIN. `0x2e9b8`
//!   reads six numbers and sixteen bytes back, and in mode 3 asks `0x10000f4`.
//! - `0x10000f4`: `0x2ecfe` reads a result. 10 reads two numbers more and asks
//!   `0x10000f3`; 11 is "already paid" (state `0x16`).
//! - `0x10000f3`: `0x2ed30` reads a result. 11 in mode 3 puts the conversion
//!   screen's state (`0x10113e8`) at `0x14`, which shows `정식판 전환이
//!   성공적으로 완료되었습니다`, clears the trial flag at `0x10116e4` and saves
//!   `option1.sav`. 10 would charge through the GXG gateway instead.
//! - `0x1000002`, the logout either reply sends after. `0x2eaa2` closes the
//!   socket.
//!
//! So the login is answered with zeros, `0x10000f4` with 10, `0x10000f3` with
//! 11 and the logout with its bare header. Anything else is written to the
//! trace and left unanswered.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "121.254.169.55";
const PORT: u16 = 15022;

const HEADER: usize = 8;

const LOGIN: u32 = 0x100_0001;
const LOGOUT: u32 = 0x100_0002;
const CHARGE: u32 = 0x100_00f3;
const CHARGE_CHECK: u32 = 0x100_00f4;

/// The login's answer: six numbers and sixteen bytes, none of which the
/// conversion looks at.
const LOGIN_NUMBERS: usize = 6;
const LOGIN_STRING: usize = 16;

/// `0x10000f4`'s "go on to the charge", with the two numbers read after it.
const CHARGE_CHECK_PROCEED: u32 = 10;
/// `0x10000f3`'s "done".
const CHARGE_DONE: u32 = 11;

pub struct PawaproEndpoint;

impl LocalEndpoint for PawaproEndpoint {
    fn name(&self) -> &str {
        "pawapro(121.254.169.55:15022)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(PawaproConnection::default())
    }
}

#[derive(Default)]
struct PawaproConnection {
    pending: Vec<u8>,
    outgoing: Vec<u8>,
    /// An answer is in that the title has not yet been told is coming.
    ///
    /// The title reads straight after every write, and handles an answer -
    /// sending the next request, and reading again - from inside that read.
    /// On the air the first read found nothing and the answer came to the read
    /// callback the title then set, so it is held back the same way here.
    held: bool,
}

fn message(command: u32, body: &[u32]) -> Vec<u8> {
    let mut message = Vec::with_capacity(HEADER + body.len() * 4);
    message.extend_from_slice(&((HEADER + body.len() * 4) as u32).to_le_bytes());
    message.extend_from_slice(&command.to_le_bytes());
    for value in body {
        message.extend_from_slice(&value.to_le_bytes());
    }
    message
}

fn answer(command: u32) -> Option<Vec<u8>> {
    Some(match command {
        LOGIN => {
            let mut answer = message(LOGIN, &[0; LOGIN_NUMBERS]);
            answer.extend_from_slice(&[0; LOGIN_STRING]);
            let length = answer.len() as u32;
            answer[..4].copy_from_slice(&length.to_le_bytes());
            answer
        }
        CHARGE_CHECK => message(CHARGE_CHECK, &[CHARGE_CHECK_PROCEED, 0, 0]),
        CHARGE => message(CHARGE, &[CHARGE_DONE]),
        LOGOUT => message(LOGOUT, &[]),
        _ => return None,
    })
}

impl LocalConnection for PawaproConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while self.pending.len() >= HEADER {
            let length = u32::from_le_bytes(self.pending[..4].try_into().unwrap()) as usize;
            if !(HEADER..=0x2000).contains(&length) {
                tracing::warn!("pawapro: dropping {} bytes that are not a message", self.pending.len());
                self.pending.clear();
                return;
            }
            if self.pending.len() < length {
                return;
            }

            let request: Vec<u8> = self.pending.drain(..length).collect();
            let command = u32::from_le_bytes(request[4..8].try_into().unwrap());
            match answer(command) {
                Some(answer) => {
                    tracing::info!("pawapro: answering {command:#x}");
                    self.outgoing.extend_from_slice(&answer);
                    self.held = true;
                }
                None => tracing::info!("pawapro: no reply shaped for {command:#x}, {} bytes {:02x?}", request.len(), request),
            }
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() || core::mem::take(&mut self.held) {
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

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{CHARGE, CHARGE_CHECK, LOGIN, LOGOUT, PawaproEndpoint, message};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    fn read_answer(connection: &mut dyn LocalConnection) -> Vec<u8> {
        assert!(matches!(connection.read(&mut [0; 16]), LocalRead::Pending));
        let mut out = [0u8; 256];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("no answer");
        };
        out[..read].to_vec()
    }

    fn le(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn only_the_trial_server_is_answered() {
        assert!(PawaproEndpoint.accepts("socket", "121.254.169.55", 15022));
        assert!(!PawaproEndpoint.accepts("socket", "121.254.169.55", 15023));
    }

    #[test]
    fn the_conversion_runs_to_done() {
        let mut connection = PawaproEndpoint.open("socket", "121.254.169.55", 15022);

        // The login, as `0x2e3dc` builds it: four numbers and the MIN.
        let mut login = message(LOGIN, &[0x78, 0x64, 0x103, 0]);
        login.extend_from_slice(b"01046119269\0\0\0\0\0");
        let length = login.len() as u32;
        login[..4].copy_from_slice(&length.to_le_bytes());
        connection.write(&login[..6]);
        assert!(!connection.readable());
        connection.write(&login[6..]);

        let answer = read_answer(connection.as_mut());
        assert_eq!((le(&answer, 0) as usize, le(&answer, 4)), (answer.len(), LOGIN));
        assert_eq!(answer.len(), 8 + 6 * 4 + 16);

        connection.write(&message(CHARGE_CHECK, &[]));
        let answer = read_answer(connection.as_mut());
        assert_eq!((le(&answer, 4), le(&answer, 8), answer.len()), (CHARGE_CHECK, 10, 20));

        connection.write(&message(CHARGE, &[]));
        assert_eq!(read_answer(connection.as_mut()), message(CHARGE, &[11]));

        connection.write(&message(LOGOUT, &[]));
        assert_eq!(read_answer(connection.as_mut()), message(LOGOUT, &[]));

        connection.write(&message(0x0300_0002, &[]));
        assert!(!connection.readable());
    }
}
