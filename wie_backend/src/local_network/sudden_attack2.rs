//! The server 서든어택포켓2 buys its cash items from.
//!
//! The title's KTF build (`01041FB9`) dials `121.78.119.73:45351` through
//! `FastRelay` when a purchase is confirmed, and that server is long gone.
//! Captured, the purchase it sends is seventeen bytes:
//!
//! ```text
//! 29 0f | 00 00 00 0a "1046119269" | 18
//! ```
//!
//! A message type, how many bytes follow, and then its fields: the subscriber
//! number behind a big-endian `u32` length, and the item. The network state
//! machine at `0x11b800` writes it in its state 2 (`0x11b854`) and waits.
//!
//! What comes back is framed differently. The read callback (`0x12d524`) waits
//! for a 45-byte header, takes `[1..5]` as the big-endian length of the whole
//! frame, and hands what follows the header to `0x11b450`, which reads a type
//! byte, skips two, and dispatches. Type `0x29`'s reader (`0x11a704`) takes two
//! big-endian `u32`s - a verdict and the length of a message behind it - and
//! moves the state machine to 10 on a zero verdict and 20 on anything else.
//! State 10 (`0x11b88e`) puts the shop in its state 13, which is 구매 성공, the
//! same state its LGT predecessor reaches on its own server's zero.
//!
//! So a granted purchase is a header carrying the frame's length, and a body of
//! the type, two bytes, a zero verdict and an empty message.
//!
//! It has to arrive a round trip late, as it would have on the air. The send
//! (`0x12d15c`) reads straight away, inside state 2, and state 2 steps the
//! machine on by one once the send returns (`0x11b888`). An answer already
//! waiting is read and dispatched inside that send, moving the machine to 10,
//! and the step then takes it to 11, a state that does nothing - the title sat
//! on 서버로 부터 데이터를 받는 중입니다 for good. So the first read after a
//! purchase finds nothing, and the answer goes to the read callback the title
//! sets on finding nothing.

use alloc::{boxed::Box, format, string::String, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "121.78.119.73";
const PORT: u16 = 45351;

/// The purchase's message type, which its answer comes back under too.
const PURCHASE: u8 = 0x29;

/// The header in front of every frame the server sends.
const HEADER: usize = 0x2d;

/// The server 서든어택포켓2 buys from, answered in process.
pub struct SuddenAttack2Endpoint;

impl LocalEndpoint for SuddenAttack2Endpoint {
    fn name(&self) -> &str {
        "sudden_attack2(121.78.119.73:45351)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _: &str, _: &str, _: u16) -> Box<dyn LocalConnection> {
        Box::<SuddenAttack2Connection>::default()
    }
}

#[derive(Default)]
struct SuddenAttack2Connection {
    /// What the title has sent that is not yet a whole message.
    request: Vec<u8>,
    /// What is left to hand back.
    reply: Vec<u8>,
    /// Whether the next read is the one straight after a request, which finds
    /// nothing however much is waiting.
    held: bool,
}

impl LocalConnection for SuddenAttack2Connection {
    fn write(&mut self, bytes: &[u8]) {
        self.request.extend_from_slice(bytes);

        // A type, the count of what follows, and that much.
        while self.request.len() >= 2 && self.request.len() >= 2 + self.request[1] as usize {
            let message: Vec<u8> = self.request.drain(..2 + self.request[1] as usize).collect();

            if message[0] != PURCHASE {
                tracing::info!(
                    "sudden_attack2: no answer for type {:#04x}: {}",
                    message[0],
                    message.iter().map(|x| format!("{x:02x}")).collect::<String>()
                );
                continue;
            }

            tracing::info!("sudden_attack2: purchase granted");
            self.reply.extend_from_slice(&frame(&purchase_granted()));
            self.held = true;
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.reply.is_empty() || core::mem::take(&mut self.held) {
            return LocalRead::Pending;
        }

        let taken = out.len().min(self.reply.len());
        out[..taken].copy_from_slice(&self.reply[..taken]);
        self.reply.drain(..taken);

        LocalRead::Data(taken)
    }

    fn readable(&self) -> bool {
        !self.reply.is_empty()
    }
}

/// A granted purchase's body: the type, two bytes the reader skips, a zero
/// verdict and an empty message.
fn purchase_granted() -> Vec<u8> {
    let mut body = vec![PURCHASE, 0, 0];
    body.extend_from_slice(&0u32.to_be_bytes());
    body.extend_from_slice(&0u32.to_be_bytes());
    body
}

/// A body behind the server's header, whose `[1..5]` is the whole frame's
/// length.
fn frame(body: &[u8]) -> Vec<u8> {
    let mut frame = vec![0u8; HEADER];
    frame[1..5].copy_from_slice(&((HEADER + body.len()) as u32).to_be_bytes());
    frame.extend_from_slice(body);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The purchase the title sent, captured.
    const PURCHASE_REQUEST: &[u8] = b"\x29\x0f\x00\x00\x00\x0a1046119269\x18";

    #[test]
    fn the_purchase_is_granted() {
        assert!(SuddenAttack2Endpoint.accepts("socket", HOST, PORT));
        assert!(!SuddenAttack2Endpoint.accepts("socket", HOST, PORT + 1));

        let mut connection = SuddenAttack2Endpoint.open("socket", HOST, PORT);
        connection.write(&PURCHASE_REQUEST[..5]);
        assert!(!connection.readable());
        connection.write(&PURCHASE_REQUEST[5..]);
        assert!(connection.readable());

        let mut out = [0u8; 128];
        assert!(matches!(connection.read(&mut out), LocalRead::Pending));
        assert!(connection.readable());
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("the purchase is answered");
        };
        assert_eq!(read, HEADER + 11);
        assert_eq!(u32::from_be_bytes(out[1..5].try_into().unwrap()) as usize, read);
        assert_eq!(out[HEADER], PURCHASE);
        assert_eq!(&out[HEADER + 3..HEADER + 7], &[0, 0, 0, 0]);
    }
}
