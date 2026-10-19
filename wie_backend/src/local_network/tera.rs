//! The server 테라-영원의혼돈's KTF build fetches its 인증서 from.
//!
//! The title (KTF `0104050F`) has no 인증서 on its first run and asks
//! `인증서가 없습니다. 최초 1회에 한하여 서버에 접속한 뒤 인증서를 받아옵니다.`
//! On 예 it opens a plain socket to `222.122.160.61` - port 11002 on a
//! handset, 11003 in the headless probe; the LGT build buys through 11001 - and
//! the server is long gone, so it sits on `전송 중입니다..` until it gives up
//! with `네트워크 상태가 원활하지 않습니다`.
//!
//! What it writes is the record its LGT build buys with (see
//! [`crate::billing::lgt_local_tera_response`]) under its own carrier's tag,
//! with no length in front:
//!
//! ```text
//! [0..3]    "KTF"
//! [3]       1, the message
//! [4..54]   "테라(인증)" in EUC-KR, zero filled
//! [54..60]  "41000" and its terminator
//! ```
//!
//! and it reads the answer the LGT build reads: a little-endian `u32` length,
//! then the body - the message come back, and a status. Tried against the
//! title, the status picks the notice it draws:
//!
//! | status | notice |
//! |--------|--------|
//! | 1 | 인증이 완료되었습니다. |
//! | 2 | 인증이 확인되었습니다. |
//! | 3 | 인증에 실패하였습니다. |
//!
//! On 1 the title keeps the 인증서 and goes on to its menu, and a later run
//! starts there without dialling at all. After the record it writes one zero
//! byte, which asks for nothing.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "222.122.160.61";
const PORTS: core::ops::RangeInclusive<u16> = 11001..=11003;

const TAG: &[u8] = b"KTF";
const AUTHENTICATION: u8 = 0x01;
const AUTHENTICATION_SIZE: usize = 60;

/// The status the title reads as a new 인증서 issued.
const ISSUED: u8 = 1;

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

        if self.pending[TAG.len()] != AUTHENTICATION {
            tracing::info!("tera: no reply shaped for {} bytes {:02x?}", self.pending.len(), self.pending);
            self.pending.clear();
            return;
        }

        if self.pending.len() < AUTHENTICATION_SIZE {
            return;
        }

        self.pending.drain(..AUTHENTICATION_SIZE);

        let body = [AUTHENTICATION, ISSUED];
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
    fn other_messages_are_left_unanswered() {
        let mut connection = TeraConnection::default();
        let mut record = authentication();
        record[3] = 0x14;
        connection.write(&record);
        assert!(!connection.readable());
    }
}
