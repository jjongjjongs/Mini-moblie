//! Nexon Mobile's GXG gateway, which SKT builds of its titles reach their
//! servers through.
//!
//! 크아비엔비2011, 슈퍼액션히어로4, 메이플스토리 Live and 던파 귀검사편 all carry the
//! same `GXGNet` library. Where their LGT builds open a billing socket straight
//! to `211.115.203.30`, these look up `ngw.gxg.com` (or `ngph.gxg.com`) and dial
//! port `7300` of whatever it resolves to, then ask that gateway to carry them to
//! the server they want. The gateway has been gone for years and the name no
//! longer resolves, so a purchase connects to `255.255.255.255` and the shop
//! gives up on it.
//!
//! Every frame either way is a 25-byte header and a body that ends in `0x17`:
//!
//! ```text
//! [0..12]   "GXG.01.00.07" - or "GXG.01.00.04" from the older builds
//! [12..15]  the type - "IPC" to open, "DP?" for data, ...
//! [15..25]  the body's length in ASCII digits, zero-padded on the right
//! ```
//!
//! The reader at `0x2e57c` (크아비엔비2011) takes the 25 bytes, `atoi`s the ten
//! from `[15]`, reads that many more, refuses a body whose last byte is not
//! `0x17`, and dispatches on `[12]` and `[13]`: `IC` answers an open, `DP`
//! carries data, `CC`, `SC` and `QC` are the gateway's own notices.
//!
//! The open names the server the title wants, as a length-prefixed dotted
//! quad and five ASCII digits of port - `0e "211.115.203.30" "10013"`.
//!
//! The answer to an open, `IC`, is read by `0x2e4a8`: a result byte, two
//! length-prefixed strings, seven ASCII digits, two bytes and a hundred more.
//! Once it is in, the title is told it is connected.

use alloc::{boxed::Box, format, string::String, sync::Arc, vec, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead, candy_shop::CandyShop};

/// The names the gateway went by. Both resolve to [`ADDRESS`].
const HOSTS: [&str; 2] = ["ngw.gxg.com", "ngph.gxg.com"];

/// Where those names point now: an address no title dials of its own accord,
/// so a connection to it is one this endpoint was asked to take.
const ADDRESS: [u8; 4] = [10, 71, 88, 71];

const PORT: u16 = 7300;

/// What a frame opens with, less the version behind it - which the answer
/// carries back as the title wrote it.
const MAGIC: &[u8; 4] = b"GXG.";

const HEADER_LEN: usize = 25;

const END: u8 = 0x17;

#[derive(Default)]
pub struct GxgEndpoint {
    /// Kept across connections: the shop reports and buys on whichever one it
    /// has open at the time.
    stars: Arc<spin::Mutex<Stars>>,
    /// The 캔디샵 of the 메이플스토리 titles, which learns which of them it
    /// serves from one frame and sells to it on others.
    shop: Arc<spin::Mutex<CandyShop>>,
}

impl GxgEndpoint {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LocalEndpoint for GxgEndpoint {
    fn name(&self) -> &str {
        "gxg(ngw.gxg.com:7300)"
    }

    fn resolve(&self, host: &str) -> Option<[u8; 4]> {
        HOSTS.iter().any(|name| host.eq_ignore_ascii_case(name)).then_some(ADDRESS)
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        let address = format!("{}.{}.{}.{}", ADDRESS[0], ADDRESS[1], ADDRESS[2], ADDRESS[3]);

        scheme == "socket" && port == PORT && (host == address || HOSTS.iter().any(|name| host.eq_ignore_ascii_case(name)))
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(GxgConnection {
            stars: self.stars.clone(),
            shop: self.shop.clone(),
            ..Default::default()
        })
    }
}

#[derive(Default)]
struct GxgConnection {
    pending: Vec<u8>,
    outgoing: Vec<u8>,
    /// The server the title asked to be carried to, once it has.
    target: Option<(String, u16)>,
    /// An answer is in that the title has not yet been told is coming.
    ///
    /// A real gateway's answer arrives a round trip after the frame that asked
    /// for it, and the title is written for that: having sent data, `0x2e8a0`
    /// tells the title it went (event 9) before it zeroes its count of what it
    /// sent, and the title reads straight away from inside that. An answer
    /// already waiting is read, and the next request sent, while the count
    /// still holds the last one - it goes out with a length of `-28` and the
    /// connection is dropped. So the first read after an answer finds nothing,
    /// as it would have on the air, and the answer goes to the read callback
    /// the title then sets.
    held: bool,
    stars: Arc<spin::Mutex<Stars>>,
    shop: Arc<spin::Mutex<CandyShop>>,
}

/// The servers behind the gateway, as the KTF builds reach them: straight,
/// through the `FastRelay` carrier library, with no gateway frame around
/// Nexon's own. 메이플스토리 Live and 크아비엔비2011 dial
/// `211.115.203.30:10018` for their shops, and the relay found nothing
/// answering it. The frames are the ones the gateway carries as data, so they
/// are answered the same way.
#[derive(Default)]
pub struct NexonRelayEndpoint {
    stars: Arc<spin::Mutex<Stars>>,
    shop: Arc<spin::Mutex<CandyShop>>,
}

impl NexonRelayEndpoint {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The address the KTF builds dial, and the ports of it they reach.
const RELAY_HOST: &str = "211.115.203.30";
const RELAY_PORTS: [u16; 2] = [10013, 10018];

impl LocalEndpoint for NexonRelayEndpoint {
    fn name(&self) -> &str {
        "nexon(211.115.203.30)"
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == RELAY_HOST && RELAY_PORTS.contains(&port)
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(NexonRelayConnection {
            stars: self.stars.clone(),
            shop: self.shop.clone(),
            ..Default::default()
        })
    }
}

#[derive(Default)]
struct NexonRelayConnection {
    pending: Vec<u8>,
    outgoing: Vec<u8>,
    stars: Arc<spin::Mutex<Stars>>,
    shop: Arc<spin::Mutex<CandyShop>>,
}

impl LocalConnection for NexonRelayConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // Nexon's frames lead with a `u32le` length counting the whole frame.
        while self.pending.len() >= MARKED_HEADER {
            let length = u32::from_le_bytes(self.pending[..4].try_into().unwrap()) as usize;
            if length < MARKED_HEADER || length > 0x10000 {
                tracing::warn!("nexon relay: dropping {} bytes that are not a frame", self.pending.len());
                self.pending.clear();
                break;
            }
            if self.pending.len() < length {
                break;
            }

            let data: Vec<u8> = self.pending.drain(..length).collect();
            tracing::info!("nexon relay <- {}", data.iter().map(|x| format!("{x:02x}")).collect::<String>());
            match self.shop.lock().answer(&data).or_else(|| answer_data(&data)) {
                Some(mut answer) => {
                    let mut stars = self.stars.lock();
                    stars.observe(&data);
                    stars.settle(&mut answer);
                    drop(stars);

                    tracing::info!("nexon relay -> {}", answer.iter().map(|x| format!("{x:02x}")).collect::<String>());
                    self.outgoing.extend_from_slice(&answer);
                }
                None => tracing::warn!("nexon relay: no answer"),
            }
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

/// One frame of the gateway's, as the title reads them.
pub fn frame(version: &[u8; 12], kind: &[u8; 3], body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER_LEN + body.len() + 1);
    frame.extend_from_slice(version);
    frame.extend_from_slice(kind);

    let mut length = [0u8; 10];
    let digits = format!("{}", body.len() + 1);
    length[..digits.len()].copy_from_slice(digits.as_bytes());
    frame.extend_from_slice(&length);

    frame.extend_from_slice(body);
    frame.push(END);
    frame
}

/// Splits one whole frame off the front of `pending`: its type and its body,
/// less the closing `0x17`.
fn take_frame(pending: &mut Vec<u8>) -> Option<([u8; 12], [u8; 3], Vec<u8>)> {
    if pending.len() < HEADER_LEN || !pending.starts_with(MAGIC) {
        return None;
    }

    let digits: Vec<u8> = pending[15..25].iter().copied().take_while(u8::is_ascii_digit).collect();
    let length: usize = core::str::from_utf8(&digits).ok()?.parse().ok()?;
    if pending.len() < HEADER_LEN + length {
        return None;
    }

    let version: [u8; 12] = pending[..12].try_into().ok()?;
    let kind = [pending[12], pending[13], pending[14]];
    let mut body: Vec<u8> = pending.drain(..HEADER_LEN + length).skip(HEADER_LEN).collect();
    if body.last() == Some(&END) {
        body.pop();
    }

    Some((version, kind, body))
}

/// The server an open names: a length-prefixed dotted quad and five digits of
/// port, after the fixed fields ahead of them.
fn open_target(body: &[u8]) -> Option<(String, u16)> {
    // The address is the one length-prefixed run of digits and dots in the
    // body; looking for it keeps this from depending on the fields before it.
    (0..body.len()).find_map(|at| {
        let length = *body.get(at)? as usize;
        let address = body.get(at + 1..at + 1 + length)?;
        if length < 7 || !address.iter().all(|x| x.is_ascii_digit() || *x == b'.') || address.iter().filter(|x| **x == b'.').count() != 3 {
            return None;
        }

        let port = body.get(at + 1 + length..at + 6 + length)?;
        let port = core::str::from_utf8(port).ok()?.parse().ok()?;
        Some((String::from_utf8_lossy(address).into_owned(), port))
    })
}

/// What the server behind the gateway says to `data`: whichever of the
/// protocols its titles speak the data is in. Nexon's own `[u32 length][0xffff]
/// [command]` frames, which 크아비엔비2011 and the 메이플/던파 builds send,
/// 컴투스's five-byte-header messages, which 슈퍼액션히어로4 does, and EA's
/// `[u32be length][command]` ones: EA프로야구2010's SKT builds carry the SMS
/// question its LGT build sends `210.222.18.28:20102` straight to `:20100`
/// through here, and left unanswered wait a long while on `네트워크 연결중`.
fn answer_data(data: &[u8]) -> Option<Vec<u8>> {
    crate::billing::lgt_local_marked_command_response(data)
        .or_else(|| crate::billing::lgt_local_opcode_header_response(data))
        .or_else(|| crate::billing::lgt_local_scrambled_opcode_header_response(data))
        .or_else(|| crate::billing::lgt_local_ea_baseball_response(data))
        .or_else(|| marked_granted(data))
}

/// The marker Nexon's `[u32 length][0xffff][command]` frames carry.
const MARKED_HEADER: usize = 8;
const MARKER: [u8; 2] = [0xff, 0xff];

/// Any other of Nexon's frames, granted under the command one past it: a zero
/// result and an empty message.
///
/// 메이플스토리 Live's 캔디샵 uploads the character before it opens - `0x0004`,
/// up to four sections under `0x0006`, two under `0x0008`, then `0x000a` and
/// on - and the readers behind its dispatcher (`0x36cfe`: `0x36b24` for the
/// commands up to 9, `0x36aec` for 10 to 19) each take the result byte and go
/// on from zero, where anything else is an error screen. Nothing past the
/// result is read on that path, so this answers every step of it the same
/// way, and whatever the titles behind this gateway send that the frames above
/// do not know.
fn marked_granted(data: &[u8]) -> Option<Vec<u8>> {
    let length = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
    if data.len() < MARKED_HEADER || length != data.len() || data[4..6] != MARKER {
        return None;
    }

    let command = u16::from_le_bytes([data[6], data[7]]).wrapping_add(1);
    let mut answer = Vec::with_capacity(MARKED_HEADER + 3);
    answer.extend_from_slice(&((MARKED_HEADER + 3) as u32).to_le_bytes());
    answer.extend_from_slice(&MARKER);
    answer.extend_from_slice(&command.to_le_bytes());
    answer.extend_from_slice(&[0, 0, 0]);

    tracing::info!("gxg: granting Nexon command {:#06x} by default", command.wrapping_sub(1));
    Some(answer)
}

/// 슈퍼액션히어로4's stars, as far as its own frames have said.
///
/// The confirmation of a purchase (`0xcc`) carries a `u32` that `0x687d0`
/// hands to `0x675fc`, which stores it as the stars the subscriber now has -
/// the halfword at `+0x14a4` of the title's state at `0x108e4ec` - rather than
/// adding it to them. A real server knew the balance; this one learns it from
/// the title's own report, `0x5a`, which carries the stars at `[0..4]` of its
/// body and which the shop sends on the way in, and the amount from the
/// approval, `0xc9`, which carries it at `[2..6]` (`0014 000003e8 0000` for
/// 1000). The confirmation then hands back the one plus the other.
#[derive(Default)]
struct Stars {
    balance: Option<u32>,
    bought: Option<u32>,
}

/// The opcode-header frame's header length, and the opcodes [`Stars`] keeps
/// track through.
const OPCODE_HEADER: usize = 5;
const REPORT_OPCODE: u8 = 0x5a;
const REPORT_BODY_MIN: usize = 32;
const APPROVAL_OPCODE: u8 = 0xc9;
const APPROVAL_BODY: usize = 8;
const APPROVAL_KIND: u16 = 0x14;
const CONFIRM_ANSWER_OPCODE: u8 = 0xcc;
const CONFIRM_ANSWER_BODY: usize = 4;

/// The opcode and body of an opcode-header frame, `[u16 length][0][opcode][0]`.
fn opcode_frame(data: &[u8]) -> Option<(u8, &[u8])> {
    let whole = data.len() >= OPCODE_HEADER && u16::from_be_bytes([data[0], data[1]]) as usize == data.len() && data[2] == 0 && data[4] == 0;
    whole.then(|| (data[3], &data[OPCODE_HEADER..]))
}

fn be_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}

impl Stars {
    /// Takes in whatever `request` says of the stars.
    fn observe(&mut self, request: &[u8]) {
        match opcode_frame(request) {
            Some((REPORT_OPCODE, body)) if body.len() >= REPORT_BODY_MIN => self.balance = be_u32(body),
            Some((APPROVAL_OPCODE, body)) if body.len() == APPROVAL_BODY && u16::from_be_bytes([body[0], body[1]]) == APPROVAL_KIND => {
                self.bought = be_u32(&body[2..])
            }
            _ => {}
        }
    }

    /// Puts the stars the subscriber now has into a confirmation's answer.
    fn settle(&mut self, answer: &mut [u8]) {
        if !matches!(opcode_frame(answer), Some((CONFIRM_ANSWER_OPCODE, body)) if body.len() == CONFIRM_ANSWER_BODY) {
            return;
        }
        let Some(bought) = self.bought.take() else {
            return;
        };

        // A shop entered without a report first is one whose balance nothing
        // has said; the amount alone at least gives the subscriber what they
        // paid for.
        let balance = self.balance.unwrap_or(0).saturating_add(bought);
        self.balance = Some(balance);
        answer[OPCODE_HEADER..].copy_from_slice(&balance.to_be_bytes());
    }
}

/// The gateway's answer to an open, as `0x2e4a8` reads it.
fn open_answer(version: &[u8; 12]) -> Vec<u8> {
    let mut body = vec![0u8]; // result
    body.push(0); // a string, empty
    body.push(0); // and another
    body.extend_from_slice(b"0000000"); // seven digits
    body.extend_from_slice(&[0, 0]);
    body.extend_from_slice(&[0u8; 100]);
    frame(version, b"ICS", &body)
}

impl LocalConnection for GxgConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let answered = self.outgoing.len();

        while let Some((version, kind, body)) = take_frame(&mut self.pending) {
            tracing::info!(
                "gxg <- {} {} bytes: {}",
                String::from_utf8_lossy(&kind),
                body.len(),
                body.iter().map(|x| format!("{x:02x}")).collect::<String>()
            );

            match &kind[..2] {
                b"IP" => {
                    self.target = open_target(&body);
                    tracing::info!("gxg open to {:?}", self.target);
                    self.outgoing.extend_from_slice(&open_answer(&version));
                }
                // Data for the server the open named, in Nexon's own
                // `[u32 length][0xffff][command]` frames - the ones the LGT
                // builds send that server straight - and answered the same way,
                // carried back as data.
                b"DP" => match self.shop.lock().answer(&body).or_else(|| answer_data(&body)) {
                    Some(mut answer) => {
                        let mut stars = self.stars.lock();
                        stars.observe(&body);
                        stars.settle(&mut answer);
                        drop(stars);

                        tracing::info!("gxg -> DPS {}", answer.iter().map(|x| format!("{x:02x}")).collect::<String>());
                        self.outgoing.extend_from_slice(&frame(&version, b"DPS", &answer));
                    }
                    None => tracing::warn!("gxg: no answer for data to {:?}", self.target),
                },
                _ => tracing::warn!("gxg: no answer for {}", String::from_utf8_lossy(&kind)),
            }
        }

        if self.outgoing.len() > answered {
            self.held = true;
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() || core::mem::take(&mut self.held) {
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
    /// The KTF builds' frames come straight, with no gateway frame around
    /// them, and are answered the same way the gateway's data is: here a
    /// 캔디샵 session, which is answered with its command plus one.
    #[test]
    fn a_relayed_frame_is_answered_without_the_gateway() {
        use super::{LocalConnection, LocalEndpoint, LocalRead, NexonRelayEndpoint};

        let endpoint = NexonRelayEndpoint::new();
        assert!(endpoint.accepts("socket", "211.115.203.30", 10018));
        assert!(!endpoint.accepts("socket", "211.115.203.31", 10018));

        let mut session = alloc::vec::Vec::new();
        session.extend_from_slice(&(8u32 + 25).to_le_bytes());
        session.extend_from_slice(&[0xff, 0xff, 0x00, 0x00]);
        session.extend_from_slice(b"01068488891");
        session.extend_from_slice(&[0u8; 14]);

        let mut connection = endpoint.open("socket", "211.115.203.30", 10018);
        connection.write(&session[..10]);
        assert!(!connection.readable());
        connection.write(&session[10..]);
        assert!(connection.readable());

        let mut out = [0u8; 64];
        let LocalRead::Data(read) = connection.read(&mut out) else {
            panic!("the session is answered");
        };
        assert_eq!(&out[4..8], &[0xff, 0xff, 0x01, 0x00]);
        assert_eq!(u32::from_le_bytes(out[..4].try_into().unwrap()) as usize, read);
    }

    use super::*;

    const VERSION: &[u8; 12] = b"GXG.01.00.07";

    fn connection(endpoint: &GxgEndpoint) -> Box<dyn LocalConnection> {
        endpoint.open("socket", "10.71.88.71", PORT)
    }

    /// Reads one whole answer, past the empty read that follows every write.
    fn read_answer(connection: &mut dyn LocalConnection) -> ([u8; 3], Vec<u8>) {
        assert!(matches!(connection.read(&mut [0; 16]), LocalRead::Pending));

        let mut buffer = [0u8; 1024];
        let LocalRead::Data(length) = connection.read(&mut buffer) else {
            panic!("no answer");
        };
        let mut pending = buffer[..length].to_vec();
        let (version, kind, body) = take_frame(&mut pending).unwrap();
        assert_eq!(&version, VERSION);
        assert!(pending.is_empty());
        (kind, body)
    }

    fn opcode_request(opcode: u8, body: &[u8]) -> Vec<u8> {
        let mut request = vec![0, 0, 0, opcode, 0];
        request.extend_from_slice(body);
        let length = request.len() as u16;
        request[..2].copy_from_slice(&length.to_be_bytes());
        request
    }

    #[test]
    fn the_names_resolve_to_the_gateway() {
        let endpoint = GxgEndpoint::new();
        assert_eq!(endpoint.resolve("ngw.gxg.com"), Some(ADDRESS));
        assert_eq!(endpoint.resolve("NGPH.GXG.COM"), Some(ADDRESS));
        assert_eq!(endpoint.resolve("example.com"), None);
        assert!(endpoint.accepts("socket", "10.71.88.71", PORT));
        assert!(!endpoint.accepts("socket", "10.71.88.71", 7301));
    }

    #[test]
    fn a_frame_round_trips() {
        let mut pending = frame(VERSION, b"DPC", b"abc");
        assert_eq!(&pending[15..25], b"4\0\0\0\0\0\0\0\0\0");
        assert_eq!(pending.last(), Some(&END));

        pending.extend_from_slice(b"GXG.");
        let (version, kind, body) = take_frame(&mut pending).unwrap();
        assert_eq!((&version, &kind, body.as_slice()), (VERSION, b"DPC", &b"abc"[..]));
        assert_eq!(pending, b"GXG.");
    }

    #[test]
    fn an_open_is_answered_and_names_its_target() {
        let mut body = vec![1, 2, 3, 14];
        body.extend_from_slice(b"211.115.203.30");
        body.extend_from_slice(b"10013");
        assert_eq!(open_target(&body), Some((String::from("211.115.203.30"), 10013)));

        let endpoint = GxgEndpoint::new();
        let mut connection = connection(&endpoint);
        connection.write(&frame(VERSION, b"IPC", &body));
        let (kind, answer) = read_answer(connection.as_mut());
        assert_eq!(&kind, b"ICS");
        assert_eq!(answer.len(), 1 + 1 + 1 + 7 + 2 + 100);
        assert_eq!(answer[0], 0);
    }

    #[test]
    fn the_logged_requests_are_answered() {
        // 메이플스토리 Live's 캔디샵 upload.
        let live =
            "42000000ffff0400007a00020648422800711600000000d1c51f1700000020000000000003050a0a030a0005011414140008140a1412040003000e01020204000003";
        // 던파 귀검사편's purchase.
        let dnf = "0d000000ffff2000000000000b";
        // 슈퍼액션히어로4's 가면상점 page.
        let hero = "002400470000000000050000000000000000000000000000000000000000000000000000";

        let decode = |hex: &str| {
            (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect::<Vec<u8>>()
        };

        assert_eq!(answer_data(&decode(live)).unwrap(), [11, 0, 0, 0, 0xff, 0xff, 5, 0, 0, 0, 0]);
        assert_eq!(answer_data(&decode(dnf)).unwrap(), [11, 0, 0, 0, 0xff, 0xff, 0x21, 0, 0, 0, 0]);
        assert_eq!(answer_data(&decode(hero)).unwrap(), [0, 14, 0, 0x47, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_purchase_confirms_the_new_balance() {
        let endpoint = GxgEndpoint::new();

        let mut report = vec![0u8; 44];
        report[..4].copy_from_slice(&0x1d0u32.to_be_bytes());
        let mut first = connection(&endpoint);
        first.write(&frame(VERSION, b"DPC", &opcode_request(0x5a, &report)));
        let (kind, answer) = read_answer(first.as_mut());
        assert_eq!(&kind, b"DPS");
        assert_eq!(answer, [0, 5, 0, 0x5a, 0]);

        let mut second = connection(&endpoint);
        second.write(&frame(VERSION, b"DPC", &opcode_request(0xc9, &[0, 0x14, 0, 0, 0x03, 0xe8, 0, 0])));
        let (_, approval) = read_answer(second.as_mut());
        assert_eq!(approval[3], 0xca);

        second.write(&frame(VERSION, b"DPC", &opcode_request(0xcb, &approval[5..])));
        let (_, confirmation) = read_answer(second.as_mut());
        assert_eq!(confirmation, [0, 9, 0, 0xcc, 0, 0, 0, 0x05, 0xb8]);
    }
}
