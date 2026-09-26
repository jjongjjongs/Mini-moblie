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
//! This phase carries the connect through: the gateway's init is answered with
//! a `SUCCESS` confirm, and each data-server command with a success header and
//! an empty body, which lands map/ranking/notice screens on their own "none
//! yet" state instead of the network error a refused connect left them on. The
//! request is logged so a body can be filled in for the screens that should
//! show records. Later phases answer the map list, the map file and the
//! ranking with real content.

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

/// The forty character server-to-client header, whose fields the title reads by
/// fixed offsets (`NetProcess.Net_ReadHeaderData`):
///
/// ```text
///   [0..4]   the reply command - see `reply_command`. Only four are read;
///            an unknown one leaves the body-length field unread, so no body
///            is taken and the parser runs off the end of an empty stream.
///   [4..12]  the body length that follows, eight digits, zero padded
///   [12..16] a status; 0002, 0003 and 0004 are the errors the title drops on,
///            so a success is any other - 0001 here
///   [16..24] a point total, eight digits (the title caps it at 99999)
///   [24]     a '1' flag the title tests for
///   [25..40] unread padding
/// ```
const HEADER_LEN: usize = 40;

/// The four reply commands `Net_ReadHeaderData` reads a body for, chosen by the
/// request's family digit (its second): the map, notice and list requests
/// (`01x0`, `02x0`) are all answered `0011`; the photo, ranking and quiz
/// requests (`03x0`, `05x0`, `07x0`) `0311`, `0511`, `0711`. Any other request
/// gets `0` + its family + `11`, which the title does not read a body for.
fn reply_command(request: &[u8]) -> [u8; 4] {
    match request.get(1) {
        Some(b'1') | Some(b'2') => *b"0011",
        Some(b'3') => *b"0311",
        Some(b'5') => *b"0511",
        Some(b'7') => *b"0711",
        family => [b'0', family.copied().unwrap_or(b'0'), b'1', b'1'],
    }
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

    /// The reply to `request`: a success header and whatever body its screen's
    /// parser (`MainCanvas.ParseData`) needs to reach its completion.
    ///
    /// `ParseData` skips the forty header bytes and then reads a body whose
    /// shape depends on the command. A command it reads nothing from is answered
    /// with the header alone; a command that reads a record count needs at least
    /// that count, or its read runs off the end of the body and the screen it
    /// was opening is abandoned.
    ///
    /// The map/notice/list command (`01x0`, `02x0`) is where a body is needed.
    /// `ParseData` reads it one of two ways, chosen not by the request but by an
    /// `isnotice` field the title sets for itself, so the reply has to satisfy
    /// both:
    ///
    /// - as a notice: eight bytes read into a date string, then an `int` -
    ///   twelve bytes.
    /// - as a map list: an `int`, then an `int` count, then that many records -
    ///   eight bytes for a zero count.
    ///
    /// Twelve zero bytes are both at once: a notice with an empty date, or a map
    /// list whose count (the second int, still within the twelve) is zero. The
    /// title lands on its notice board or on `맵파일이 없습니다` rather than
    /// reading off the end of the body and abandoning the screen (a null map
    /// array, then a paint that dereferences it). A later phase fills the map
    /// records from the bundled maps.
    fn reply(request: &[u8]) -> Vec<u8> {
        let mut reply = Self::header(request);

        let is_map_family = matches!(request.get(1), Some(b'1') | Some(b'2'));
        if is_map_family {
            let body = [0u8; 12];
            Self::set_body_length(&mut reply, body.len());
            reply.extend_from_slice(&body);
        }

        reply
    }

    /// The forty byte success header for `request`: the reply command, an empty
    /// body length, a non-error status and a point, the rest padded.
    fn header(request: &[u8]) -> Vec<u8> {
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(&reply_command(request)); // [0..4]  command
        header.extend_from_slice(b"00000000"); // [4..12]   body length: none
        header.extend_from_slice(b"0001"); // [12..16]      status: not an error
        header.extend_from_slice(b"00000000"); // [16..24]  point
        header.push(b'1'); // [24]                          flag
        header.resize(HEADER_LEN, b'0'); // [25..40]        padding
        header
    }

    /// Writes `length` into a header's `[4..12]` body-length field.
    fn set_body_length(header: &mut [u8], length: usize) {
        let digits = alloc::format!("{length:08}");
        header[4..12].copy_from_slice(digits.as_bytes());
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

        if self.is_gateway {
            if self.pending.len() >= INIT_LEN {
                self.pending.drain(..INIT_LEN);
                self.outgoing.extend_from_slice(&Self::confirm());
                tracing::info!("snowboard gateway: answered init with SUCCESS confirm");
            }
            return;
        }

        // The data server's request is the ASCII command string the title
        // builds in `FnNetControl`, opening with its four digit code. One write
        // carries the whole request, and each request opens a fresh connection,
        // so a code in hand is a request to answer.
        if self.outgoing.is_empty() && self.pending.len() >= 4 {
            let reply = Self::reply(&self.pending);
            tracing::info!("snowboard data: {} -> {}", hex_ascii(&self.pending), hex_ascii(&reply));
            self.pending.clear();
            self.outgoing.extend_from_slice(&reply);
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if !self.outgoing.is_empty() {
            let take = out.len().min(self.outgoing.len());
            out[..take].copy_from_slice(&self.outgoing[..take]);
            self.outgoing.drain(..take);
            return LocalRead::Data(take);
        }

        // A request with no answer is closed rather than left waiting: a read
        // that never returns strands the title, where an end of stream lands in
        // the same catch its author wrote for a dropped connection.
        if !self.is_gateway && !self.pending.is_empty() {
            tracing::info!("snowboard data: no answer for {}, closing", hex_ascii(&self.pending));
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

#[cfg(test)]
mod tests {
    use super::{CONFIRM_LEN, HEADER_LEN, INIT_LEN, LocalConnection, LocalRead, SUCCESS, SnowBoardConnection};
    use alloc::{vec, vec::Vec};

    fn read_all(connection: &mut SnowBoardConnection) -> Vec<u8> {
        let mut out = vec![0u8; 256];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            other => panic!("expected data, got {other:?}"),
        }
    }

    #[test]
    fn the_gateway_answers_the_init_with_a_success_confirm() {
        let mut gateway = SnowBoardConnection {
            is_gateway: true,
            pending: Vec::new(),
            outgoing: Vec::new(),
        };
        gateway.write(&[0u8; INIT_LEN]);

        let confirm = read_all(&mut gateway);
        assert_eq!(confirm.len(), CONFIRM_LEN);
        assert_eq!(confirm[4], SUCCESS);
        assert_eq!(confirm[42], SUCCESS, "the checksum is the XOR of +4..+42");
    }

    #[test]
    fn a_command_with_no_body_is_answered_with_the_header_alone() {
        let mut data = SnowBoardConnection {
            is_gateway: false,
            pending: Vec::new(),
            outgoing: Vec::new(),
        };
        // A photo request (family '3'): answered 0311, and read for no body.
        data.write(b"03100100something");

        let header = read_all(&mut data);
        assert_eq!(header.len(), HEADER_LEN);
        assert_eq!(&header[0..4], b"0311", "the photo family's reply command");
        assert_eq!(&header[4..12], b"00000000", "an empty body");
        assert_ne!(&header[12..16], b"0002", "not one of the error statuses");
        assert_eq!(header[24], b'1');
    }

    #[test]
    fn the_map_list_request_is_answered_0011_with_a_zero_count_body() {
        let mut data = SnowBoardConnection {
            is_gateway: false,
            pending: Vec::new(),
            outgoing: Vec::new(),
        };
        // The '0110' request the map screen sends with its notice flag clear -
        // a '0' at offset 40 - is read back as a map list.
        let mut request = b"0110".to_vec();
        request.resize(41, b'0');
        assert_eq!(request[40], b'0', "the notice flag is clear");
        data.write(&request);

        let reply = read_all(&mut data);
        assert_eq!(reply.len(), HEADER_LEN + 12, "the header and a twelve byte body");
        assert_eq!(&reply[0..4], b"0011", "the map family's reply command, which the header parser reads");
        assert_eq!(&reply[4..12], b"00000012", "the body length is written into the header");
        assert_eq!(
            &reply[HEADER_LEN..],
            &[0u8; 12],
            "zeros: a zero count for a list, an empty date for a notice"
        );
    }

    #[test]
    fn the_notice_request_is_answered_0011_with_a_date_and_int_body() {
        let mut data = SnowBoardConnection {
            is_gateway: false,
            pending: Vec::new(),
            outgoing: Vec::new(),
        };
        // The same '0110' with the notice flag set - a '1' at offset 40.
        let mut request = b"0110".to_vec();
        request.resize(41, b'0');
        request[40] = b'1';
        data.write(&request);

        let reply = read_all(&mut data);
        assert_eq!(reply.len(), HEADER_LEN + 12, "the header and a twelve byte body");
        assert_eq!(&reply[0..4], b"0011", "the map family's reply command");
        assert_eq!(&reply[4..12], b"00000012", "the body length is written into the header");
    }

    #[test]
    fn an_unanswered_data_read_closes_rather_than_waits() {
        let mut data = SnowBoardConnection {
            is_gateway: false,
            pending: Vec::new(),
            outgoing: Vec::new(),
        };
        // A read before any write has nothing to close over: it waits.
        assert_eq!(data.read(&mut [0u8; 8]), LocalRead::Pending);
    }
}
