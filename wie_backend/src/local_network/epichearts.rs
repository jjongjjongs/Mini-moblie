//! 에픽하츠's 최초인증 server (KTF `01042932`, 네시삼십삼분), gone for years.
//!
//! A first run - with no `epic19.dat`, the licence a handset keeps once it is
//! authenticated - asks `최초 실행 시 인증이 필요합니다` and, confirmed, dials
//! `175.119.158.72:15007` through the `FastRelay` library.
//!
//! The relay speaks first: its reader (`0x19ec0c`, state `0xc8`) waits for a
//! seven-byte envelope, `[00][u32be size][u16be extension length]`, skips the
//! extension, and only then reads the server's frames. Without it the frames
//! were taken as an envelope and never read. The extension is not read, but it
//! cannot be empty: the reader waits for its length in bytes before going on,
//! and asked for none it reads none, over and over. Two zero bytes stand in.
//!
//! The title then writes one frame,
//! `[u32le length counting the whole frame][0xffff][u16le command][body]`, the
//! command `0x012c`: a version word, the handset's number, the program's id,
//! its model and a few words more. With nothing answering, it sat on its
//! connecting box until it gave up.
//!
//! The answer, command `0x012d`, is read by `0x1bf924` (reached from the
//! dispatcher at `0x1bf41c`):
//!
//! ```text
//! 3 x 0x1c  the account's character slots; +7 is 0xff for an empty one
//! u16       a flag word, neither 1 nor 2 for a new account
//! u16, u32  kept and not looked at for a new account
//! u8        whether the u32 behind it is taken
//! u32
//! u8        0: the handset's own number is the account's; else 12 bytes follow
//! u8        how many notices follow, each a string
//! ```
//!
//! after which the title writes the handset's own `epic19.dat` (`0x15654c`) and
//! goes on to its title screen. Answered with three empty slots and nothing
//! else, it is a new account on this handset.
//!
//! A later launch logs in with command `0x012e` - the version word, the
//! twelve bytes of the account's key (`+0xe4`), the program's id and the
//! model; its builder (`0x1bec90`) sends it expecting `0x012f`, and answered
//! `0x012d` instead the title says `예상하지 못한 패킷이 들어왔습니다 … : 301`.
//! `0x012f`'s branch reads nothing of the body. What the title asks for after
//! it is not known yet: the requests it sends from here on are traced, so a
//! device log names them.
//!
//! No frame from this server may have an empty body: the reader (`0x19ec0c`)
//! takes the head, then waits for the body's length in bytes before it
//! dispatches the frame, and asked for none it reads none, over and over.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "175.119.158.72";
const PORT: u16 = 15007;

const MARKER: u16 = 0xffff;
/// The length, the marker and the command.
const HEADER: usize = 8;

const AUTHENTICATE: u16 = 0x012c;
const AUTHENTICATED: u16 = 0x012d;

/// A later launch's login, sent expecting [`LOGGED_IN`] (`0x1becb8`).
const LOG_IN: u16 = 0x012e;
const LOGGED_IN: u16 = 0x012f;
/// Not read, but a frame's body cannot be empty.
const LOGGED_IN_BODY: [u8; 1] = [0];

const SLOTS: usize = 3;
const SLOT: usize = 0x1c;
const EMPTY_SLOT: u8 = 0xff;
const EMPTY_SLOT_MARK: usize = 7;

/// A frame longer than this is refused rather than buffered without bound.
const MAX_FRAME: usize = 4096;

/// The relay's opening envelope and a two-byte extension.
const GREETING: [u8; 9] = [0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x02, 0x00, 0x00];

/// Answers 에픽하츠's 최초인증 server.
pub struct EpicHeartsEndpoint;

impl LocalEndpoint for EpicHeartsEndpoint {
    fn name(&self) -> &str {
        "epichearts(175.119.158.72:15007)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(EpicHeartsConnection::new())
    }
}

struct EpicHeartsConnection {
    /// What the title has written and not yet made a whole frame of.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl EpicHeartsConnection {
    /// A connection the relay has just greeted.
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            outgoing: GREETING.to_vec(),
        }
    }

    fn take_frame(&mut self) -> Option<Vec<u8>> {
        let length = u32::from_le_bytes(self.pending.get(..4)?.try_into().ok()?) as usize;
        if !(HEADER..=MAX_FRAME).contains(&length) {
            tracing::info!("epichearts: dropping {} bytes that do not frame", self.pending.len());
            self.pending.clear();
            return None;
        }
        if self.pending.len() < length {
            return None;
        }

        Some(self.pending.drain(..length).collect())
    }
}

impl LocalConnection for EpicHeartsConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while let Some(frame) = self.take_frame() {
            match answer(&frame) {
                Some(reply) => self.outgoing.extend_from_slice(&reply),
                None => tracing::info!("epichearts: no reply shaped for {} bytes {:02x?}", frame.len(), frame),
            }
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

/// The answer to one whole frame, for the requests it is known for.
fn answer(frame: &[u8]) -> Option<Vec<u8>> {
    if u16::from_le_bytes([frame[4], frame[5]]) != MARKER {
        return None;
    }

    match u16::from_le_bytes([frame[6], frame[7]]) {
        AUTHENTICATE => {
            tracing::info!("epichearts: 최초인증 granted, a new account");
            Some(reply(AUTHENTICATED, &new_account()))
        }
        LOG_IN => {
            tracing::info!("epichearts: logged in");
            Some(reply(LOGGED_IN, &LOGGED_IN_BODY))
        }
        _ => None,
    }
}

/// [`AUTHENTICATED`]'s body for an account with no characters yet.
fn new_account() -> Vec<u8> {
    let mut body = alloc::vec![0u8; SLOTS * SLOT];
    for slot in 0..SLOTS {
        body[slot * SLOT + EMPTY_SLOT_MARK] = EMPTY_SLOT;
    }
    // The flag word, the u16 and u32 behind it, the byte saying whether the
    // next u32 is taken, that u32, the account's own-number byte and the
    // notice count - every one zero.
    body.extend_from_slice(&[0; 2 + 2 + 4 + 1 + 4 + 1 + 1]);

    body
}

fn reply(command: u16, body: &[u8]) -> Vec<u8> {
    let length = HEADER + body.len();
    let mut reply = Vec::with_capacity(length);
    reply.extend_from_slice(&(length as u32).to_le_bytes());
    reply.extend_from_slice(&MARKER.to_le_bytes());
    reply.extend_from_slice(&command.to_le_bytes());
    reply.extend_from_slice(body);

    reply
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::{EpicHeartsConnection, EpicHeartsEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    /// The 최초인증 the title wrote in the probe.
    const AUTHENTICATE: [u8; 89] = [
        0x59, 0x00, 0x00, 0x00, 0xff, 0xff, 0x2c, 0x01, 0x45, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, //
        b'0', b'1', b'0', b'4', b'6', b'1', b'1', b'9', b'2', b'6', b'9', 0, 0, 0, 0, 0, //
        0, 0, 0, 0, b'0', b'0', b'0', b'1', b'0', b'4', b'2', b'9', b'3', b'2', 0, 0, //
        0, 0, 0, 0, 0, 0, 0, 0, b'E', b'm', b'u', b'l', b'a', b't', b'o', b'r', //
        0, 0, 0, 0, 0, 0, 0, 0, 0xc4, 0x04, 0x04, 0x00, 0x00, 0x00, 0xc0, 0x00, //
        0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
    ];

    #[test]
    fn only_the_authentication_server_is_answered() {
        assert!(EpicHeartsEndpoint.accepts("socket", "175.119.158.72", 15007));
        assert!(!EpicHeartsEndpoint.accepts("socket", "175.119.158.72", 15008));
        assert!(!EpicHeartsEndpoint.accepts("socket", "175.119.158.73", 15007));
    }

    #[test]
    fn the_relay_greets_before_anything_is_written() {
        let mut connection = EpicHeartsConnection::new();
        assert!(connection.readable());

        let mut out = vec![0u8; 16];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("no greeting");
        };
        // The envelope's size counts it all, and the extension is not empty.
        assert_eq!(&out[..read], &[0, 0, 0, 0, 9, 0, 2, 0, 0]);
        assert!(!connection.readable());
    }

    #[test]
    fn the_first_authentication_makes_a_new_account() {
        let mut connection = EpicHeartsConnection::new();
        let mut greeting = [0u8; 9];
        let _ = connection.read(&mut greeting);

        // Written in two pieces, as one frame.
        connection.write(&AUTHENTICATE[..10]);
        assert!(!connection.readable());
        connection.write(&AUTHENTICATE[10..]);
        assert!(connection.readable());

        let mut out = vec![0u8; 256];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("no answer");
        };
        let reply = &out[..read];

        assert_eq!(u32::from_le_bytes(reply[..4].try_into().unwrap()) as usize, reply.len());
        assert_eq!(&reply[4..8], &[0xff, 0xff, 0x2d, 0x01]);
        let body = &reply[8..];
        assert_eq!(body.len(), 3 * 0x1c + 15);
        for slot in 0..3 {
            assert_eq!(body[slot * 0x1c + 7], 0xff, "slot {slot} is empty");
        }
        assert!(body[3 * 0x1c..].iter().all(|&byte| byte == 0));
    }

    /// A launch after the first logs in, off the device log, and is let in.
    #[test]
    fn a_later_launch_logs_in() {
        let mut log_in = vec![0x46, 0x00, 0x00, 0x00, 0xff, 0xff, 0x2e, 0x01, 0x45, 0x00, 0x02, 0x01];
        log_in.extend_from_slice(&[0xad, 0xde, 0x24, 0xce, 0x04, 0x80, 0xc0, 0x2b, 0x76, 0x40, 0x10, 0xce]);
        log_in.extend_from_slice(&[0; 8]);
        log_in.extend_from_slice(b"0001042932");
        log_in.extend_from_slice(&[0; 11]);
        log_in.extend_from_slice(b"Emulator");
        log_in.extend_from_slice(&[0; 9]);
        assert_eq!(log_in.len(), 0x46);

        let mut connection = EpicHeartsConnection::new();
        let mut greeting = [0u8; 9];
        let _ = connection.read(&mut greeting);
        connection.write(&log_in);

        let mut out = vec![0u8; 256];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("no answer");
        };
        // 0x012f, which the login is sent expecting, with a body however
        // short: the reader only dispatches a frame once some of it arrives.
        assert_eq!(&out[..read], &[9, 0, 0, 0, 0xff, 0xff, 0x2f, 0x01, 0]);
    }
}
