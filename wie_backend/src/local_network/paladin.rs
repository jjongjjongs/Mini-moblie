//! The ranking server 디펜스히어로 (SKT `0031237334`) fetches its 인증서 from.
//!
//! On its first run the title says it will not play without a certificate
//! (`인증서가 없으면 게임 실행이 되지 않습니다`) and, once the player agrees,
//! dials `rank.paladin.co.kr`, port `20000`. That server is gone; the name
//! still resolves, to an address nothing answers on, so the connect never
//! completes and the title waits on it for good.
//!
//! # The protocol
//!
//! The title's own, not its DSP library's: the network layer runs it when its
//! mode byte (`0x1018dcc`) is 0. Every message either way opens with an 8-byte
//! header:
//!
//! ```text
//!   +0x00  "WM"
//!   +0x02  u16le  the whole message's length, header included
//!   +0x04  u16le  12
//!   +0x06  u8     the type
//!   +0x07  u8     the status, in an answer: zero for success
//!   +0x08  ...    the type's own fields
//! ```
//!
//! The certificate is type 1, built by the dialog's 확인 (`0x12caac`): the
//! header - its length field says 24 - then four twelve-byte fields (two of
//! the handset's properties, two formatted numbers) and two words, all sent
//! in one write. The length a request carries is not its own, so a request
//! is taken by its header alone. The receive
//! (`0x103f58`) reads four bytes, takes the length from a `WM` header and
//! reads the rest; the dispatcher (`0x104280`) goes on `+6`, and type 1's
//! handler (`0x1331e8`) wants the status zero and the little-endian word at
//! `+8` to be 1. Then it sets the flag at `+4` of the title's `CONFIG`
//! (`0x12cc04`) and saves it - that flag is the whole certificate - and the
//! title goes on to its menu. Anything else shows `네트워크 오류로 인증 실패
//! 하였습니다`.
//!
//! The ranking itself is answered with nothing; the title's ranking screens
//! are where they were.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "rank.paladin.co.kr";
/// What the name resolves to now, which the title dials.
const ADDRESS: &str = "121.78.127.249";
/// What this endpoint resolves the name to: an address no title dials of its
/// own accord.
const LOCAL_ADDRESS: [u8; 4] = [10, 80, 76, 68];
const LOCAL_ADDRESS_TEXT: &str = "10.80.76.68";
const PORT: u16 = 20000;

const MAGIC: &[u8; 2] = b"WM";
const HEADER: usize = 8;
const SECOND_FIELD: u16 = 12;

const CERTIFICATE: u8 = 1;
const OK: u8 = 0;
const GRANTED: u32 = 1;

/// Answers 디펜스히어로's ranking server.
pub struct PaladinEndpoint;

impl LocalEndpoint for PaladinEndpoint {
    fn name(&self) -> &str {
        "paladin(rank.paladin.co.kr:20000)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && port == PORT && (host == ADDRESS || host == LOCAL_ADDRESS_TEXT || host.eq_ignore_ascii_case(HOST))
    }

    fn resolve(&self, host: &str) -> Option<[u8; 4]> {
        host.eq_ignore_ascii_case(HOST).then_some(LOCAL_ADDRESS)
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(PaladinConnection::default())
    }
}

#[derive(Default)]
struct PaladinConnection {
    /// What the title has written that is not yet a whole message.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

/// A message of `kind` with `status` and `body` behind the header.
fn message(kind: u8, status: u8, body: &[u8]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&((HEADER + body.len()) as u16).to_le_bytes());
    out.extend_from_slice(&SECOND_FIELD.to_le_bytes());
    out.push(kind);
    out.push(status);
    out.extend_from_slice(body);
    out
}

impl LocalConnection for PaladinConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() < HEADER {
            return;
        }
        if !self.pending.starts_with(MAGIC) {
            tracing::info!("paladin: no message in {} bytes {:02x?}", self.pending.len(), self.pending);
            self.pending.clear();
            return;
        }

        // One request a write, its length not its own: answered by its
        // header, and whatever follows it in the same write is its body.
        let request: Vec<u8> = core::mem::take(&mut self.pending);
        match request[6] {
            CERTIFICATE => {
                tracing::info!("paladin: certificate granted");
                self.outgoing.extend_from_slice(&message(CERTIFICATE, OK, &GRANTED.to_le_bytes()));
            }
            kind => tracing::info!("paladin: no answer for type {kind}, {} bytes {:02x?}", request.len(), request),
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

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{PaladinConnection, PaladinEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    fn read_all(connection: &mut PaladinConnection) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            _ => Vec::new(),
        }
    }

    /// The certificate request as the title builds it: the header (its
    /// length field 24), four twelve-byte fields and two words.
    fn certificate_request() -> Vec<u8> {
        let mut request = b"WM".to_vec();
        request.extend_from_slice(&24u16.to_le_bytes());
        request.extend_from_slice(&12u16.to_le_bytes());
        request.extend_from_slice(&[1, 0]);
        request.extend_from_slice(b"01046119269\0");
        request.extend_from_slice(b"SCH-W830\0\0\0\0");
        request.extend_from_slice(&[0; 24]);
        request.extend_from_slice(&1u32.to_le_bytes());
        request.extend_from_slice(&0u32.to_le_bytes());
        request
    }

    #[test]
    fn the_ranking_server_is_answered_by_name_and_address() {
        assert_eq!(PaladinEndpoint.resolve("rank.paladin.co.kr"), Some([10, 80, 76, 68]));
        assert_eq!(PaladinEndpoint.resolve("example.com"), None);
        assert!(PaladinEndpoint.accepts("socket", "10.80.76.68", 20000));
        assert!(PaladinEndpoint.accepts("socket", "121.78.127.249", 20000));
        assert!(!PaladinEndpoint.accepts("socket", "121.78.127.249", 20001));
    }

    #[test]
    fn the_certificate_is_granted() {
        let mut connection = PaladinConnection::default();
        let request = certificate_request();

        // Nothing until the header is in.
        connection.write(&request[..4]);
        assert!(!connection.readable());
        connection.write(&request[4..]);

        let answer = read_all(&mut connection);
        assert_eq!(&answer[..2], b"WM");
        assert_eq!(u16::from_le_bytes([answer[2], answer[3]]) as usize, answer.len());
        assert_eq!(answer[6], 1, "type 1");
        assert_eq!(answer[7], 0, "status zero");
        assert_eq!(u32::from_le_bytes(answer[8..12].try_into().unwrap()), 1, "granted");
    }

    #[test]
    fn other_types_are_left_unanswered() {
        let mut connection = PaladinConnection::default();
        let mut request = certificate_request();
        request[6] = 5;
        connection.write(&request);
        assert!(!connection.readable());
    }
}
