//! The answer 데몬헌터's authentication asks for.
//!
//! The title says what it needs on its own title screen - `최초 게임실행 시 서버
//! 접속을 통한 인증이 1회 필요합니다.` - and then asks `222.237.78.175:40240`
//! for it. That server has been gone for years, so the run reaches a twenty
//! second connect timeout and the title puts up its network failure, once per
//! launch, forever: the file it would write to remember the answer is only
//! written when an answer comes.
//!
//! What it sends is a length ahead of a tab-separated record, measured off the
//! wire:
//!
//!   00 00 00 2c  IR \t 01046119269 \t demon \t 1.0.2 \t 5080091 \t WIPIC \t yes
//!
//! - four bytes of big-endian length counting only what follows, then the
//! record its own format string spells, `IR\t%s\t%s\t%s\t%s\tWIPIC\t%s`, with
//! the subscriber number the handset answered `PHONENUMBER` with.
//!
//! The answer is one frame the same way. `IROK` is the word the title's own code
//! carries for a granted authentication, next to the message it shows for one -
//! `정상적으로 인증처리되었습니다. 감사합니다.` - and the read it posts asks for
//! sixteen bytes, which is that word behind its length with a payload of eight.
//! Answered so, the title shows that message and goes on to the game.
//!
//! The same server carries the title's cash shop, on a port of its own: a
//! purchase dials `222.237.78.175:10020`, named `TEST_BILLSOCK` the same way.
//! What it asks there - `CKN_C` for a KOIN balance, `CKN_U` to spend one, `CASH`
//! for a handset payment - wants balances and prices back, which no answer here
//! could invent, so this does not answer them.
//!
//! It does take the connection, for two reasons. The request is written down -
//! a title only sends what it has, and this is the only place those bytes are
//! ever visible - and the connection is then ended rather than left open, so the
//! title reports its own network failure at once instead of sitting on a screen
//! until a twenty second timeout it can no longer reach.

use alloc::{boxed::Box, format, string::String, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// Where this title's servers lived.
const GPANG_HOST: &str = "222.237.78.175";

/// The port the authentication asks on, and the one the cash shop asks on.
const AUTHENTICATION_PORT: u16 = 40240;
const SHOP_PORT: u16 = 10020;

/// The width of the length that opens every frame, counting only what follows.
const LENGTH_WIDTH: usize = 4;

/// What a request asking to be authenticated opens with.
const AUTHENTICATION_REQUEST: &[u8] = b"IR\t";

/// What a granted authentication answers with.
const AUTHENTICATION_GRANTED: &[u8] = b"IROK";

/// How much the answer carries behind that word. The title's read asks for
/// sixteen bytes and gets no more; a shorter answer is one it waits out.
const GRANTED_PAYLOAD: usize = 8;

/// The server 데몬헌터 authenticates against, answered in process.
pub struct GpangEndpoint {
    name: String,
}

impl Default for GpangEndpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl GpangEndpoint {
    pub fn new() -> Self {
        Self {
            name: format!("gpang({GPANG_HOST}:{AUTHENTICATION_PORT},{SHOP_PORT})"),
        }
    }
}

impl LocalEndpoint for GpangEndpoint {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == GPANG_HOST && matches!(port, AUTHENTICATION_PORT | SHOP_PORT)
    }

    fn open(&self, _: &str, _: &str, _: u16) -> Box<dyn LocalConnection> {
        Box::new(GpangConnection::default())
    }
}

#[derive(Default)]
struct GpangConnection {
    /// What the title has sent that is not yet a complete frame.
    request: Vec<u8>,
    /// What is left to hand back.
    reply: Vec<u8>,
    /// Whether the title asked something this cannot answer. Its read then
    /// reports end of stream, which is a failure a title acts on, where silence
    /// is one it waits out.
    unanswerable: bool,
    /// Whether anything has ever been sent on this connection.
    written: bool,
    /// Whether it has been said that this connection is being read before
    /// anything was sent on it. Once, because a title in that position either
    /// polls or waits to be told, and neither wants a line per attempt.
    waiting_said: bool,
}

impl GpangConnection {
    /// Takes one complete frame off the front of `request`, if there is one.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
        if self.request.len() < LENGTH_WIDTH {
            return None;
        }

        let length = self.request[..LENGTH_WIDTH]
            .iter()
            .fold(0usize, |value, &byte| (value << 8) | byte as usize);
        let total = LENGTH_WIDTH + length;

        if self.request.len() < total {
            return None;
        }

        let frame = self.request.drain(..total).collect::<Vec<_>>();

        Some(frame[LENGTH_WIDTH..].to_vec())
    }
}

impl LocalConnection for GpangConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.request.extend_from_slice(bytes);
        self.written = true;

        while let Some(payload) = self.take_frame() {
            if !payload.starts_with(AUTHENTICATION_REQUEST) {
                // Everything else this server carries is the cash shop, which
                // wants content rather than a yes. Writing the request down is
                // what this can offer it - a title only sends what it has, and
                // these bytes are visible nowhere else - and then ending the
                // connection, so the title reports a failure rather than waiting
                // on an answer that is not coming.
                self.unanswerable = true;
                tracing::info!(
                    "gpang: no answer for {:?}",
                    payload
                        .iter()
                        .map(|&byte| if byte == b'\t' { '\u{2192}' } else { char::from(byte) })
                        .collect::<String>()
                );
                continue;
            }

            tracing::info!("gpang: authenticating {} bytes", payload.len());

            let granted = AUTHENTICATION_GRANTED.len() + GRANTED_PAYLOAD;
            let mut frame = vec![0u8; LENGTH_WIDTH + granted];

            for index in 0..LENGTH_WIDTH {
                frame[index] = (granted >> (8 * (LENGTH_WIDTH - 1 - index))) as u8;
            }
            frame[LENGTH_WIDTH..LENGTH_WIDTH + AUTHENTICATION_GRANTED.len()].copy_from_slice(AUTHENTICATION_GRANTED);

            self.reply.extend_from_slice(&frame);
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.reply.is_empty() {
            if !self.unanswerable && !self.written && !self.waiting_said {
                // A title reading a connection it has not written to is waiting
                // for the server to speak first, and this one does not know what
                // that server said. Worth one line, because from outside it
                // looks exactly like a title that has simply stopped.
                tracing::info!("gpang: read before anything was sent - this connection is waiting to be spoken to first");
                self.waiting_said = true;
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Exactly what 데몬헌터 writes, off the wire.
    const REQUEST: &[u8] = b"\x00\x00\x00\x2cIR\t01046119269\tdemon\t1.0.2\t5080091\tWIPIC\tyes";

    fn connection() -> Box<dyn LocalConnection> {
        GpangEndpoint::new().open("socket", GPANG_HOST, AUTHENTICATION_PORT)
    }

    #[test]
    fn the_authentication_is_granted() {
        let mut connection = connection();
        connection.write(REQUEST);

        assert!(connection.readable());

        let mut out = [0u8; 16];
        assert_eq!(connection.read(&mut out), LocalRead::Data(16));
        assert_eq!(&out, b"\x00\x00\x00\x0cIROK\0\0\0\0\0\0\0\0");
    }

    /// The title's read asks for sixteen bytes and takes them a few at a time
    /// when that is all it is given, so the answer has to survive being split.
    #[test]
    fn an_answer_taken_piecemeal_is_still_the_whole_answer() {
        let mut connection = connection();
        connection.write(REQUEST);

        let mut whole = Vec::new();
        loop {
            let mut out = [0u8; 3];
            match connection.read(&mut out) {
                LocalRead::Data(taken) => whole.extend_from_slice(&out[..taken]),
                _ => break,
            }
        }

        assert_eq!(whole, b"\x00\x00\x00\x0cIROK\0\0\0\0\0\0\0\0");
    }

    /// A request arriving in pieces is one request, not none.
    #[test]
    fn a_request_split_across_writes_is_read_as_one() {
        let mut connection = connection();

        for chunk in REQUEST.chunks(7) {
            connection.write(chunk);
        }

        assert!(connection.readable());
    }

    /// Nothing is answered until the whole frame its length promises is there.
    #[test]
    fn a_frame_short_of_its_length_is_not_answered() {
        let mut connection = connection();
        connection.write(&REQUEST[..REQUEST.len() - 1]);

        assert!(!connection.readable());
        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Pending);
    }

    /// A title reading a connection it has never written to is waiting to be
    /// spoken to first - 데몬헌터's shop does exactly this - and that is a wait,
    /// not an end: the endpoint has no grounds to say the exchange is over.
    #[test]
    fn a_read_before_anything_is_sent_waits() {
        let mut connection = connection();

        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Pending);
        assert!(!connection.readable());
    }

    /// The cash shop asks for content, and a yes is not content. The connection
    /// ends instead, so the title reports a failure rather than waiting.
    #[test]
    fn the_cash_shop_is_not_answered() {
        let mut connection = connection();
        connection.write(b"\x00\x00\x00\x17CKN_U|0|demon|05590091|");

        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Closed);
    }

    /// Only the one server, so a title reaching any other still reaches the
    /// network.
    #[test]
    fn no_other_address_is_answered_for() {
        let endpoint = GpangEndpoint::new();

        // Both of the title's ports: 데몬헌터 authenticates on one and shops on
        // the other, and a shop connection left to the network is one that waits
        // out a timeout with its request unread.
        assert!(endpoint.accepts("socket", GPANG_HOST, AUTHENTICATION_PORT));
        assert!(endpoint.accepts("socket", GPANG_HOST, SHOP_PORT));
        assert!(!endpoint.accepts("socket", GPANG_HOST, 80));
        assert!(!endpoint.accepts("socket", "222.237.78.176", AUTHENTICATION_PORT));
        assert!(!endpoint.accepts("http", GPANG_HOST, AUTHENTICATION_PORT));
    }
}
