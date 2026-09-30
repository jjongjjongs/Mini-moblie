//! 오즈-천공의 기사단's own game server, answered in process.
//!
//! The title's 새로하기 opens `socket://210.222.18.25:31000` the moment
//! character creation begins. The server has been gone for years; with nothing
//! answering, the connect times out and the title drops back to the menu. This
//! answers it in process so the connect and the login handshake go through.
//!
//! # The protocol (as measured off its own socket)
//!
//! Length-prefixed binary, `[u32be length][u32be type][payload]`, the length
//! covering the whole frame. Two exchanges are seen:
//!
//! - a **login**, `00 00 00 0d 00 00 00 01 00 00 03 e8 28` - type 1, a five
//!   byte payload - answered with `00 00 00 0c 00 00 00 01 00 00 00 00`, type
//!   1 and a zero status, which the title accepts.
//! - a **poll**, `00 00 00 0c 00 00 00 00 00 00 00 0a` - type 0, payload 10 -
//!   which the title sends every five seconds while its screen says
//!   CONNECTING, waiting for the server's go-ahead. Answering it with the same
//!   type and a zero status does not satisfy it: the title reads the reply,
//!   keeps CONNECTING and polls again.
//!
//! What the title accepts to leave CONNECTING is not knowable from the outside -
//! it is a server push it never asks for in a form the reply can carry - so the
//! branch that gates the screen has to be read from the compiled code. A device
//! run does connect and poll reliably, so [`OzConnection::read`] arms the
//! control-flow probe the moment the title finishes reading a poll reply, which
//! writes the branch trace of the dispatch that rejects it to the log. That is
//! the lead needed to locate the gate; the reply and the title's behaviour are
//! unchanged. DIAGNOSTIC(oz-connect): to be removed once the branch is located.

use alloc::{boxed::Box, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// The game server this answers for.
const HOST: &str = "210.222.18.25";
const PORT: u16 = 31000;

/// `[u32be length][u32be type]` before any payload.
const HEADER: usize = 8;

/// Branches to trace once a poll reply has been read, enough to carry past the
/// receiver's queue write and into the main thread's dispatch of it.
const TRACE_BRANCHES: u32 = 60000;

/// Answers 오즈-천공의 기사단's game server.
pub struct OzKnightsEndpoint;

impl LocalEndpoint for OzKnightsEndpoint {
    fn name(&self) -> &str {
        "oz(210.222.18.25:31000)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(OzConnection::default())
    }
}

#[derive(Default)]
struct OzConnection {
    /// What the title has written that is not yet a complete frame.
    request: Vec<u8>,
    /// What is left to hand back.
    outgoing: Vec<u8>,
    /// A poll reply is queued and the probe should be armed the moment the
    /// title starts reading it, to trace the parse and dispatch that follow.
    arm_pending: bool,
    /// The probe is armed only once, on the first poll reply.
    armed: bool,
}

impl OzConnection {
    /// Takes one complete `[u32be length][u32be type][payload]` frame off the
    /// front of `request`, if a whole one has arrived.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
        if self.request.len() < HEADER {
            return None;
        }

        let total = u32::from_be_bytes([self.request[0], self.request[1], self.request[2], self.request[3]]) as usize;
        if total < HEADER {
            tracing::warn!(
                "oz {HOST}:{PORT}: a frame of {total} bytes is shorter than its header; dropping {} buffered bytes",
                self.request.len()
            );
            self.request.clear();
            return None;
        }
        if self.request.len() < total {
            return None;
        }

        Some(self.request.drain(..total).collect())
    }

    /// The reply to `frame`: its type echoed and a zero status, under a length
    /// that counts the whole frame - the "granted" the title's parser reads.
    fn reply(frame: &[u8]) -> Vec<u8> {
        let mut reply = vec![0u8; HEADER + 4];
        let total = reply.len() as u32;
        reply[..4].copy_from_slice(&total.to_be_bytes());
        reply[4..HEADER].copy_from_slice(&frame[4..HEADER]);
        // The last four bytes stay zero: the status the title reads as granted.
        reply
    }
}

impl LocalConnection for OzConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.request.extend_from_slice(bytes);

        while let Some(frame) = self.take_frame() {
            let message_type = u32::from_be_bytes([frame[4], frame[5], frame[6], frame[7]]);
            let reply = Self::reply(&frame);
            tracing::info!("oz {HOST}:{PORT}: {} -> {}", hex(&frame), hex(&reply));

            // The poll (type 0) is the one the title keeps re-sending; arm the
            // probe the moment the title starts reading this reply, so the trace
            // covers the receiver reading the frame and the dispatch it runs on
            // it - the parse completes right after the last byte is read, so
            // arming on the drain caught the idle loop just past it instead. The
            // login (type 1) is left alone.
            if message_type == 0 && !self.armed {
                self.arm_pending = true;
            }

            self.outgoing.extend_from_slice(&reply);
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() {
            return LocalRead::Pending;
        }

        // DIAGNOSTIC(oz-connect): the title is about to read the poll reply;
        // trace from here through its parse and the dispatch that rejects it.
        if self.arm_pending && !self.armed {
            self.arm_pending = false;
            self.armed = true;
            crate::probe::arm("oz-connect", TRACE_BRANCHES);
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

/// A compact hex+ASCII rendering of a captured buffer, for pinning the framing.
fn hex(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;

    let mut out = alloc::string::String::new();
    for &byte in bytes.iter().take(64) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{HEADER, LocalConnection, LocalRead, OzConnection};
    use alloc::{vec, vec::Vec};

    fn read_all(connection: &mut OzConnection) -> Vec<u8> {
        let mut out = vec![0u8; 256];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            other => panic!("expected data, got {other:?}"),
        }
    }

    #[test]
    fn the_login_is_answered_type_1_zero_status() {
        let mut connection = OzConnection::default();
        connection.write(&[0x00, 0x00, 0x00, 0x0d, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x03, 0xe8, 0x28]);

        let reply = read_all(&mut connection);
        assert_eq!(reply, [0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(reply.len(), HEADER + 4);
    }

    #[test]
    fn the_poll_is_answered_type_0_zero_status() {
        let mut connection = OzConnection::default();
        connection.write(&[0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a]);

        let reply = read_all(&mut connection);
        assert_eq!(reply, [0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn a_partial_frame_is_not_answered_until_complete() {
        let mut connection = OzConnection::default();
        connection.write(&[0x00, 0x00, 0x00, 0x0c, 0x00, 0x00]);
        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Pending);
        connection.write(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x0a]);
        assert_eq!(read_all(&mut connection).len(), 12);
    }
}
