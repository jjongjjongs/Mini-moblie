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
//! It does take that connection, and ends it at once. A title left waiting on an
//! endpoint that will never speak waits forever, where the dead server it used
//! to reach at least timed out; an end is a failure a title acts on. The
//! connection is worth taking anyway, because whatever the title sends on it is
//! written down, and a title only sends what it has - those bytes are visible
//! nowhere else.
//!
//! The shop is not framed the way the authentication is, and its own state
//! machine at 0x12aa84 is what says so. It runs on a halfword at its object's
//! +4, and the states it steps through are:
//!
//!   3  waits, then converts its server and connects (net slot 30)
//!   4  waits for the connect callback's byte at the socket's +9, picks its
//!      request by a command code at +0x20 - 100, 200, 300, 500, 1000, 2000 -
//!      builds it, and writes it through net slot 31
//!   5  reads exactly **two** bytes and runs them through `MC_utilNtohs`
//!   6  reads that many more
//!
//! So a shop frame is `[u16 length][payload]`, where an authentication frame is
//! `[u32 length][payload]`. Reading the shop's request as the wider one is
//! reading a length of tens of millions and waiting for a frame that never
//! finishes, which is why a capture of a purchase showed the connection taken
//! and nothing sent on it. The bytes were arriving the whole time.
//!
//! What the title actually sends settles the rest. A handset payment writes
//!
//!   CASH|0|demon|05590091|00635C003|200|549895392
//!
//! and nothing ahead of it: the request carries no length at all, only the
//! record - the command, the application, its serial, the product code, the
//! price in won, and a number of its own for the transaction.
//!
//! The answer does carry a length. The parser at 0x12b298 reads exactly six
//! bytes, steps a cursor two forward, takes four characters and compares them
//! against a constant; equal takes the branch that shows a granted payment,
//! unequal the one that shows a refused one. Six bytes is therefore a two-byte
//! length and the word `SASH`, which is the answer this gives.
//!
//! The KOIN requests on the same connection - `CKN_C` for a balance, `CKN_U` to
//! spend one - are left unanswered. They want numbers back, and a wrong number
//! is worse than none.

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

/// What a handset payment asks, and what a granted one answers with.
const HANDSET_PAYMENT_REQUEST: &[u8] = b"CASH|";
const HANDSET_PAYMENT_GRANTED: &[u8] = b"SASH";

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

    fn open(&self, _: &str, _: &str, port: u16) -> Box<dyn LocalConnection> {
        // The shop's port is unanswerable from the moment it opens: nothing here
        // can serve that exchange, whichever way round the title runs it. The
        // connection is taken all the same, to write down whatever the title
        // sends.
        let authentication = port == AUTHENTICATION_PORT;

        Box::new(GpangConnection {
            authentication,
            unanswerable: !authentication,
            ..Default::default()
        })
    }
}

#[derive(Default)]
struct GpangConnection {
    /// Whether this is the authentication's connection, which is the one whose
    /// framing is known and whose request is answered.
    authentication: bool,
    /// What the title has sent that is not yet a complete frame.
    request: Vec<u8>,
    /// What is left to hand back.
    reply: Vec<u8>,
    /// Whether the title asked something this cannot answer. Its read then
    /// reports end of stream, which is a failure a title acts on, where silence
    /// is one it waits out.
    unanswerable: bool,
    /// Whether the end of this connection has been said out loud. Once, because
    /// a title either polls or waits to be told, and neither wants a line per
    /// attempt.
    end_said: bool,
}

impl GpangConnection {
    /// Answers one of the shop's requests, or writes down the one it cannot.
    ///
    /// The shop's requests carry no length: 데몬헌터 writes
    /// `CASH|0|demon|05590091|00635C003|200|549895392` and nothing ahead of it.
    /// Its answers do carry one - see the module - so a granted payment is two
    /// bytes of length and then the word.
    fn shop_request(&mut self, bytes: &[u8]) {
        tracing::info!("gpang: shop sent {} bytes\n{}", bytes.len(), hex_dump(bytes));

        self.request.extend_from_slice(bytes);

        if !self.request.starts_with(HANDSET_PAYMENT_REQUEST) {
            // The KOIN requests - CKN_C for a balance, CKN_U to spend one - want
            // numbers back that no answer here could invent, and a wrong number
            // is worse than none.
            self.unanswerable = true;

            return;
        }

        tracing::info!("gpang: granting the handset payment");

        self.reply.extend_from_slice(&(HANDSET_PAYMENT_GRANTED.len() as u16).to_be_bytes());
        self.reply.extend_from_slice(HANDSET_PAYMENT_GRANTED);
    }

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
        if !self.authentication {
            self.shop_request(bytes);

            return;
        }

        self.request.extend_from_slice(bytes);

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
            if !self.unanswerable {
                return LocalRead::Pending;
            }

            if !self.end_said {
                tracing::info!("gpang: nothing to answer with; ending the connection");
                self.end_said = true;
            }

            return LocalRead::Closed;
        }

        let taken = out.len().min(self.reply.len());
        out[..taken].copy_from_slice(&self.reply[..taken]);
        self.reply.drain(..taken);

        LocalRead::Data(taken)
    }

    /// A title that waits to be told rather than polling has to be told, or its
    /// read never comes - including the read that finds the end.
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

    /// The authentication's own connection waits while it has nothing to say:
    /// the answer is on its way as soon as the request arrives.
    #[test]
    fn a_read_before_the_request_waits() {
        let mut connection = connection();

        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Pending);
        assert!(!connection.readable());
    }

    fn shop() -> Box<dyn LocalConnection> {
        GpangEndpoint::new().open("socket", GPANG_HOST, SHOP_PORT)
    }

    /// Exactly what 데몬헌터 sends to buy with a handset payment, off the wire.
    const PAYMENT: &[u8] = b"CASH|0|demon|05590091|00635C003|200|549895392";

    /// Six bytes: a two-byte length and the word its parser compares against.
    #[test]
    fn a_handset_payment_is_granted() {
        let mut connection = shop();
        connection.write(PAYMENT);

        assert!(connection.readable());

        let mut out = [0u8; 6];
        assert_eq!(connection.read(&mut out), LocalRead::Data(6));
        assert_eq!(&out, b"\x00\x04SASH");
    }

    /// The KOIN requests want a balance back, so they get an end rather than a
    /// number nobody knows - 데몬헌터 would otherwise sit on that read forever.
    #[test]
    fn a_koin_request_ends_rather_than_waiting() {
        for request in [b"CKN_C|0|demon|0|".as_slice(), b"CKN_U|0|demon|05590091|".as_slice()] {
            let mut connection = shop();
            connection.write(request);

            assert!(connection.readable());
            assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Closed);
        }
    }

    /// A shop connection nothing has been asked on yet is over too: the title
    /// reads before it is answered, and an endpoint that will never speak leaves
    /// it waiting forever.
    #[test]
    fn an_unasked_shop_connection_ends() {
        let mut connection = shop();

        assert!(connection.readable());
        assert_eq!(connection.read(&mut [0u8; 16]), LocalRead::Closed);
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

/// Sixteen bytes to a line, hex then printable ASCII - the shape every other
/// dump in this project reads in.
fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();

    for (index, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|byte| format!("{byte:02x}")).collect();
        let text: String = chunk
            .iter()
            .map(|&byte| if (0x20..0x7f).contains(&byte) { char::from(byte) } else { '.' })
            .collect();

        out.push_str(&format!("  {:04x}  {:<47}  {text}\n", index * 16, hex.join(" ")));
    }

    out
}
