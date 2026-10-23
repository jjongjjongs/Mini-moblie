//! The server 테라-영원의혼돈's KTF build fetches its 인증서 from and buys
//! through.
//!
//! The title (KTF `0104050F`) has no 인증서 on its first run and asks
//! `인증서가 없습니다. 최초 1회에 한하여 서버에 접속한 뒤 인증서를 받아옵니다.`
//! On 예 it opens a plain socket to `222.122.160.61` - port 11002 on a
//! handset, 11003 in the headless probe; the LGT build buys through 11001 - and
//! the server is long gone, so it sits on `전송 중입니다..` until it gives up
//! with `네트워크 상태가 원활하지 않습니다`.
//!
//! Every request is a record with no length in front of it: the tag, the
//! message, and fixed-width fields the message decides. Three are known:
//!
//! ```text
//! 0x01  60 bytes   the 인증서: "테라(인증)" in 50 bytes, then "41000"
//! 0x0d  50 bytes   a shop request: the shop, "41001" and "테라" in 46 bytes
//! 0x14  111 bytes  a purchase: a header with the price, the item (the code
//!                  "00490DT0B" and its name), then the shop the same 46 bytes
//! ```
//!
//! The purchase is the record the LGT build buys with (see
//! [`crate::billing::lgt_local_tera_response`]) under this carrier's tag.
//!
//! The answer is what the LGT build reads too: a little-endian `u32` length,
//! then the body. When it is whole the receive loop at `0x11f110` marks it
//! done (state 6) and `0x1a7f60` hands it to the transaction that asked,
//! through the table at `0x204958` indexed by the transaction less 0x64. Each
//! handler compares the body's first byte with the message it sent and takes
//! the second as its status, and reads nothing behind them:
//!
//! - `0x12c`, the 인증서, at `0x1aba7e`. Tried against the title, the status
//!   picks the notice: 1 `인증이 완료되었습니다.`, 2 `인증이 확인되었습니다.`,
//!   3 `인증에 실패하였습니다.` On 1 it keeps the 인증서 and goes on to its
//!   menu, and a later run starts there without dialling at all.
//! - `0xbe`, message 0x0d, at `0x1aae68`: 1 goes on (`0x1aaea6`), anything
//!   else is a failure notice at `0x1ab05a`.
//! - `0xd3`, the purchase, at `0x1aa260`: 1 hands the item over, as in the
//!   LGT build.
//!
//! So each is answered with its message come back and 1. After a record the
//! title writes one zero byte, which asks for nothing.
//!
//! 원조낚시광3 (KTF `01042251`) buys its full version - 정식판 전환, 4000원 -
//! from the same server at port 11001, with a fourth record:
//!
//! ```text
//! 0x1b  67 bytes   a purchase: a header with the price (0x0fa0), the item
//!                  "00490F10A", the shop "70056" and the title's name
//! ```
//!
//! Its dispatcher (`0x1382d4`) has no case for `0x1b`: what answers the
//! purchase is message `0x0d`, whose handler (`0x138a96`) takes the status
//! behind it and on 1 clears the failure the screen reads, which puts up
//! `정식판 전환이 완료되었습니다. 감사합니다.` and swaps 정식판전환 in its menu
//! for 도감열람. Answered with `0x1b` - its own message come back - the title
//! said `정식판 전환에 실패하였습니다`.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "222.122.160.61";
const PORTS: core::ops::RangeInclusive<u16> = 11001..=11003;

const TAG: &[u8] = b"KTF";

/// The messages answered: each one's record size and the message the answer
/// carries.
const RECORDS: [(u8, usize, u8); 4] = [(0x01, 60, 0x01), (0x0d, 50, 0x0d), (0x14, 111, 0x14), (0x1b, 67, 0x0d)];

/// The status every handler goes on from: the 인증서 issued, the shop's
/// request granted, the item handed over.
const GRANTED: u8 = 1;

/// Answers 테라-영원의혼돈's 인증 server.
pub struct TeraEndpoint;

impl LocalEndpoint for TeraEndpoint {
    fn name(&self) -> &str {
        "tera(222.122.160.61)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && PORTS.contains(&port)
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(TeraConnection::default())
    }
}

#[derive(Default)]
struct TeraConnection {
    /// What the title has written that is not yet a whole record.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl LocalConnection for TeraConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // Anything that cannot open a record - the zero byte the title sends
        // after one - is dropped rather than left in front of the next.
        if !self.pending.starts_with(&TAG[..self.pending.len().min(TAG.len())]) {
            self.pending.clear();
            return;
        }

        if self.pending.len() < TAG.len() + 1 {
            return;
        }

        let message = self.pending[TAG.len()];
        let Some(&(_, size, reply)) = RECORDS.iter().find(|(known, _, _)| *known == message) else {
            tracing::info!("tera: no reply shaped for {} bytes {:02x?}", self.pending.len(), self.pending);
            self.pending.clear();
            return;
        };

        if self.pending.len() < size {
            return;
        }

        self.pending.drain(..size);

        let body = [reply, GRANTED];
        self.outgoing.extend_from_slice(&(body.len() as u32).to_le_bytes());
        self.outgoing.extend_from_slice(&body);
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

    use super::{TeraConnection, TeraEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    /// The record the title wrote in the probe.
    fn authentication() -> Vec<u8> {
        let mut record = vec![0u8; 60];
        record[..4].copy_from_slice(b"KTF\x01");
        record[4..14].copy_from_slice(&[0xc5, 0xd7, 0xb6, 0xf3, 0x28, 0xc0, 0xce, 0xc1, 0xf5, 0x29]);
        record[54..59].copy_from_slice(b"41000");
        record
    }

    fn read_all(connection: &mut TeraConnection) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn only_the_authentication_server_is_answered() {
        assert!(TeraEndpoint.accepts("socket", "222.122.160.61", 11002));
        assert!(TeraEndpoint.accepts("socket", "222.122.160.61", 11003));
        assert!(!TeraEndpoint.accepts("socket", "222.122.160.61", 11004));
        assert!(!TeraEndpoint.accepts("socket", "222.122.160.62", 11002));
    }

    #[test]
    fn the_authentication_is_issued() {
        let mut connection = TeraConnection::default();
        let record = authentication();

        // Written in two pieces, answered once whole.
        connection.write(&record[..20]);
        assert!(!connection.readable());
        connection.write(&record[20..]);

        assert_eq!(read_all(&mut connection), [2, 0, 0, 0, 1, 1]);

        // The zero byte after it asks for nothing, and leaves nothing behind.
        connection.write(&[0]);
        assert!(!connection.readable());
        connection.write(&authentication());
        assert_eq!(read_all(&mut connection), [2, 0, 0, 0, 1, 1]);
    }

    #[test]
    fn the_shop_is_granted() {
        let mut connection = TeraConnection::default();

        // As the title wrote them buying from its shop.
        let mut request = vec![0u8; 50];
        request[..9].copy_from_slice(b"KTF\x0d41001");
        request[9..13].copy_from_slice(&[0xc5, 0xd7, 0xb6, 0xf3]);
        connection.write(&request);
        assert_eq!(read_all(&mut connection), [2, 0, 0, 0, 0x0d, 1]);

        let mut purchase = vec![0u8; 111];
        purchase[..14].copy_from_slice(&[0x4b, 0x54, 0x46, 0x14, 0x01, 0x1d, 0, 0, 0, 0x01, 0x84, 0x03, 0, 0]);
        purchase[14..23].copy_from_slice(b"00490DT0B");
        purchase[65..70].copy_from_slice(b"41001");
        connection.write(&purchase[..60]);
        assert!(!connection.readable());
        connection.write(&purchase[60..]);
        assert_eq!(read_all(&mut connection), [2, 0, 0, 0, 0x14, 1]);
    }

    /// 원조낚시광3's full-version purchase, off the device log, is answered as
    /// message 0x0d.
    #[test]
    fn the_fishing_full_version_is_bought() {
        let mut connection = TeraConnection::default();

        let mut purchase = vec![0u8; 67];
        purchase[..11].copy_from_slice(&[0x4b, 0x54, 0x46, 0x1b, 0x0d, 0x01, 0x01, 0xa0, 0x0f, 0x00, 0x00]);
        purchase[11..20].copy_from_slice(b"00490F10A");
        purchase[22..27].copy_from_slice(b"70056");
        connection.write(&purchase);
        assert_eq!(read_all(&mut connection), [2, 0, 0, 0, 0x0d, 1]);
    }

    #[test]
    fn other_messages_are_left_unanswered() {
        let mut connection = TeraConnection::default();
        let mut record = authentication();
        record[3] = 0x0e;
        connection.write(&record);
        assert!(!connection.readable());
    }
}
