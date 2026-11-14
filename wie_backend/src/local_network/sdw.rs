//! SD세계대전's patch server.
//!
//! The title (KTF `0103CCFA`) dials `211.174.60.126:3101` as it starts, to
//! fetch whatever data it has been patched with since, and the server is long
//! gone: the connect is taken and nothing comes back, so it sits on its
//! `0kb/0kb` progress box for good.
//!
//! Every message either way is `"SDWW"`, a little-endian `u16` counting what
//! follows, and that much body, which opens with the message. The receive at
//! `0x10c7f0` checks the magic and the length and hands the body to the
//! dispatcher at `0x125060`, which jumps on its first byte through the table
//! at `0x1414fc`. The title writes three of them on its way in:
//!
//! - `0x01`, the login: the subscriber's number and `KTF_QVGA`, each behind
//!   its length. Its answer (`0x122608`) is a little-endian `u32`, the size of
//!   the patch, held against the room the handset has left: more than that is
//!   `단말기에 남은 파일용량이 부족합니다`, anything else goes on to ask for it.
//! - `0x00`, asking for the patch. Its answer (`0x122500`) is a run of items,
//!   each a type byte and what that type carries - 0 a name, 1 a run of
//!   bytes, 2 a file's record - read until the body ends, and then it asks
//!   again. Type 3 says the patch is all there and 4 that it failed
//!   (`패치를 다운로드 받는 도중 에러가 발생했습니다`).
//! - `0x02`, done. Its answer (`0x1226ac`) reads nothing and shows
//!   `모든 다운로드가 완료되었습니다. 정상적으로 게임을 시작합니다.`, after which
//!   the title goes to its menu.
//!
//! The data the title was released with is all in its package, so there is no
//! patch to give: the login is answered with a size of nothing, the request
//! for it with type 3 straight away, and the last with itself. What it writes
//! after that (`0x41`, `0x42`) is not waited on.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOST: &str = "211.174.60.126";
const PORT: u16 = 3101;

const MAGIC: &[u8] = b"SDWW";
/// The magic and the length behind it.
const HEAD: usize = 6;

const PATCH: u8 = 0x00;
const LOGIN: u8 = 0x01;
const DONE: u8 = 0x02;

/// The item that says the patch is all there.
const PATCH_COMPLETE: u8 = 3;

/// Answers SD세계대전's patch server.
pub struct SdwEndpoint;

impl LocalEndpoint for SdwEndpoint {
    fn name(&self) -> &str {
        "sdw(211.174.60.126:3101)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(SdwConnection::default())
    }
}

#[derive(Default)]
struct SdwConnection {
    /// What the title has written that is not yet a whole message.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

fn message(body: &[u8]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(body.len() as u16).to_le_bytes());
    out.extend_from_slice(body);
    out
}

impl LocalConnection for SdwConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        while self.pending.len() >= HEAD {
            if !self.pending.starts_with(MAGIC) {
                tracing::info!("sdw: no message in {} bytes {:02x?}", self.pending.len(), self.pending);
                self.pending.clear();
                return;
            }

            let length = u16::from_le_bytes([self.pending[4], self.pending[5]]) as usize;
            if self.pending.len() < HEAD + length {
                return;
            }

            let frame: Vec<u8> = self.pending.drain(..HEAD + length).collect();
            let answer: &[u8] = match frame.get(HEAD) {
                Some(&LOGIN) => &[LOGIN, 0, 0, 0, 0],
                Some(&PATCH) => &[PATCH, PATCH_COMPLETE],
                Some(&DONE) => &[DONE],
                _ => continue,
            };
            self.outgoing.extend_from_slice(&message(answer));
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

    use super::{SdwConnection, SdwEndpoint, message};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    fn read_all(connection: &mut SdwConnection) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn only_the_patch_server_is_answered() {
        assert!(SdwEndpoint.accepts("socket", "211.174.60.126", 3101));
        assert!(!SdwEndpoint.accepts("socket", "211.174.60.126", 3102));
        assert!(!SdwEndpoint.accepts("socket", "211.174.60.127", 3101));
    }

    #[test]
    fn the_title_is_let_in_with_nothing_to_patch() {
        let mut connection = SdwConnection::default();

        // The login as the title wrote it, in two pieces, and the request for
        // the patch behind it in the same write.
        let mut login = vec![1, 11];
        login.extend_from_slice(b"01046119269");
        login.push(8);
        login.extend_from_slice(b"KTF_QVGA");
        let mut written = message(&login);
        written.extend_from_slice(&message(&[0]));

        connection.write(&written[..10]);
        assert!(!connection.readable());
        connection.write(&written[10..]);

        let mut expected = message(&[1, 0, 0, 0, 0]);
        expected.extend_from_slice(&message(&[0, 3]));
        assert_eq!(read_all(&mut connection), expected);

        connection.write(&message(&[2]));
        assert_eq!(read_all(&mut connection), b"SDWW\x01\x00\x02");

        // What it says after that is not waited on.
        connection.write(&message(&[0x42]));
        assert!(!connection.readable());
    }
}
