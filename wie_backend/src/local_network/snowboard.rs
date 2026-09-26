//! The server 엑스피드스노보드 (X-peed Snowboard 2006) asks before its online
//! menus will work.
//!
//! The title plays its bundled slopes offline - `res/map/map*.dat` ride in the
//! jar and `SLOPE` reads them straight - but its title screen opens a socket to
//! its own ranking/map server the moment it starts, and its download, ranking,
//! notice and quiz menus each open one too. That server
//! (`211.234.104.44:10279`, with a carrier gateway at `203.236.40.219:6200`)
//! has been gone for years, so [`Connector`] refuses the connect and the title
//! shows its own `네트워크 접속 에러`.
//!
//! Refusing is the safe answer while nothing here speaks the protocol, but it
//! is an answer the title only recovers from - it never reaches its online
//! menus. A local endpoint answers in process instead.
//!
//! # The protocol (as read from `NetProcess`)
//!
//! Two kinds of connection, chosen by the title's `m_NetState`:
//!
//! - the **gateway** (`SERVER_URL`, `203.236.40.219:6200`) takes an 83 byte
//!   init packet - `"IJN\0"`, then the client id, jam id and subscriber number
//!   little-endian - and answers with a 43 byte confirm: four header bytes, a
//!   result byte at `+4` (`'0'` = `SUCCESS`), a thirty byte name, a six byte
//!   birth date, a gender byte and an XOR of `+4..+42` at `+42`.
//! - the **data server** (`211.234.104.44:10279`) carries the command traffic.
//!   Each server-to-client message opens with a forty character ASCII header -
//!   a four character command at `[0..4]`, a secondary code at `[12..16]`, an
//!   eight digit `point` at `[16..24]` - followed by a body whose length the
//!   header carries.
//!
//! # Phase 1
//!
//! This phase answers only the connect itself: the gateway's confirm as
//! `SUCCESS`, and the data server's traffic is logged so the command framing
//! can be pinned from the title's own writes. Later phases answer the map
//! list, the map file and the ranking.

use alloc::{boxed::Box, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// The gateway that authenticates, and the data server that carries commands.
/// The test server the title also names is answered the same way.
const ENDPOINTS: [(&str, u16); 3] = [
    ("211.234.104.44", 10279), // data server
    ("203.236.40.219", 6200),  // gateway
    ("203.236.40.205", 6200),  // test gateway
];

/// The init packet the gateway takes: `"IJN\0"` then the ids. Its length is
/// fixed, so a whole one has arrived once this many bytes are in hand.
const INIT_LEN: usize = 83;

/// The confirm the gateway answers with. `+4` is the result the title reads;
/// everything else it either trims to empty or never looks at.
const CONFIRM_LEN: usize = 43;

/// `NetProcess.SUCCESS`, the result byte that lets the title go on. It is the
/// ASCII digit `'0'`, not a zero byte.
const SUCCESS: u8 = b'0';

/// Answers 엑스피드스노보드's ranking/map server and its carrier gateway.
pub struct SnowBoardEndpoint;

impl LocalEndpoint for SnowBoardEndpoint {
    fn name(&self) -> &str {
        "snowboard(211.234.104.44:10279)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket"
            && ENDPOINTS
                .iter()
                .any(|&(endpoint_host, endpoint_port)| host == endpoint_host && port == endpoint_port)
    }

    fn open(&self, _scheme: &str, _host: &str, port: u16) -> Box<dyn LocalConnection> {
        Box::new(SnowBoardConnection {
            is_gateway: port == 6200,
            pending: Vec::new(),
            outgoing: Vec::new(),
        })
    }
}

struct SnowBoardConnection {
    /// The gateway takes the init packet; the data server takes commands. The
    /// two answer differently, so a connection remembers which it is.
    is_gateway: bool,
    /// What the title has written that has not yet been answered.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl SnowBoardConnection {
    /// The 43 byte confirm, `SUCCESS` and everything past the result left blank.
    /// The title reads the result at `+4` and trims the name and birth fields to
    /// empty, so a blank body is one it accepts.
    fn confirm() -> Vec<u8> {
        let mut packet = vec![0u8; CONFIRM_LEN];
        packet[4] = SUCCESS;
        // `+42` is an XOR of `+4..+42`; with the result the only non-zero byte
        // in that range, the checksum is the result itself.
        packet[42] = SUCCESS;
        packet
    }
}

impl LocalConnection for SnowBoardConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        tracing::debug!(
            "snowboard {} write: {} bytes now pending: {}",
            if self.is_gateway { "gateway" } else { "data" },
            self.pending.len(),
            hex_ascii(&self.pending),
        );

        if self.is_gateway && self.pending.len() >= INIT_LEN {
            self.pending.drain(..INIT_LEN);
            self.outgoing.extend_from_slice(&Self::confirm());
            tracing::info!("snowboard gateway: answered init with SUCCESS confirm");
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if !self.outgoing.is_empty() {
            let take = out.len().min(self.outgoing.len());
            out[..take].copy_from_slice(&self.outgoing[..take]);
            self.outgoing.drain(..take);
            return LocalRead::Data(take);
        }

        // The data server's command framing is being pinned from the write
        // logs. Until a command has an answer, close rather than leave the read
        // waiting: a read that never returns strands the title, where an end of
        // stream lands in the same catch its author wrote for a dropped
        // connection. The command the title wrote is captured in the log first.
        if !self.is_gateway && !self.pending.is_empty() {
            tracing::info!("snowboard data: no answer yet for {}, closing", hex_ascii(&self.pending));
            return LocalRead::Closed;
        }

        LocalRead::Pending
    }

    fn readable(&self) -> bool {
        !self.outgoing.is_empty()
    }
}

/// A compact hex+ASCII rendering of a captured buffer, for pinning the framing.
fn hex_ascii(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;

    let mut out = alloc::string::String::new();
    for &byte in bytes.iter().take(128) {
        let _ = write!(out, "{byte:02x}");
    }
    out.push(' ');
    out.push('|');
    for &byte in bytes.iter().take(128) {
        out.push(if (0x20..0x7f).contains(&byte) { byte as char } else { '.' });
    }
    out.push('|');
    out
}
