//! The server 드래곤아이즈2 asks before it will start.
//!
//! The title already has every byte it needs: its eight data files ride in the
//! archive's `P/` directory, `MC_dbExists` answers for all of them, and its own
//! screen reads `(872KB/872KB)`. It still opens a socket to `211.110.18.253`
//! and waits there, drawing `CONNECTING` over and over.
//!
//! Waiting is all it can do. The connect routine at `0x104de8` fires
//! `MC_netSocketConnect` and returns without reading the result, so the
//! callback at `0x104c70` is the only thing that can move it on - and that
//! callback answers a failure by storing a flag of 0 and a state of 2 and
//! returning. There is no retry and no branch from there into the game, so
//! refusing the connection leaves the title exactly where losing it does.
//! Something has to answer.
//!
//! # The protocol
//!
//! Every frame both ways is a little-endian `u16` of how many bytes follow,
//! then that many, opening with a `u16` message code. The title's receive path
//! reads those two bytes at `0x104b36`, accumulates until it has the whole
//! frame (`0x104b8a`), and hands it to `0x104570`, whose parser at `0x101160`
//! takes the code and dispatches on it.
//!
//! The title sends 11000 and is answered with 11001:
//!
//! ```text
//!   +0   u16 le  bytes that follow
//!   +2   u16     11000
//!   +4   u8      1
//!   +5   u8[3]   1, 0, 5 - the version its own "ver 1.0.5" string spells
//!   +8   char[11] the subscriber number, NUL terminated
//!   +19  u8      how many records follow
//!   +20  records: u8 name length, the name, u32 le size
//! ```
//!
//! That is an inventory rather than a request: one record per data file with
//! the size the handset holds, and every size matches the archive's own to the
//! byte. The server's part is to say what of it is out of date.
//!
//! Nothing here is: the archive is what a handset that finished this exchange
//! wrote out. So the answer is the one that names no file at all, and the title
//! takes it, writes its own record, and asks to be restarted - `안정성을 위해
//! 단말기를 재시작 해야 됩니다.` - which is what it does when a data session
//! ends rather than fails.
//!
//! # What was read and what was not
//!
//! The framing, the request's layout and the code pair were read out of the
//! title's own code and confirmed against its traffic. The body of an 11001 was
//! not: the parser walks a stream of tagged fields - 5000, 5003, 5006, 5199,
//! 5201 and 5502 among them - and the title reaches the same completion whether
//! that stream carries one of those tags or nothing at all, so what is sent is
//! the shortest thing it accepts. A title that needed a file listed back would
//! need that stream mapped; this one never asks, because it is not missing
//! anything.
//!
//! An 11002 is a code the same parser knows, and answering 11000 with one
//! leaves the title on `CONNECTING` - which is how 11001 was pinned as the
//! reply rather than merely a code the parser recognises.

use alloc::{boxed::Box, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// The host the title opens. The port comes from a state byte read at
/// `0x104e52` - 3 picks 8503 and anything else 8501 - so both are answered.
const HOST: &str = "211.110.18.253";
const PORTS: [u16; 2] = [8501, 8503];

/// What the title sends, and what it is answered with.
const MSG_INVENTORY: u16 = 11000;
const MSG_INVENTORY_REPLY: u16 = 11001;

/// A frame's own header: the length word and the code that follows it.
const HEADER: usize = 4;

/// Answers 드래곤아이즈2's data server.
pub struct DragonEyesEndpoint;

impl LocalEndpoint for DragonEyesEndpoint {
    fn name(&self) -> &str {
        "dragoneyes(211.110.18.253:8501/8503)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && PORTS.contains(&port)
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(DragonEyesConnection {
            pending: Vec::new(),
            outgoing: Vec::new(),
        })
    }
}

struct DragonEyesConnection {
    /// What the title has written that is not yet a whole frame.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl LocalConnection for DragonEyesConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // One write is not always one frame, so take whole frames only and
        // leave any tail for the write that completes it.
        while let Some(frame) = self.take_frame() {
            self.dispatch(&frame);
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

impl DragonEyesConnection {
    /// One whole frame, if `pending` holds one.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
        let length = u16::from_le_bytes([*self.pending.first()?, *self.pending.get(1)?]) as usize;
        let total = length.checked_add(2)?;

        if self.pending.len() < total {
            return None;
        }

        Some(self.pending.drain(..total).collect())
    }

    fn dispatch(&mut self, frame: &[u8]) {
        let Some(code) = frame.get(2..4) else { return };
        let code = u16::from_le_bytes([code[0], code[1]]);

        match code {
            MSG_INVENTORY => {
                tracing::debug!("dragoneyes: inventory of {} bytes, answering with nothing to fetch", frame.len());
                self.reply(MSG_INVENTORY_REPLY, &[0]);
            }
            // Nothing else has been seen from this title. Saying nothing is
            // better than saying something wrong: an answer it cannot place
            // would be read as a frame it asked for.
            _ => tracing::debug!("dragoneyes: nothing to say to message {code}"),
        }
    }

    /// Queues `[u16 le length][u16 code][payload]`.
    fn reply(&mut self, code: u16, payload: &[u8]) {
        let length = (HEADER - 2 + payload.len()) as u16;

        let mut frame = vec![0u8; 0];
        frame.extend_from_slice(&length.to_le_bytes());
        frame.extend_from_slice(&code.to_le_bytes());
        frame.extend_from_slice(payload);

        self.outgoing.extend_from_slice(&frame);
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{DragonEyesConnection, DragonEyesEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    fn connection() -> DragonEyesConnection {
        DragonEyesConnection {
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    /// The inventory the title actually sends, with one file in it.
    fn inventory() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&11000u16.to_le_bytes());
        body.push(1);
        body.extend_from_slice(&[1, 0, 5]);
        body.extend_from_slice(b"1046119269\0");
        body.push(1);
        body.push(b"img.dat".len() as u8);
        body.extend_from_slice(b"img.dat");
        body.extend_from_slice(&83273u32.to_le_bytes());

        let mut frame = (body.len() as u16).to_le_bytes().to_vec();
        frame.extend_from_slice(&body);
        frame
    }

    fn read_all(connection: &mut DragonEyesConnection) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 64];

        while let LocalRead::Data(read) = connection.read(&mut buf) {
            out.extend_from_slice(&buf[..read]);
        }

        out
    }

    #[test]
    fn the_endpoint_takes_both_ports_the_title_may_dial() {
        let endpoint = DragonEyesEndpoint;

        assert!(endpoint.accepts("socket", "211.110.18.253", 8503));
        assert!(endpoint.accepts("socket", "211.110.18.253", 8501));
        assert!(!endpoint.accepts("socket", "211.110.18.253", 8502));
        assert!(!endpoint.accepts("socket", "211.115.203.17", 8503));
        assert!(!endpoint.accepts("http", "211.110.18.253", 8503));
    }

    /// An inventory is answered with 11001, which is what moves the title off
    /// its `CONNECTING` screen.
    #[test]
    fn an_inventory_is_answered() {
        let mut connection = connection();
        connection.write(&inventory());

        let answer = read_all(&mut connection);

        assert_eq!(u16::from_le_bytes([answer[0], answer[1]]) as usize, answer.len() - 2);
        assert_eq!(u16::from_le_bytes([answer[2], answer[3]]), 11001);
    }

    /// A frame split across writes is still one frame, and a second one is
    /// answered on its own.
    #[test]
    fn a_frame_split_across_writes_is_answered_once_it_is_whole() {
        let mut connection = connection();
        let frame = inventory();
        let (head, tail) = frame.split_at(5);

        connection.write(head);
        assert!(read_all(&mut connection).is_empty(), "answered a frame it has not had yet");

        connection.write(tail);

        let answer = read_all(&mut connection);
        assert_eq!(u16::from_le_bytes([answer[2], answer[3]]), 11001);
    }

    /// Two frames in one write are two exchanges.
    #[test]
    fn two_frames_in_one_write_are_both_taken() {
        let mut connection = connection();
        let mut both = inventory();
        both.extend_from_slice(&inventory());

        connection.write(&both);

        let answer = read_all(&mut connection);
        assert_eq!(answer.len(), 10, "expected two five-byte answers, got {answer:?}");
    }

    /// A message this endpoint has never seen gets no answer at all, rather
    /// than one the title would read as the reply it is waiting for.
    #[test]
    fn an_unknown_message_is_left_alone() {
        let mut connection = connection();
        let mut frame = 2u16.to_le_bytes().to_vec();
        frame.extend_from_slice(&11002u16.to_le_bytes());

        connection.write(&frame);

        assert!(read_all(&mut connection).is_empty());
        assert!(!connection.readable());
    }

    /// Nothing is offered before the title has asked for anything.
    #[test]
    fn a_fresh_connection_has_nothing_to_read() {
        let mut connection = connection();

        assert!(!connection.readable());
        assert_eq!(connection.read(&mut [0u8; 8]), LocalRead::Pending);
    }
}
