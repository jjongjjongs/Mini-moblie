//! The server 아무이유없어2 keeps its saves on.
//!
//! The title (KTF `01042813`, KTH) checks for a save of its own on the server
//! before it starts - `기존에 저장되어 있는 저장파일이 있는지 확인중입니다` - and
//! dials `211.234.100.70:10100` for it. That server has been gone for years, the
//! connect callback reported a failure, and the title went no further.
//!
//! Both directions are the same sixty byte header, with whatever the message
//! carries behind it:
//!
//! ```text
//! [0x00..0x02]  i16le  command
//! [0x02..0x0c]         the title's name, "NOREASON2"
//! [0x1c..0x20]  i32le  how many bytes follow the header
//! [0x20..0x2c]         the subscriber number
//! [0x2c..0x37]         the content id, "0091373452"
//! ```
//!
//! The save check is command `0x14` with nothing behind it (`0x126390`). The
//! read callback (`0x118ef8`) gathers what arrives and hands it to `0x1184b8`,
//! which for this exchange waits for a whole header and the body it declares,
//! wants command `0x6e`, and reads a `u32` out of the body: `0` is "no save
//! here" (state `0x1f`), `1` is a save that follows it (`0x24`), anything else a
//! failure (`0x25`). Nothing else in the header is read, so the answer is the
//! request's own header with the command and the length changed, and a zero.
//!
//! A purchase is command `0x15` (`0x125ca4`), its body a `u32` naming the item
//! and `[0x2c]` the item's code. Its answer comes back under command `0x78`,
//! whose reader (`0x118ad8`) takes twelve bytes of body: a verdict and two
//! `u32`s it keeps. The purchase screen (`0x13a814`) waits for the verdict's
//! state - `0` is 50, the purchase made, `1` is 51 and `2` is 54 - so a granted
//! purchase is the request's header under `0x78` with twelve zero bytes behind
//! it.
//!
//! The same server and the same state machine carry the title's other
//! exchanges, under commands of their own. Those wait on bodies nothing here has
//! read, so they are written down and the connection is ended, which the title
//! reports as a failure rather than waiting on an answer that is not coming.

use alloc::{boxed::Box, format, string::String, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "211.234.100.70";
const PORT: u16 = 10100;

/// The header every message opens with, either way.
const HEADER: usize = 0x3c;

/// Where the header says how long the body behind it is.
const BODY_LENGTH_AT: usize = 0x1c;

/// The largest body taken for a frame; anything longer is not one.
const MAX_BODY: usize = 0x10000;

/// The save check, and the command its answer comes back under.
const SAVE_CHECK: i16 = 0x14;
const SAVE_CHECK_ANSWER: i16 = 0x6e;

/// The save check's verdict: there is no save on the server.
const NO_SAVE: u32 = 0;

/// A purchase, and the command its answer comes back under.
const PURCHASE: i16 = 0x15;
const PURCHASE_ANSWER: i16 = 0x78;

/// A granted purchase's body: a zero verdict and the two values kept with it.
const PURCHASE_GRANTED: [u8; 12] = [0; 12];

/// The server 아무이유없어2 checks for a save on, answered in process.
pub struct NoReasonEndpoint {
    name: String,
}

impl Default for NoReasonEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl NoReasonEndpoint {
    pub fn new() -> Self {
        Self {
            name: format!("noreason({HOST}:{PORT})"),
        }
    }
}

impl LocalEndpoint for NoReasonEndpoint {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _: &str, _: &str, _: u16) -> Box<dyn LocalConnection> {
        Box::<NoReasonConnection>::default()
    }
}

#[derive(Default)]
struct NoReasonConnection {
    /// What the title has sent that is not yet a whole message.
    request: Vec<u8>,
    /// What is left to hand back.
    reply: Vec<u8>,
    /// Whether the title asked something this cannot answer. Its read then
    /// reports end of stream.
    unanswerable: bool,
}

impl NoReasonConnection {
    /// Takes one whole message off the front of `request`, if there is one.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
        if self.request.len() < HEADER {
            return None;
        }

        let body = i32::from_le_bytes(self.request[BODY_LENGTH_AT..BODY_LENGTH_AT + 4].try_into().unwrap());
        let Some(body) = usize::try_from(body).ok().filter(|&body| body <= MAX_BODY) else {
            tracing::warn!("noreason: {} bytes that are not a message", self.request.len());
            self.request.clear();
            self.unanswerable = true;

            return None;
        };

        if self.request.len() < HEADER + body {
            return None;
        }

        Some(self.request.drain(..HEADER + body).collect())
    }
}

impl LocalConnection for NoReasonConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.request.extend_from_slice(bytes);

        while let Some(frame) = self.take_frame() {
            let command = i16::from_le_bytes([frame[0], frame[1]]);
            match command {
                SAVE_CHECK => {
                    tracing::info!("noreason: no save on the server");
                    self.reply.extend_from_slice(&answer(&frame, SAVE_CHECK_ANSWER, &NO_SAVE.to_le_bytes()));
                }
                PURCHASE => {
                    tracing::info!("noreason: purchase granted");
                    self.reply.extend_from_slice(&answer(&frame, PURCHASE_ANSWER, &PURCHASE_GRANTED));
                }
                _ => {
                    tracing::info!(
                        "noreason: no answer for command {command:#x}, {} bytes: {}",
                        frame.len(),
                        frame.iter().map(|x| format!("{x:02x}")).collect::<String>()
                    );
                    self.unanswerable = true;
                }
            }
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.reply.is_empty() {
            return if self.unanswerable { LocalRead::Closed } else { LocalRead::Pending };
        }

        let taken = out.len().min(self.reply.len());
        out[..taken].copy_from_slice(&self.reply[..taken]);
        self.reply.drain(..taken);

        LocalRead::Data(taken)
    }

    fn readable(&self) -> bool {
        !self.reply.is_empty() || self.unanswerable
    }
}

/// An answer: the request's own header under the answer's command, and the
/// body behind it.
fn answer(request: &[u8], command: i16, body: &[u8]) -> Vec<u8> {
    let mut answer = request[..HEADER].to_vec();
    answer[..2].copy_from_slice(&command.to_le_bytes());
    answer[BODY_LENGTH_AT..BODY_LENGTH_AT + 4].copy_from_slice(&(body.len() as i32).to_le_bytes());
    answer.extend_from_slice(body);

    answer
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn save_check() -> Vec<u8> {
        let mut request = vec![0u8; HEADER];
        request[..2].copy_from_slice(&SAVE_CHECK.to_le_bytes());
        request[2..11].copy_from_slice(b"NOREASON2");
        request[0x20..0x2b].copy_from_slice(b"01046119269");
        request[0x2c..0x36].copy_from_slice(b"0091373452");
        request
    }

    /// The save check, written in two pieces, is answered that there is no
    /// save: command `0x6e`, a body of four, and a zero in it.
    #[test]
    fn the_save_check_finds_no_save() {
        let endpoint = NoReasonEndpoint::new();
        assert!(endpoint.accepts("socket", HOST, PORT));
        assert!(!endpoint.accepts("socket", HOST, PORT + 1));

        let request = save_check();
        let mut connection = endpoint.open("socket", HOST, PORT);
        connection.write(&request[..20]);
        assert!(!connection.readable());
        connection.write(&request[20..]);
        assert!(connection.readable());

        let mut out = [0u8; 128];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("the save check is answered");
        };
        assert_eq!(read, HEADER + 4);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), SAVE_CHECK_ANSWER);
        assert_eq!(&out[2..11], b"NOREASON2");
        assert_eq!(i32::from_le_bytes(out[BODY_LENGTH_AT..BODY_LENGTH_AT + 4].try_into().unwrap()), 4);
        assert_eq!(&out[HEADER..HEADER + 4], &[0, 0, 0, 0]);
    }

    /// A purchase, as the title wrote it, is granted: command `0x78`, a body
    /// of twelve, and a zero verdict.
    #[test]
    fn a_purchase_is_granted() {
        let mut request = save_check();
        request[..2].copy_from_slice(&PURCHASE.to_le_bytes());
        request[BODY_LENGTH_AT] = 4;
        request[0x2c..0x35].copy_from_slice(b"202412I04");
        request.extend_from_slice(&0x57u32.to_le_bytes());

        let mut connection = NoReasonEndpoint::new().open("socket", HOST, PORT);
        connection.write(&request);

        let mut out = [0u8; 128];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("the purchase is answered");
        };
        assert_eq!(read, HEADER + 12);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), PURCHASE_ANSWER);
        assert_eq!(i32::from_le_bytes(out[BODY_LENGTH_AT..BODY_LENGTH_AT + 4].try_into().unwrap()), 12);
        assert_eq!(&out[HEADER..HEADER + 12], &[0u8; 12]);
    }

    /// Any other command ends the connection.
    #[test]
    fn another_command_ends_the_connection() {
        let mut request = save_check();
        request[0] = 0x13;

        let mut connection = NoReasonEndpoint::new().open("socket", HOST, PORT);
        connection.write(&request);

        assert!(connection.readable());
        assert!(matches!(connection.read(&mut [0u8; 16]), LocalRead::Closed));
    }
}
