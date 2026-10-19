//! The server SBS포트리스4's KTF build logs in at.
//!
//! The title (KTF `01042CDD`) shows its `-게임인증-` notice and the SMS consent
//! on a first run, then opens a plain socket to `211.200.30.157:5600` - the
//! binary also carries `211.200.30.154`, the same server's other address - and
//! sat on its connecting box while the connect to the dead server never
//! completed.
//!
//! Every message either way opens with its code in three ASCII digits. The
//! login the title writes is code 136, 49 bytes:
//!
//! ```text
//! [0..3]    "136"
//! [3..14]   the subscriber number, "01046119269"
//! [14..22]  the AID, "01042CDD"
//! [22..42]  the handset model, zero filled
//! [42..49]  "1102KTF"
//! ```
//!
//! The receiver (`0x123cfe`) `atoi`s the first three bytes of what it has
//! buffered, asks `0x123c3c` how long a message with that code is, and once
//! that much has arrived dispatches codes 200 to 243 through a jump table.
//!
//! Byte `[0x2a]` of the login is the answer the player gave the SMS consent
//! box, and the box's `OK` (`0x114de0`) is what sends it; the title then waits
//! to come back to the box it left. Code 236 is what moves it on: its handler
//! (`0x124c12`) sets byte 5 of `portriss4.sav`'s header - the 인증 a later run
//! starts past - copies the subscriber number into the header, writes the save
//! (`0x123244`) and raises box 2, `게임인증이 완료 되었습니다`, whose `OK` goes
//! to the title screen. Four bytes long, the fourth read by nothing.
//!
//! Not 242, `RES_NGC_LOGIN`: its zero reloads the save and leaves the consent
//! box up for good, anything else raises an empty box 100. Nor 241,
//! `RES_ALIAS`, which registers a nickname and jumps to a state whose pictures
//! were never loaded.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

const HOSTS: [&str; 2] = ["211.200.30.157", "211.200.30.154"];
const PORT: u16 = 5600;

/// The login's code, and the code of the answer that completes the 인증.
const LOGIN: &[u8; 3] = b"136";
const AUTHENTICATED: &[u8; 4] = b"236\x00";

/// Answers SBS포트리스4's login server.
pub struct FortressEndpoint;

impl LocalEndpoint for FortressEndpoint {
    fn name(&self) -> &str {
        "fortress4(211.200.30.157:5600)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && HOSTS.contains(&host) && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(FortressConnection::default())
    }
}

#[derive(Default)]
struct FortressConnection {
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl LocalConnection for FortressConnection {
    // A write to a local connection is always taken whole, so the title has no
    // reason to split a message, and each write is read as one.
    fn write(&mut self, bytes: &[u8]) {
        if bytes.starts_with(LOGIN) {
            self.outgoing.extend_from_slice(AUTHENTICATED);
            return;
        }

        tracing::info!("fortress4: no reply shaped for {} bytes {:02x?}", bytes.len(), bytes);
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

    use super::{FortressConnection, FortressEndpoint};
    use crate::local_network::{LocalConnection, LocalEndpoint, LocalRead};

    /// The login the title wrote in the probe.
    const LOGIN: &[u8] = b"1360104611926901042CDDEmulator\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x001102KTF";

    fn read_all(connection: &mut FortressConnection) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn only_the_login_server_is_answered() {
        assert!(FortressEndpoint.accepts("socket", "211.200.30.157", 5600));
        assert!(FortressEndpoint.accepts("socket", "211.200.30.154", 5600));
        assert!(!FortressEndpoint.accepts("socket", "211.200.30.157", 5601));
        assert!(!FortressEndpoint.accepts("socket", "211.200.30.158", 5600));
    }

    #[test]
    fn the_login_is_answered_236() {
        assert_eq!(LOGIN.len(), 49);

        let mut connection = FortressConnection::default();
        connection.write(LOGIN);
        assert!(connection.readable());
        assert_eq!(read_all(&mut connection), b"236\x00");

        connection.write(b"999");
        assert!(!connection.readable());
    }
}
