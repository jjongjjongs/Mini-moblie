//! The carrier relay middleware some KTF titles dial through, answered in
//! process.
//!
//! A title that lists the dependency `01039AD6` (`com.vdigm.billcom.relay`)
//! opens `socket://wipiwicgsfg.magicn.com:17096` and speaks a two-layer
//! protocol over it. The outer layer is a relay envelope; inside it is the
//! title's own slot-service conversation. The gateway has been gone for years,
//! so with nothing answering the title reports `서버와의 접속이 끊어졌습니다`
//! and returns to the menu. This answers it so 오즈-천공의 기사단's 새로하기
//! reaches its character-name entry, which is where the recovered conversation
//! ends.
//!
//! # The relay envelope
//!
//! `[0x00][u32be size][u16be ext_len][ext][payload]`, `size` covering the whole
//! frame including the seven-byte prefix. Extensions are opaque and empty in
//! the answers here.
//!
//! # The slot-service conversation (inside the payload)
//!
//! Each message is `[u32be length][u32be kind][u32be command][body]`, `length`
//! covering the whole message. The conversation walks a fixed sequence, and a
//! reply carries the request's own kind and command:
//!
//! - `kind 1, command 1000, body [0x1e]` - the login, answered `[0x00]`.
//! - `kind 0, command 10, empty` - a poll once logged in, answered empty.
//! - `kind 5, command 1400, [len][identity]` - the identity, answered empty.
//! - `kind 5, command 1410, [slot]` - the chosen slot, answered with a
//!   length-prefixed notice ("이 기기에 슬롯을 생성합니다.").
//! - `kind 5, command 1420, empty` - answered with the "비용은 청구되지
//!   않습니다." notice, the free-of-charge line the title shows.
//! - `kind 5, command 1430, empty` - answered with the slot's display label
//!   (" 로컬<n>") and an eight-byte trailer the local service leaves zero.
//!
//! Recovered from a working WIPI player's `slotRelay`; the display strings are
//! its EUC-KR bytes.
//!
//! # 월드접속
//!
//! The same gateway session carries 오즈's 월드접속, which it opens in a mode
//! of its own (`network/d.a` = 4) and walks further than the slot service:
//!
//! - `kind 1, command 1010, [len]"1.0.0" 00 00` - the client's version,
//!   answered `kind 2, command 1140, [u16 len][text]`: the notice the title
//!   shows under `공지사항` before going on.
//! - `kind 4, command 1300, [character summary]` - sent once the notice is
//!   confirmed, answered with the server list: a count, then per server
//!   `[len][name][4-byte IP][u32 port][status]` (`network/a.e([BI)V`).
//!
//! # Where a session goes
//!
//! Every frame's extension is a tag-length-value block, and the relay library
//! names the server it wants in it - tag `0x13` the IP and `0x12` the port. The
//! gateway sessions name the title's own gateway; once a server is picked from
//! the list, the title opens a new session naming that server's address, and
//! [`KODAN_ADDRESS`] is the one this list hands out, answered by
//! [`super::kodan`].

use alloc::{boxed::Box, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead, kodan::KodanWorld};

/// The relay gateway this answers for.
const HOST: &str = "wipiwicgsfg.magicn.com";
const PORT: u16 = 17096;

/// The relay envelope's fixed prefix: a zero marker, a `u32be` size and a
/// `u16be` extension length.
const RELAY_PREFIX: usize = 7;

/// The slot message's fixed header: a `u32be` length, kind and command.
const SLOT_HEADER: usize = 12;

/// The world server the server list offers, as the relay extension names it.
/// It is never dialled - the relay library only passes it along, and the
/// session naming it is answered by [`KodanWorld`].
pub(super) const KODAN_ADDRESS: ([u8; 4], u16) = ([127, 0, 0, 2], 7000);

/// The extension tags naming the server a session is for.
const TAG_PORT: u8 = 0x12;
const TAG_IP: u8 = 0x13;

/// 월드접속's first request once logged in: the client's version.
const WORLD_VERSION: u32 = 1010;
/// The notice the gateway answers it with.
const NOTICE_KIND: u32 = 2;
const NOTICE: u32 = 1140;
/// The server list request and its answer.
const SERVER_LIST_KIND: u32 = 4;
const SERVER_LIST: u32 = 1300;
/// The status byte a listed server shows as `쾌적`.
const SERVER_IDLE: u8 = 0;

/// "코단 월드에 오신 것을 환영합니다." in EUC-KR.
pub(super) const MESSAGE_WELCOME: &[u8] = &[
    0xc4, 0xda, 0xb4, 0xdc, 0x20, 0xbf, 0xf9, 0xb5, 0xe5, 0xbf, 0xa1, 0x20, 0xbf, 0xc0, 0xbd, 0xc5, 0x20, 0xb0, 0xcd, 0xc0, 0xbb, 0x20, 0xc8, 0xaf,
    0xbf, 0xb5, 0xc7, 0xd5, 0xb4, 0xcf, 0xb4, 0xd9, 0x2e,
];
/// "코단" in EUC-KR, the one server's name.
const SERVER_NAME: &[u8] = &[0xc4, 0xda, 0xb4, 0xdc];

/// A frame larger than this is refused rather than buffered without bound.
const MAX_FRAME: usize = 1 << 20;

/// "이 기기에 슬롯을 생성합니다." in EUC-KR.
const MESSAGE_CREATING: &[u8] = &[
    0xc0, 0xcc, 0x20, 0xb1, 0xe2, 0xb1, 0xe2, 0xbf, 0xa1, 0x20, 0xbd, 0xbd, 0xb7, 0xd4, 0xc0, 0xbb, 0x20, 0xbb, 0xfd, 0xbc, 0xba, 0xc7, 0xd5, 0xb4,
    0xcf, 0xb4, 0xd9, 0x2e,
];

/// "비용은 청구되지 않습니다." in EUC-KR.
const MESSAGE_FREE: &[u8] = &[
    0xba, 0xf1, 0xbf, 0xeb, 0xc0, 0xba, 0x20, 0xc3, 0xbb, 0xb1, 0xb8, 0xb5, 0xc7, 0xc1, 0xf6, 0x20, 0xbe, 0xca, 0xbd, 0xc0, 0xb4, 0xcf, 0xb4, 0xd9,
    0x2e,
];

/// " 로컬" in EUC-KR, the slot label's prefix; the slot number follows in ASCII.
const LABEL_PREFIX: &[u8] = &[0x20, 0xb7, 0xce, 0xc4, 0xc3];

/// Answers the carrier relay middleware.
pub struct RelayEndpoint;

impl LocalEndpoint for RelayEndpoint {
    fn name(&self) -> &str {
        "relay(wipiwicgsfg.magicn.com:17096)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == HOST && port == PORT
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(RelayConnection::default())
    }
}

#[derive(Default)]
struct RelayConnection {
    /// What the title has written that is not yet a whole relay frame.
    request: Vec<u8>,
    /// What is left to hand back.
    outgoing: Vec<u8>,
    /// Which conversation this session is, decided by its first frame.
    service: Option<Service>,
    /// A malformed or unsupported request drops the connection.
    closed: bool,
}

/// The conversation a session carries, by the server its extension names.
enum Service {
    Gateway(SlotRelay),
    World(KodanWorld),
}

impl Service {
    fn for_extension(extension: &[u8]) -> Self {
        if target(extension) == Some(KODAN_ADDRESS) {
            Service::World(KodanWorld::default())
        } else {
            Service::Gateway(SlotRelay::default())
        }
    }

    /// The messages answering one request, or `None` to drop the session.
    fn respond(&mut self, payload: &[u8]) -> Option<Vec<Vec<u8>>> {
        match self {
            Service::Gateway(slot) => slot.respond(payload).map(|reply| vec![reply]),
            Service::World(world) => world.respond(payload),
        }
    }
}

/// The server an extension names, from its IP and port tags.
fn target(extension: &[u8]) -> Option<([u8; 4], u16)> {
    let mut ip = None;
    let mut port = None;
    let mut rest = extension;
    while let [tag, len, tail @ ..] = rest {
        let len = *len as usize;
        if tail.len() < len {
            break;
        }
        let value = &tail[..len];
        match (*tag, value) {
            (TAG_IP, &[a, b, c, d]) => ip = Some([a, b, c, d]),
            (TAG_PORT, &[hi, lo]) => port = Some(u16::from_be_bytes([hi, lo])),
            _ => {}
        }
        rest = &tail[len..];
    }

    Some((ip?, port?))
}

/// The recovered slot-service state machine.
#[derive(Default)]
pub(super) struct SlotRelay {
    phase: u8,
    slot: u8,
    label: Vec<u8>,
}

/// A relay frame's extension TLVs and the slot message it carries.
type Frame = (Vec<u8>, Vec<u8>);

impl RelayConnection {
    /// Takes one relay frame's payload off the front of `request`. `Ok(None)`
    /// when a whole frame has not arrived; `Err` when the framing is not this
    /// one, which drops the connection.
    fn take_payload(&mut self) -> Result<Option<Frame>, ()> {
        let data = &self.request;
        if data.is_empty() {
            return Ok(None);
        }
        if data[0] != 0 {
            return Err(());
        }
        if data.len() < 5 {
            return Ok(None);
        }
        let size = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
        if !(RELAY_PREFIX..=MAX_FRAME).contains(&size) {
            return Err(());
        }
        if data.len() < RELAY_PREFIX {
            return Ok(None);
        }
        let ext_len = u16::from_be_bytes([data[5], data[6]]) as usize;
        if ext_len > size - RELAY_PREFIX {
            return Err(());
        }
        if data.len() < size {
            return Ok(None);
        }

        let extension = data[RELAY_PREFIX..RELAY_PREFIX + ext_len].to_vec();
        let payload = data[RELAY_PREFIX + ext_len..size].to_vec();
        self.request.drain(..size);
        Ok(Some((extension, payload)))
    }

    /// Wraps `payload` in a relay envelope with no extensions.
    fn envelope(payload: &[u8]) -> Vec<u8> {
        let size = RELAY_PREFIX + payload.len();
        let mut frame = vec![0u8; size];
        frame[1..5].copy_from_slice(&(size as u32).to_be_bytes());
        // frame[5..7] extension length stays zero.
        frame[RELAY_PREFIX..].copy_from_slice(payload);
        frame
    }
}

impl SlotRelay {
    /// A slot message carrying `kind`, `command` and `body`.
    pub(super) fn message(kind: u32, command: u32, body: &[u8]) -> Vec<u8> {
        let length = SLOT_HEADER + body.len();
        let mut message = vec![0u8; length];
        message[0..4].copy_from_slice(&(length as u32).to_be_bytes());
        message[4..8].copy_from_slice(&kind.to_be_bytes());
        message[8..12].copy_from_slice(&command.to_be_bytes());
        message[12..].copy_from_slice(body);
        message
    }

    /// The reply to one slot-service `payload`, or `None` when it is malformed
    /// or unsupported in the current phase (which drops the connection).
    fn respond(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        if payload.len() < SLOT_HEADER {
            return None;
        }
        let length = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        if length != payload.len() {
            return None;
        }
        let kind = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let command = u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]);
        let body = &payload[12..];

        // The keepalive poll, once logged in.
        if kind == 0 && command == 10 && body.is_empty() && self.phase > 0 {
            return Some(Self::message(kind, command, &[]));
        }

        match (kind, command, self.phase) {
            // Login.
            (1, 1000, 0) if body == [30] => {
                self.phase = 1;
                Some(Self::message(kind, command, &[0]))
            }
            // 월드접속's version, `[len]"1.0.0"` and two zero bytes: the notice.
            (1, WORLD_VERSION, 1) if body.len() > 1 && (body[0] as usize) < body.len() => {
                let mut notice = Vec::from((MESSAGE_WELCOME.len() as u16).to_be_bytes());
                notice.extend_from_slice(MESSAGE_WELCOME);
                Some(Self::message(NOTICE_KIND, NOTICE, &notice))
            }
            // The server list, asked for once the notice is confirmed: the one
            // server this answers for.
            (SERVER_LIST_KIND, SERVER_LIST, 1) => {
                let mut list = vec![1, SERVER_NAME.len() as u8];
                list.extend_from_slice(SERVER_NAME);
                list.extend_from_slice(&KODAN_ADDRESS.0);
                list.extend_from_slice(&(KODAN_ADDRESS.1 as u32).to_be_bytes());
                list.push(SERVER_IDLE);
                Some(Self::message(kind, command, &list))
            }
            // Identity: a length-prefixed field.
            (5, 1400, 1) if body.len() > 1 && body[0] as usize == body.len() - 1 && body[0] <= 127 => {
                self.phase = 2;
                Some(Self::message(kind, command, &[]))
            }
            // The chosen slot.
            (5, 1410, 2) if body.len() == 1 && body[0] <= 127 => {
                self.slot = body[0];
                self.phase = 3;
                let mut reply = vec![1, MESSAGE_CREATING.len() as u8];
                reply.extend_from_slice(MESSAGE_CREATING);
                Some(Self::message(kind, command, &reply))
            }
            // The free-of-charge notice.
            (5, 1420, 3) if body.is_empty() => {
                self.phase = 4;
                let mut reply = vec![MESSAGE_FREE.len() as u8];
                reply.extend_from_slice(MESSAGE_FREE);
                Some(Self::message(kind, command, &reply))
            }
            // The slot's display label.
            (5, 1430, 4) | (5, 1430, 5) if body.is_empty() => {
                if self.phase == 4 {
                    self.label = LABEL_PREFIX.to_vec();
                    push_decimal(&mut self.label, self.slot as u32 + 1);
                    self.phase = 5;
                }
                let mut reply = vec![1, self.label.len() as u8];
                reply.extend_from_slice(&self.label);
                reply.extend_from_slice(&[0u8; 8]);
                Some(Self::message(kind, command, &reply))
            }
            _ => None,
        }
    }
}

/// Appends `value` as ASCII decimal digits (EUC-KR's digits are ASCII).
fn push_decimal(out: &mut Vec<u8>, value: u32) {
    if value >= 10 {
        push_decimal(out, value / 10);
    }
    out.push(b'0' + (value % 10) as u8);
}

impl LocalConnection for RelayConnection {
    fn write(&mut self, bytes: &[u8]) {
        if self.closed {
            return;
        }
        self.request.extend_from_slice(bytes);

        loop {
            match self.take_payload() {
                Ok(Some((extension, payload))) => match self.service.get_or_insert_with(|| Service::for_extension(&extension)).respond(&payload) {
                    Some(replies) => {
                        for reply in replies {
                            tracing::info!("relay {HOST}:{PORT}: {} -> {}", hex(&payload), hex(&reply));
                            self.outgoing.extend_from_slice(&Self::envelope(&reply));
                        }
                    }
                    None => {
                        tracing::warn!("relay {HOST}:{PORT}: no reply for {}, closing", hex(&payload));
                        self.closed = true;
                        self.request.clear();
                        break;
                    }
                },
                Ok(None) => break,
                Err(()) => {
                    tracing::warn!("relay {HOST}:{PORT}: malformed frame, closing");
                    self.closed = true;
                    self.request.clear();
                    break;
                }
            }
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if !self.outgoing.is_empty() {
            let take = out.len().min(self.outgoing.len());
            out[..take].copy_from_slice(&self.outgoing[..take]);
            self.outgoing.drain(..take);
            return LocalRead::Data(take);
        }

        if self.closed { LocalRead::Closed } else { LocalRead::Pending }
    }

    fn readable(&self) -> bool {
        !self.outgoing.is_empty()
    }
}

/// A compact hex rendering of a captured buffer, for pinning the framing.
pub(super) fn hex(bytes: &[u8]) -> alloc::string::String {
    use core::fmt::Write;

    let mut out = alloc::string::String::new();
    for &byte in bytes.iter().take(64) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{LocalConnection, LocalRead, RelayConnection};
    use alloc::{vec, vec::Vec};

    fn slot_frame(kind: u32, command: u32, body: &[u8]) -> Vec<u8> {
        let message = super::SlotRelay::message(kind, command, body);
        RelayConnection::envelope(&message)
    }

    fn read_all(connection: &mut RelayConnection) -> Vec<u8> {
        let mut out = vec![0u8; 256];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            other => panic!("expected data, got {other:?}"),
        }
    }

    /// The reply's envelope and slot message carry the request's kind and
    /// command; a login is answered with a zero status.
    #[test]
    fn the_login_is_answered() {
        let mut connection = RelayConnection::default();
        connection.write(&slot_frame(1, 1000, &[30]));

        let reply = read_all(&mut connection);
        // Envelope: marker 0, size, ext 0; then a slot message kind 1 command 1000 body [0].
        assert_eq!(reply[0], 0);
        assert_eq!(&reply[7..11], &[0, 0, 0, 0x0d], "slot message length 13");
        assert_eq!(&reply[11..15], &[0, 0, 0, 1], "kind 1");
        assert_eq!(&reply[15..19], &[0, 0, 0x03, 0xe8], "command 1000");
        assert_eq!(reply[19], 0, "zero status");
    }

    /// The full creation walk reaches the label reply, which carries " 로컬1".
    #[test]
    fn the_creation_walk_reaches_the_label() {
        let mut connection = RelayConnection::default();
        connection.write(&slot_frame(1, 1000, &[30]));
        let _ = read_all(&mut connection);
        connection.write(&slot_frame(5, 1400, &[3, b'a', b'b', b'c']));
        let _ = read_all(&mut connection);
        connection.write(&slot_frame(5, 1410, &[0]));
        let _ = read_all(&mut connection);
        connection.write(&slot_frame(5, 1420, &[]));
        let _ = read_all(&mut connection);
        connection.write(&slot_frame(5, 1430, &[]));

        let reply = read_all(&mut connection);
        // The label body opens [1][len]; the label is " 로컬1" - the prefix bytes
        // then ASCII '1'.
        let body = &reply[19..];
        assert_eq!(body[0], 1);
        let label_len = body[1] as usize;
        assert_eq!(&body[2..2 + label_len], &[0x20, 0xb7, 0xce, 0xc4, 0xc3, b'1']);
    }

    /// 월드접속's version is answered with the notice under kind 2, and the
    /// server list that follows offers the one world server.
    #[test]
    fn the_world_gateway_answers_with_a_notice_and_a_server() {
        let mut connection = RelayConnection::default();
        connection.write(&slot_frame(1, 1000, &[30]));
        let _ = read_all(&mut connection);

        connection.write(&slot_frame(1, 1010, &[5, b'1', b'.', b'0', b'.', b'0', 0, 0]));
        let notice = read_all(&mut connection);
        assert_eq!(&notice[11..19], &[0, 0, 0, 2, 0, 0, 0x04, 0x74], "kind 2 command 1140");

        connection.write(&slot_frame(4, 1300, &[0]));
        let list = read_all(&mut connection);
        let body = &list[19..];
        assert_eq!(body[0], 1, "one server");
        let name = body[1] as usize;
        assert_eq!(&body[2 + name..6 + name], &super::KODAN_ADDRESS.0);
        assert_eq!(&body[6 + name..10 + name], &(super::KODAN_ADDRESS.1 as u32).to_be_bytes());
    }

    /// A session whose extension names the listed server is the world's.
    #[test]
    fn a_session_naming_the_world_server_is_the_worlds() {
        let (ip, port) = super::KODAN_ADDRESS;
        let mut extension = vec![0x06, 4, 1, 3, 0xcd, 0x8a, 0x12, 2];
        extension.extend_from_slice(&port.to_be_bytes());
        extension.extend_from_slice(&[0x13, 4]);
        extension.extend_from_slice(&ip);
        assert_eq!(super::target(&extension), Some(super::KODAN_ADDRESS));

        let message = super::SlotRelay::message(1, 1000, &[30]);
        let mut frame = vec![0u8; 7];
        frame.extend_from_slice(&extension);
        frame.extend_from_slice(&message);
        let size = frame.len() as u32;
        frame[1..5].copy_from_slice(&size.to_be_bytes());
        frame[5..7].copy_from_slice(&(extension.len() as u16).to_be_bytes());

        let mut connection = RelayConnection::default();
        connection.write(&frame);
        let _ = read_all(&mut connection);
        // The world answers the version itself, under kind 1.
        connection.write(&slot_frame(1, 1010, &[5, b'1', b'.', b'0', b'.', b'0', 0, 0]));
        let answer = read_all(&mut connection);
        assert_eq!(&answer[11..19], &[0, 0, 0, 1, 0, 0, 0x03, 0xf2]);
    }

    /// An unsupported request drops the connection rather than answering.
    #[test]
    fn an_unsupported_request_closes() {
        let mut connection = RelayConnection::default();
        connection.write(&slot_frame(9, 9, &[]));
        assert_eq!(connection.read(&mut [0u8; 8]), LocalRead::Closed);
    }
}
