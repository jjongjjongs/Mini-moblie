//! The services these titles reached over a billing socket, answered in process.
//!
//! A handset opened a socket to the carrier or the publisher to authenticate a
//! copy, to sell an item, or - for a title whose online menu went the same way -
//! to log in, and every one of those services has been switched off for years. A
//! title that reaches one and is told nothing usually stops on a screen it never
//! leaves.
//!
//! Five protocols turn up across the titles here, and a request is recognised by
//! its own shape rather than by which title sent it. Anything that is not one of
//! them is left unanswered rather than guessed at.
//!
//! Both API surfaces reach these: a WIPI-C title through `MC_netBillSocket`, a
//! WIPI-Java one through `org.kwis.msf.io.URL`'s `BillSocket://`. They answer
//! the same protocols, so the answers live here rather than in either.

use alloc::{format, string::String, vec, vec::Vec};

/// The seven-byte granted frame this protocol's answers are: the `0xffff`
/// marker, the length, the message type, and a zero status.
const GRANTED_FRAME_SIZE: usize = 7;

/// Which end of a billing frame's `u16` header fields comes first.
///
/// The frames carry a `0xffff` marker, a length and a message type, and titles
/// do not agree on how the two `u16`s are laid out: 붉은보석 writes a 19-byte
/// purchase request as `ff ff 13 00 68 00 ...`, little end first, where the
/// same frame reconstructed big-endian would be `ff ff 00 13 00 68 ...`. The
/// marker is a palindrome and says nothing, so the length is what tells them
/// apart - only one reading of it can describe the frame in hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BillFrameOrder {
    Big,
    Little,
}

impl BillFrameOrder {
    pub fn read(self, bytes: [u8; 2]) -> u16 {
        match self {
            Self::Big => u16::from_be_bytes(bytes),
            Self::Little => u16::from_le_bytes(bytes),
        }
    }

    pub fn write(self, value: u16) -> [u8; 2] {
        match self {
            Self::Big => value.to_be_bytes(),
            Self::Little => value.to_le_bytes(),
        }
    }
}

/// A billing frame's header, read in whichever order its own length makes sense
/// in.
pub struct BillFrame {
    pub order: BillFrameOrder,
    pub message_type: u16,
}

impl BillFrame {
    /// `None` for anything that is not one of these frames: too short to carry
    /// a header, no `0xffff` marker, or a length that describes no frame this
    /// could be under either reading.
    ///
    /// A native client may hand `MC_netSocketWrite` only part of the frame it
    /// built - 붉은보석 declares nineteen bytes and has been seen writing ten -
    /// so a length longer than the slice is accepted. It has to be at least the
    /// six a header takes, and where neither reading is exact the shorter one
    /// wins, being the one that could still be this frame.
    pub fn parse(request: &[u8]) -> Option<Self> {
        if request.len() < 6 || request[0] != 0xff || request[1] != 0xff {
            return None;
        }

        let length = [request[2], request[3]];
        let mut best: Option<(BillFrameOrder, usize)> = None;

        for order in [BillFrameOrder::Big, BillFrameOrder::Little] {
            let declared = order.read(length) as usize;

            if declared < 6 || declared < request.len() {
                continue;
            }

            if best.is_none_or(|(_, shortest)| declared < shortest) {
                best = Some((order, declared));
            }
        }

        let (order, _) = best?;

        Some(Self {
            order,
            message_type: order.read([request[4], request[5]]),
        })
    }
}

/// A billing frame as a trace line: its header fields read out, then the bytes.
///
/// These frames all start `ffff`, a `u16` length and a `u16` type, so naming
/// those three is what makes a capture readable without counting nibbles. Read
/// the way [`BillFrame`] reads them, and named with the order it settled on, so
/// a trace shows what the code acted on rather than one guess at it. Anything
/// this cannot read as a frame is shown as bytes alone. Capped, because a trace
/// is for reading.
pub fn bill_frame_trace(frame: &[u8]) -> String {
    // Enough for a whole request. 제노니아1's is 72 bytes and a 64-byte cap cut
    // off the end of it, which is the half that says what the title asked for.
    const SHOWN: usize = 256;

    let bytes: Vec<String> = frame.iter().take(SHOWN).map(|byte| format!("{byte:02x}")).collect();
    let bytes = format!("{}{}", bytes.join(" "), if frame.len() > SHOWN { " ..." } else { "" });

    let Some(parsed) = BillFrame::parse(frame) else {
        return format!("{} bytes [{bytes}]", frame.len());
    };

    format!(
        "{:?}-endian type {:#06x} len {} of {} bytes [{bytes}]",
        parsed.order,
        parsed.message_type,
        parsed.order.read([frame[2], frame[3]]),
        frame.len(),
    )
}
/// The answer to the pipe-delimited cash request NHN's titles send.
///
/// 데몬헌터 (`0002B5EB`) opens a billing socket to `222.237.78.175` and writes an
/// ASCII record rather than a framed message:
///
/// ```text
/// CASH|0|demon|05590091|00029B60004|500|2034517541
/// ```
///
/// - the transaction, the game's own code and account, the item code, its price
///   in won, and a token. The item codes and prices are a table in the title's
///   own `binary.mod`, `00029B60001|100|` through `0002B640007|2900|`.
///
/// What it does with the answer is a chain of string compares at `0x21004`:
/// equal to `SASH` takes the branch that shows 결제가 완료되었습니다, and the
/// two failures it knows by name are `SFL|MOVER` (monthly purchase limit) and
/// `SFL|PNUM` (staff accounts). Anything else - including the nothing a
/// switched-off gateway returns - falls through to 네트워크 장애가
/// 발생했습니다, which is the notice the title cannot get past.
///
/// The answer carries its own length ahead of it. The title's receive is a
/// two-step state machine at `0x20f76`: state 6 recvs exactly two bytes, reads
/// them as a `u16` and passes that through `MC_utilHtons` - so the field is
/// big-endian on the wire - and state 7 recvs exactly that many bytes and
/// compares them. The length counts the body alone; the two it was read from
/// are already consumed.
///
/// Answered `SASH` bare, the title read `SA` as its length, made 0x5341 of it
/// and waited for 21313 bytes that were never coming - which the trace caught
/// as `MC_utilHtons(0x4153)` on the very next line.
///
/// So the answer is `00 04` then `SASH`, and nothing after it: the compare is
/// an equality against a string built to the length just read.
///
/// `None` for anything that is not one of these records, which is not something
/// to answer with a guess.
pub fn lgt_local_cash_response(request: &[u8]) -> Option<Vec<u8>> {
    const REQUEST: &[u8] = b"CASH|";
    const GRANTED: &[u8] = b"SASH";

    if !request.starts_with(REQUEST) {
        return None;
    }

    let mut response = Vec::with_capacity(2 + GRANTED.len());
    response.extend_from_slice(&(GRANTED.len() as u16).to_be_bytes());
    response.extend_from_slice(GRANTED);

    Some(response)
}
/// What GAMEVIL's server answers one of its titles' purchases with.
///
/// 제노니아1 (`00027BAA`) opens a billing socket to `218.145.70.36:31206` and
/// writes a 72-byte record; 제노니아2 (`0002C004`) and 3 (`0002FE78`) write a
/// 93-byte one to the same place. They are the same record with a tail added:
///
/// ```text
/// [0..2]   u16 LE - the record's own length
/// [2..4]   u16 LE - the command
/// [4..16]  the subscriber number
/// [16..56] the item, EUC-KR
/// [56..60] u32 LE - the price in won
/// [60..72] the item code, the title's own aid and an index
/// [72..]   2 and 3 add a flag and the handset model
/// ```
///
/// The reply is settled by GAMEVIL's own later Android port of 제노니아1, which
/// carries C++ symbols for the protocol these titles speak - the same server
/// address and the same `00027BAA00n` item codes are strings inside it.
///
/// `tagNetHeader` is four bytes: `GetLength` reads a `u16` at `[0]`, `GetCMD` a
/// `u16` at `[2]`, and `CGsNetCore::GetRecvPacketHeaderSize` returns 4.
/// `CMvNet::OnRecvDone` then skips the header, reads one **signed byte** as the
/// status and calls `OnError(cmd, status)` when it is below `-1`, and otherwise
/// switches on the command over a fixed list - `0x101`, `0x103`, ... `0x701`,
/// `0x805` - dropping anything not on it without a word.
///
/// Every command a title sends is even and the answer to it is that command plus
/// one: `0x0700` is `CS_BUY_ITEM` and `0x0701` reaches `API_ZN_SC_BUY_ITEM`,
/// which reads nothing out of the body and calls a single callback. 제노니아1
/// buys with `0x0700`, 2 and 3 with `0x0400`, so each is answered with its own
/// command plus one.
///
/// Which accounts for every sweep run at 제노니아1 before the port settled it.
/// `0x107`, `0x103` and `0x101` are real commands, so they were dispatched - to
/// handlers with nothing to say. `0x0700` and `0x0007`, answered as though the
/// command were the request's own, are not commands at all and were dropped. And
/// every reply whose status came out negative drew a red message, because the
/// status is read before the command is looked at.
///
/// `None` for anything that is not one of these records: the declared length has
/// to be the record in hand, it has to be long enough to carry a purchase, and
/// the command has to be one a title sends rather than one it is sent.
pub fn lgt_local_gamevil_packet_response(request: &[u8]) -> Option<Vec<u8>> {
    /// Through the item code, which is the shortest of these records seen.
    const SHORTEST_PURCHASE: usize = 72;
    /// Not negative, so `OnRecvDone` reaches the command instead of `OnError`.
    const GRANTED: u8 = 0;
    /// Header, status, and room behind it. The buy handler reads nothing there,
    /// but a reply that carries a little cannot come up short.
    const LENGTH: usize = 0x20;

    if request.len() < SHORTEST_PURCHASE || u16::from_le_bytes([request[0], request[1]]) as usize != request.len() {
        return None;
    }

    // A title's own commands are the even ones; the odd are what it is answered
    // with. Answering an odd command would be answering an answer.
    let command = u16::from_le_bytes([request[2], request[3]]);
    if command == 0 || command % 2 != 0 {
        return None;
    }

    let mut response = vec![0u8; LENGTH];
    response[0..2].copy_from_slice(&(LENGTH as u16).to_le_bytes());
    response[2..4].copy_from_slice(&(command + 1).to_le_bytes());
    response[4] = GRANTED;

    Some(response)
}
/// What answers the big-endian record 레전드오브마스터 sends its purchases in.
///
/// 레전드오브마스터 (`0002A4B1`) opens `BillSocket://211.189.18.116:9407` and
/// writes a 55-byte record. Buying a 최상급강화석 for 500원 writes:
///
/// ```text
/// 00 37  08 36  00 ... 00  64  00 ... 00  12  01 f4  <item, EUC-KR>  00 ... 00  c8 d1
/// ```
///
/// ```text
/// [0..2]   u16 BE - the record's own length
/// [2..4]   u16 BE - the command
/// [4..53]  the body: the item, its price in won, and the counters around them
/// [53..55] a checksum
/// ```
///
/// The title is compiled ahead of time, so what it does with the answer is ARM
/// rather than bytecode. Its network thread's `run` is a state machine over one
/// field, and the read state at `0xf3e68` is the whole of the reply's shape:
///
/// - `read(header, 0, 4)`, then `getShort(header, 0)` as the length - which has
///   to be above zero - and `getShort(header, 2)` as the command, which has to
///   be **above 1000** or the thread drops the connection. `getShort` at
///   `0xe26a8` is `(buf[off] << 8) | buf[off + 1]`, so both are big-endian.
/// - `read(body, 0, length - 6)` when that is positive, kept as the reply body.
/// - `read(header, 0, 4)` once more - a four-byte tail it reads past and never
///   looks at.
///
/// Which is not the shape of its own requests: the 55 it writes are four of
/// header, 49 of body and two of checksum, so the length counts six of overhead
/// either way but the tail it reads is twice the tail it writes. The answer is
/// built for the reader rather than mirrored off the writer.
///
/// The dispatcher at `0x5b79c` then zeroes the body's read cursor and switches
/// on the command. `0x0837` - the request's own command plus one - reaches
/// `0x63d04`, which takes **one signed byte** off the body and treats `0` and
/// `6` as granted; anything else raises the flag the 통신장애 notice is drawn
/// from. Nothing else in that handler reads the body.
///
/// So the answer is the command plus one and a zero status byte. The body is
/// padded past the one byte this command reads because the cursor is shared
/// with every other command's handler, and a body only as long as its shortest
/// reader would put a longer one out of bounds.
///
/// `None` for anything that is not one of these records: the declared length has
/// to be the record in hand, and the command has to be one a title sends - the
/// even ones - rather than one it is sent.
pub fn lgt_local_big_endian_record_response(request: &[u8]) -> Option<Vec<u8>> {
    /// A length and a command, which is what the title reads before anything
    /// else.
    const HEADER: usize = 4;
    /// Read past and discarded, but it has to be there to be read past.
    const TAIL: usize = 4;
    /// The length field counts the header and the tail as six between them.
    const LENGTH_OVERHEAD: usize = 6;
    /// Room behind the status byte, for the handlers that read further.
    const BODY: usize = 0x20;
    /// Which the purchase handler spells "granted".
    const GRANTED: u8 = 0;
    /// Below this the title drops the connection rather than dispatching.
    const LEAST_COMMAND: u16 = 1000;

    if request.len() < HEADER + 2 || u16::from_be_bytes([request[0], request[1]]) as usize != request.len() {
        return None;
    }

    // A title's own commands are the even ones; the odd are what it is answered
    // with. Answering an odd command would be answering an answer.
    let command = u16::from_be_bytes([request[2], request[3]]);
    if command <= LEAST_COMMAND || command % 2 != 0 {
        return None;
    }

    let mut response = vec![0u8; HEADER + BODY + TAIL];
    response[0..2].copy_from_slice(&((BODY + LENGTH_OVERHEAD) as u16).to_be_bytes());
    response[2..4].copy_from_slice(&(command + 1).to_be_bytes());
    response[HEADER] = GRANTED;

    Some(response)
}
/// What answers the length-prefixed command 영웅서기4 opens its online menu with.
///
/// 영웅서기4 (`0002D74B`) reaches `210.222.18.31:8894` through
/// `MC_netBillSocket` - the carrier's socket carries a game service here rather
/// than a purchase - and speaks a frame of its own:
///
/// ```text
/// [0..4]  u32 LE - the frame's own length, this field included
/// [4]     u8     - the major command
/// [5]     u8     - the minor command
/// [6..]   the body
/// ```
///
/// Opening 상점 writes `07 00 00 00 01 01 04`, and five seconds later
/// `06 00 00 00 00 0a` - which the title names itself, through
/// `MC_knlPrintk`: `[SEND PROTOCL] MAJOR_SYSTEM_MESSAGE / MINOR_KEEP_ALIVE_MSG`.
///
/// The title is native, so its receive path is ARM. The callback at `0x572c8`
/// queues whatever arrives, and the dispatcher at `0x61518` drops anything under
/// six bytes, reads the major at `[4]` and the minor at `[5]`, and switches on
/// the major - `1`, `5`, `0x14` and `0x64` are handled and everything else is
/// dropped without a word, the title's own major `0` keep-alive included.
///
/// Which is what the online menu's login is, read out of those handlers:
///
/// | the title sends | the handler | what it does next |
/// |-----------------|-------------|-------------------|
/// | `1/0x01`        | `0x612d0`   | reads nothing of the reply; answers with its `PHONENUMBER` as `1/0x3d` |
/// | `1/0x3d`        | `0x6137a`   | reads nothing of the reply; answers `1/0x3e` |
/// | `1/0x3e`        | `0x613b8`   | reads nothing of the reply while the 상점 flag is set; closes the 서버 응답을 기다리는중 notice and asks for the catalogue as `5/0x3f` |
/// | `5/0x3f`        | `0x5eb1e`   | takes a `u16 LE` count at `[8]` and that many 37-byte rows behind it, then opens the shop screen |
///
/// Buying from that screen is two more, and both read a status byte at `[6]`
/// and a NUL-terminated message at `[8]` - the pattern every `major 5` handler
/// shares, where `1` is the only status that is not an error box:
///
/// | the title sends | the handler | what it does next |
/// |-----------------|-------------|-------------------|
/// | `5/0x42`        | `0x5e9b2`   | the charge, carrying the row's handle and price. On `1` it asks for the item as `5/0x40`; on anything else it draws the message and stops |
/// | `5/0x40`        | `0x5e882`   | the delivery. On `1` it reads `[7]` as an offset and takes the byte at `[8 + offset]`: `0xff` puts the row's own item in the bag and returns to the shop, and anything else is an index into `/ITM/DAT/_ITM_CASH_RANOMBOX` |
///
/// So the first three are answered with the command alone - the title only needs
/// to see its own command come back to take the next step - the catalogue is
/// answered with the sixteen items [`hero4_catalogue`] lays out, and the two
/// halves of a purchase are granted.
///
/// The item a purchase delivers is `0xff`, the plain one: the title puts the row
/// it already has in the bag rather than rolling a random box, so what arrives
/// is the item the shop screen named and nothing this side chose. Both messages
/// are left empty, because there is no server here to have written one and the
/// granted path draws its own notice rather than the reply's.
///
/// `None` for anything else, the keep-alive included: the frame has to declare
/// its own length, and the command pair has to be one of the four whose answer
/// is known. A command answered wrongly here does not stall the title - it puts
/// it through a branch meant for a different exchange.
pub fn lgt_local_major_minor_response(request: &[u8]) -> Option<Vec<u8>> {
    /// A length, a major and a minor - and the least the dispatcher will look
    /// at, which drops anything under six bytes.
    const HEADER: usize = 6;

    if request.len() < HEADER || u32::from_le_bytes([request[0], request[1], request[2], request[3]]) as usize != request.len() {
        return None;
    }

    /// The status every `major 5` handler reads at `[6]`, and the only one that
    /// is not an error box.
    const GRANTED: u8 = 1;
    /// What the delivery reads as "the row's own item", rather than an index
    /// into the random box table.
    const PLAIN_ITEM: u8 = 0xff;

    let (major, minor) = (request[4], request[5]);
    let body: Vec<u8> = match (major, minor) {
        (1, 0x01) | (1, 0x3d) | (1, 0x3e) => Vec::new(),
        (5, 0x3f) => hero4_catalogue(),
        // Charged. The message is at `[8]`, empty, and unread on this path.
        (5, 0x42) => vec![GRANTED, 0, 0, 0],
        // Delivered. `[7]` is how far past the message the item byte sits, so
        // the empty message takes the one byte and `0xff` follows it.
        (5, 0x40) => vec![GRANTED, 1, 0, PLAIN_ITEM],
        _ => return None,
    };

    let length = HEADER + body.len();
    let mut response = Vec::with_capacity(length);
    response.extend_from_slice(&(length as u32).to_le_bytes());
    response.push(major);
    response.push(minor);
    response.extend_from_slice(&body);

    Some(response)
}

/// The body of 영웅서기4's shop catalogue: the page it is on, how many pages
/// there are, and the rows themselves.
///
/// The handler at `0x5eb1e` reads the body as
///
/// ```text
/// [0]      u8  - the page this is
/// [1]      u8  - how many pages there are
/// [2..4]   u16 LE - how many rows follow
/// [4..]    the rows, 37 bytes each
/// ```
///
/// and keeps the first three at `0x15669e8+0x132`, which is where the shop
/// screen's own left/right handler at `0x5ee10` reads the page from and wraps it
/// against the page count. One page and sixteen rows on it.
///
/// A row is read by `0x5e1d8`, which builds each item with the title's own
/// factory and then overrides four of its fields out of the row:
///
/// ```text
/// [0]      u8  - a per-row flag the shop screen keeps beside the list
/// [1..9]   the server's first handle for the row, echoed back on a purchase
/// [9..17]  its second, which is what a purchase actually sends
/// [17]     u8  - the item kind, which is what the local item table is keyed by
/// [18]     u8  - the item id in that table, where its name and icon come from
/// [19]     u8  - the grade
/// [21..25] u32 LE - the price
/// ```
///
/// **The list itself is not the original service's.** It is the one the
/// 영웅서기4_보물함 build carries, which reaches the same screen without a server
/// at all: its patch redirects the two `0x5ded8` catalogue requests to a stub
/// that returns without sending, and builds the sixteen items itself at
/// `0x7d31c` from a table of ids at `0x7d3c4` and a table of grades at `0x7d3d4`,
/// the grade in the low nibble and a multiplier in the high one, priced at fifty
/// won a step for the first twelve rows and five hundred for the last four. The
/// first row's grade is `0x14` rather than its low nibble, which is that build's
/// own exception and is kept here.
///
/// Reproducing it over the wire rather than patching the module is what lets an
/// unmodified archive reach the same screen: the title's own handler builds the
/// same items from the same ids, and everything the row does not name - the
/// item's name, icon and stats - still comes from the archive's own item table.
fn hero4_catalogue() -> Vec<u8> {
    /// Every row is this wide, whether or not it fills it.
    const ROW: usize = 37;
    /// The kind the local item table is keyed by for all sixteen, which is what
    /// the 보물함 build passes its factory.
    const KIND: u8 = 8;
    /// Rows one to twelve are priced in fifties, the last four in five hundreds.
    const CHEAP_ROWS: usize = 12;
    const CHEAP_STEP: u32 = 50;
    const COSTLY_STEP: u32 = 500;
    /// The first row's grade, which the reference build spells out rather than
    /// taking from its table.
    const FIRST_GRADE: u8 = 0x14;

    /// `(item id, packed grade)` - the grade in the low nibble and the price's
    /// multiplier in the high one, as `0x7d3c4` and `0x7d3d4` pair them.
    const ROWS: [(u8, u8); 16] = [
        (0x0f, 0x50),
        (0x05, 0x5a),
        (0x10, 0x45),
        (0x14, 0x21),
        (0x13, 0xa1),
        (0x18, 0xaa),
        (0x15, 0xa5),
        (0x16, 0xa1),
        (0x11, 0x61),
        (0x12, 0x61),
        (0x1d, 0xa1),
        (0x17, 0xa1),
        (0x19, 0x31),
        (0x1a, 0x41),
        (0x1b, 0x51),
        (0x1c, 0x61),
    ];

    let mut body = vec![0u8; 4 + ROWS.len() * ROW];
    body[0] = 0;
    body[1] = 1;
    body[2..4].copy_from_slice(&(ROWS.len() as u16).to_le_bytes());

    for (index, (item, packed)) in ROWS.into_iter().enumerate() {
        let step = if index < CHEAP_ROWS { CHEAP_STEP } else { COSTLY_STEP };
        let price = step * (packed >> 4) as u32;
        let grade = if index == 0 { FIRST_GRADE } else { packed & 0x0f };

        let row = &mut body[4 + index * ROW..4 + (index + 1) * ROW];
        row[17] = KIND;
        row[18] = item;
        row[19] = grade;
        row[21..25].copy_from_slice(&price.to_le_bytes());
    }

    body
}
/// The granted answer to an application billing request, in the frame shape
/// `lgt_local_purchase_success_response` establishes for the purchase
/// transaction: the `0xffff` marker, the frame length, the request's own type
/// plus one, and a zero status - which is what this protocol spells "granted".
///
/// `None` for anything that is not one of these frames, which is not something
/// to answer with a guess.
pub fn lgt_local_granted_response(request: &[u8]) -> Option<Vec<u8>> {
    let frame = BillFrame::parse(request)?;
    let order = frame.order;
    let message_type = frame.message_type;

    let length = order.write(GRANTED_FRAME_SIZE as u16);
    let response_type = order.write(message_type.wrapping_add(1));

    // Answered in the order it was asked in: a title that wrote its length
    // little end first reads the answer's the same way.
    Some(vec![0xff, 0xff, length[0], length[1], response_type[0], response_type[1], 0x00])
}

/// The answer to a billing request, whichever of these protocols it is in.
///
/// Tried in order of how specific each shape is: the `0xffff`-framed message,
/// then the pipe-delimited cash record, then the GAMEVIL packet, then the
/// big-endian record, then the length-prefixed command. `None` when a request is
/// none of them, which is not something to answer with a guess.
///
/// The three packet shapes cannot be mistaken for one another. Each declares its
/// own length, and no two of them read that length the same way: a length that
/// is the record in hand as a `u16` one end first is thousands the other way
/// round, and a `u32` that is the record in hand has the command in its high
/// half read as a `u16`.
pub fn response(request: &[u8]) -> Option<Vec<u8>> {
    lgt_local_granted_response(request)
        .or_else(|| lgt_local_cash_response(request))
        .or_else(|| lgt_local_gamevil_packet_response(request))
        .or_else(|| lgt_local_big_endian_record_response(request))
        .or_else(|| lgt_local_major_minor_response(request))
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;

    #[test]
    fn a_frame_s_own_length_says_which_end_of_its_fields_comes_first() {
        use super::{BillFrame, BillFrameOrder};

        // The 19-byte purchase request 붉은보석 actually writes: length and type
        // little end first, then the subscriber number as ASCII.
        let captured = [
            0xff, 0xff, 0x13, 0x00, 0x68, 0x00, b'0', b'1', b'0', b'5', b'5', b'4', b'5', b'2', b'3', b'8', b'3', 0x00, 0x01,
        ];
        let frame = BillFrame::parse(&captured).unwrap();
        assert_eq!(frame.order, BillFrameOrder::Little);
        assert_eq!(frame.message_type, 0x68);

        // The same frame written the other way round reads as itself too.
        let big_endian = [
            0xff, 0xff, 0x00, 0x13, 0x00, 0x68, b'0', b'1', b'0', b'5', b'5', b'4', b'5', b'2', b'3', b'8', b'3', 0x00, 0x01,
        ];
        let frame = BillFrame::parse(&big_endian).unwrap();
        assert_eq!(frame.order, BillFrameOrder::Big);
        assert_eq!(frame.message_type, 0x68);

        // And so does a header-complete prefix of either, where the shorter of
        // the two readings is the one that could still be this frame.
        assert_eq!(BillFrame::parse(&captured[..10]).unwrap().order, BillFrameOrder::Little);
        assert_eq!(BillFrame::parse(&big_endian[..10]).unwrap().order, BillFrameOrder::Big);
    }

    #[test]
    fn a_granted_reply_echoes_the_request_type_and_says_nothing_is_owed() {
        use super::lgt_local_granted_response;

        // The purchase transaction, which already had an answer of its own:
        // request 0x68 is answered by response 0x69 with a zero status.
        let reply = lgt_local_granted_response(&[0xff, 0xff, 0x00, 0x0a, 0x00, 0x68, 1, 2, 3, 4]).unwrap();
        assert_eq!(reply, vec![0xff, 0xff, 0x00, 0x07, 0x00, 0x69, 0x00]);

        // Every other request is answered the same way, which is what mode 1
        // had no peer to do before.
        let reply = lgt_local_granted_response(&[0xff, 0xff, 0x00, 0x06, 0x00, 0x20]).unwrap();
        assert_eq!(reply, vec![0xff, 0xff, 0x00, 0x07, 0x00, 0x21, 0x00]);
    }

    #[test]
    fn a_granted_reply_is_shaped_only_for_a_frame_this_reads() {
        use super::lgt_local_granted_response;

        // No marker, and too short to carry a header at all.
        assert!(lgt_local_granted_response(&[0x00, 0x00, 0x00, 0x06, 0x00, 0x68]).is_none());
        assert!(lgt_local_granted_response(&[0xff, 0xff, 0x00]).is_none());

        // A length that cannot hold a header whichever end is read first.
        assert!(lgt_local_granted_response(&[0xff, 0xff, 0x00, 0x00, 0x00, 0x68]).is_none());

        // A slice longer than either reading of the length it declares: 0x0304
        // one way, 0x0403 the other, both short of the bytes in hand.
        let overlong = vec![0xffu8, 0xff, 0x03, 0x04, 0x00, 0x68]
            .into_iter()
            .chain(core::iter::repeat_n(0u8, 2000))
            .collect::<Vec<u8>>();
        assert!(lgt_local_granted_response(&overlong).is_none());
    }

    /// 제노니아1's own 72-byte purchase record, captured off the title.
    fn zenonia_purchase_request() -> Vec<u8> {
        let mut request = vec![0u8; 72];
        request[0..2].copy_from_slice(&72u16.to_le_bytes());
        request[2..4].copy_from_slice(&0x0700u16.to_le_bytes());
        request[4..15].copy_from_slice(b"01055145031");
        // 생명의 근원(10개), EUC-KR, in its 40-byte field.
        request[16..33].copy_from_slice(&[
            0xbb, 0xfd, 0xb8, 0xed, 0xc0, 0xc7, 0x20, 0xb1, 0xd9, 0xbf, 0xf8, 0x28, 0x31, 0x30, 0xb0, 0xb3, 0x29,
        ]);
        request[56..60].copy_from_slice(&700u32.to_le_bytes());
        request[60..71].copy_from_slice(b"00027BAA002");

        request
    }

    /// 제노니아2's 93-byte record: the same, with a flag and the handset model
    /// behind the item code, and its own command.
    fn zenonia2_purchase_request() -> Vec<u8> {
        let mut request = vec![0u8; 93];
        request[0..2].copy_from_slice(&93u16.to_le_bytes());
        request[2..4].copy_from_slice(&0x0400u16.to_le_bytes());
        request[4..15].copy_from_slice(b"01055452383");
        request[56..60].copy_from_slice(&100u32.to_le_bytes());
        request[60..71].copy_from_slice(b"0002C004001");
        request[72] = 1;
        request[73..81].copy_from_slice(b"Emulator");

        request
    }

    #[test]
    fn a_gamevil_purchase_is_answered_the_way_onrecvdone_reads_it() {
        use super::lgt_local_gamevil_packet_response;

        // Each title is answered with its own command plus one: 제노니아1 buys
        // with 0x0700, 2 and 3 with 0x0400.
        for (request, expected) in [(zenonia_purchase_request(), 0x0701u16), (zenonia2_purchase_request(), 0x0401)] {
            let response = lgt_local_gamevil_packet_response(&request).unwrap();

            // `tagNetHeader`: a length at [0] and a command at [2], both u16
            // little end first, four bytes of it.
            assert_eq!(u16::from_le_bytes([response[0], response[1]]) as usize, response.len());
            assert_eq!(u16::from_le_bytes([response[2], response[3]]), expected);

            // The status `OnRecvDone` reads straight after the header. Below -1
            // goes to OnError instead of the command switch.
            assert!(response[4] as i8 >= 0);

            // The buy handler reads nothing behind it.
            assert!(response[5..].iter().all(|&byte| byte == 0));

            // And the same answer every time - the sweeps are over.
            assert_eq!(lgt_local_gamevil_packet_response(&request), Some(response));
        }
    }

    #[test]
    fn only_a_record_that_declares_itself_is_answered_as_a_purchase() {
        use super::lgt_local_gamevil_packet_response;

        // A length that is not the record in hand.
        let mut wrong_length = zenonia_purchase_request();
        wrong_length[0] = 0x47;
        assert_eq!(lgt_local_gamevil_packet_response(&wrong_length), None);

        // An odd command is one a title is answered with, not one it sends, so
        // answering it would be answering an answer.
        let mut answer_shaped = zenonia_purchase_request();
        answer_shaped[2..4].copy_from_slice(&0x0701u16.to_le_bytes());
        assert_eq!(lgt_local_gamevil_packet_response(&answer_shaped), None);

        // Too short to be carrying a purchase, however well it declares itself.
        let mut stub = vec![0u8; 8];
        stub[0..2].copy_from_slice(&8u16.to_le_bytes());
        stub[2..4].copy_from_slice(&0x0400u16.to_le_bytes());
        assert_eq!(lgt_local_gamevil_packet_response(&stub), None);

        // And the other protocols answered here are not mistaken for it.
        assert_eq!(
            lgt_local_gamevil_packet_response(b"CASH|0|demon|05590091|00029B60004|500|2034517541"),
            None
        );
        assert_eq!(lgt_local_gamevil_packet_response(&[0xff, 0xff, 0x00, 0x06, 0x00, 0x68]), None);
        assert_eq!(lgt_local_gamevil_packet_response(b""), None);
    }

    #[test]
    fn a_cash_request_is_answered_with_the_word_its_sender_reads_as_paid() {
        // The record 데몬헌터 actually writes, captured off the title, and the
        // length its two-step receive reads before the body it counts.
        let request = b"CASH|0|demon|05590091|00029B60004|500|2034517541";
        assert_eq!(lgt_local_cash_response(request).as_deref(), Some(b"\x00\x04SASH".as_slice()));

        // Every item in the title's own price table is the same request.
        assert_eq!(
            lgt_local_cash_response(b"CASH|0|demon|05590091|0002B640007|2900|1").as_deref(),
            Some(b"\x00\x04SASH".as_slice())
        );

        // Nothing else is one of these records.
        assert_eq!(lgt_local_cash_response(b"SASH"), None);
        assert_eq!(lgt_local_cash_response(b"CASH"), None);
        assert_eq!(lgt_local_cash_response(b""), None);
        assert_eq!(lgt_local_cash_response(&[0xff, 0xff, 0x00, 0x06, 0x00, 0x68]), None);
    }

    /// The 55-byte record 레전드오브마스터 writes to buy a 최상급강화석 for
    /// 500원, captured off the title.
    fn legend_of_master_purchase_request() -> Vec<u8> {
        let mut request = vec![0u8; 55];
        request[0..2].copy_from_slice(&55u16.to_be_bytes());
        request[2..4].copy_from_slice(&0x0836u16.to_be_bytes());
        request[18] = 0x64;
        request[29] = 0x12;
        request[30..32].copy_from_slice(&500u16.to_be_bytes());
        // 최상급강화석, EUC-KR.
        request[32..44].copy_from_slice(&[0xc3, 0xd6, 0xbb, 0xf3, 0xb1, 0xde, 0xb0, 0xad, 0xc8, 0xad, 0xbc, 0xae]);
        request[53..55].copy_from_slice(&0xc8d1u16.to_be_bytes());

        request
    }

    #[test]
    fn a_big_endian_record_is_answered_the_way_its_read_state_reads_it() {
        use super::lgt_local_big_endian_record_response;

        let request = legend_of_master_purchase_request();
        let response = lgt_local_big_endian_record_response(&request).unwrap();

        // The four byte header the read state takes first, big end first both
        // fields - and a length that counts the header and the tail as six.
        let length = i16::from_be_bytes([response[0], response[1]]);
        assert!(length > 0);
        assert_eq!(length as usize, response.len() - 2);

        // The command has to be above 1000 or the thread drops the connection,
        // and it is the request's own plus one so the dispatcher reaches the
        // purchase handler.
        let command = i16::from_be_bytes([response[2], response[3]]);
        assert!(command > 1000);
        assert_eq!(command, 0x0837);

        // Which reads one signed byte off the body: zero is granted.
        assert_eq!(response[4] as i8, 0);

        // A body long enough to read past, and a four byte tail behind it.
        assert_eq!(response.len(), 4 + (length as usize - 6) + 4);

        // And the same answer every time.
        assert_eq!(lgt_local_big_endian_record_response(&request), Some(response));
    }

    #[test]
    fn only_a_big_endian_record_that_declares_itself_is_answered_as_one() {
        use super::lgt_local_big_endian_record_response;

        // A length that is not the record in hand.
        let mut wrong_length = legend_of_master_purchase_request();
        wrong_length[1] = 0x38;
        assert_eq!(lgt_local_big_endian_record_response(&wrong_length), None);

        // A command the title is answered with rather than one it sends.
        let mut answer_shaped = legend_of_master_purchase_request();
        answer_shaped[2..4].copy_from_slice(&0x0837u16.to_be_bytes());
        assert_eq!(lgt_local_big_endian_record_response(&answer_shaped), None);

        // A command the read state would drop the connection over rather than
        // dispatch, so answering it would only cost the title its socket.
        let mut too_low = legend_of_master_purchase_request();
        too_low[2..4].copy_from_slice(&0x0064u16.to_be_bytes());
        assert_eq!(lgt_local_big_endian_record_response(&too_low), None);

        // And the other protocols answered here are not mistaken for it - each
        // declares its own length, and the GAMEVIL packet's reads as thousands
        // the other end first.
        assert_eq!(lgt_local_big_endian_record_response(&zenonia_purchase_request()), None);
        assert_eq!(
            lgt_local_big_endian_record_response(b"CASH|0|demon|05590091|00029B60004|500|2034517541"),
            None
        );
        assert_eq!(lgt_local_big_endian_record_response(&[0xff, 0xff, 0x00, 0x06, 0x00, 0x68]), None);
        assert_eq!(lgt_local_big_endian_record_response(b""), None);

        // Nor is it mistaken for one of them.
        assert_eq!(lgt_local_gamevil_packet_response(&legend_of_master_purchase_request()), None);
        assert_eq!(lgt_local_granted_response(&legend_of_master_purchase_request()), None);
    }

    /// The 7-byte frame 영웅서기4 writes to open its online 상점, captured off
    /// the title, and the 6-byte keep-alive it writes five seconds later.
    fn hero_lore_frame(major: u8, minor: u8, body: &[u8]) -> Vec<u8> {
        let length = 6 + body.len();
        let mut frame = Vec::with_capacity(length);
        frame.extend_from_slice(&(length as u32).to_le_bytes());
        frame.push(major);
        frame.push(minor);
        frame.extend_from_slice(body);

        frame
    }

    #[test]
    fn a_length_prefixed_command_is_answered_the_way_its_dispatcher_reads_it() {
        use super::lgt_local_major_minor_response;

        // The record the title actually writes on opening 상점.
        assert_eq!(hero_lore_frame(1, 1, &[4]), [0x07, 0x00, 0x00, 0x00, 0x01, 0x01, 0x04]);

        // The login the title walks: each step is answered with its own command
        // back, and each handler reads nothing else out of the reply.
        for (major, minor) in [(1u8, 0x01u8), (1, 0x3d), (1, 0x3e)] {
            let request = hero_lore_frame(major, minor, &[4]);
            let response = lgt_local_major_minor_response(&request).unwrap();

            // Six bytes: under that the dispatcher drops the frame unread.
            assert_eq!(response.len(), 6);
            assert_eq!(
                u32::from_le_bytes([response[0], response[1], response[2], response[3]]) as usize,
                response.len()
            );
            assert_eq!((response[4], response[5]), (major, minor));
        }

        // Both halves of a purchase, which carry the row's handle and its price.
        let purchase = hero_lore_frame(5, 0x42, &[0, 0, 0, 0, 0, 0, 0, 0, 0xf4, 0x01, 0x00, 0x00]);
        assert_eq!(purchase.len(), 18);
        let response = lgt_local_major_minor_response(&purchase).unwrap();
        assert_eq!((response[4], response[5]), (5, 0x42));
        // Granted, and an empty message where the error box would read one.
        assert_eq!(response[6], 1);
        assert_eq!(response[8], 0);

        let response = lgt_local_major_minor_response(&hero_lore_frame(5, 0x40, &[0; 12])).unwrap();
        assert_eq!((response[4], response[5]), (5, 0x40));
        assert_eq!(response[6], 1);
        // The item byte sits `[7]` past the message, and is the plain item.
        let offset = response[7] as usize;
        assert_eq!(response[8], 0);
        assert_eq!(response[8 + offset], 0xff);

        // And the catalogue.
        let response = lgt_local_major_minor_response(&hero_lore_frame(5, 0x3f, &[0])).unwrap();
        assert_eq!(
            u32::from_le_bytes([response[0], response[1], response[2], response[3]]) as usize,
            response.len()
        );
        assert_eq!((response[4], response[5]), (5, 0x3f));
    }

    #[test]
    fn the_catalogue_lays_out_the_sixteen_rows_its_reader_walks() {
        use super::lgt_local_major_minor_response;

        const ROW: usize = 37;

        let response = lgt_local_major_minor_response(&hero_lore_frame(5, 0x3f, &[0])).unwrap();
        let body = &response[6..];

        // One page, and the count the reader takes before the rows.
        assert_eq!((body[0], body[1]), (0, 1));
        assert_eq!(u16::from_le_bytes([body[2], body[3]]), 16);

        // Every row is walked at its full stride, so the frame has to carry all
        // sixteen of them - the reader reads to the last one's 35th byte.
        assert_eq!(body.len(), 4 + 16 * ROW);
        assert_eq!(response.len(), 6 + 4 + 16 * ROW);

        // The ids, grades and prices the 보물함 build builds its own sixteen
        // from: the grade in a packed byte's low nibble, the price its high
        // nibble by fifty for the first twelve rows and by five hundred for the
        // last four - and the first row's grade spelled 0x14 instead.
        let expected: [(u8, u8, u32); 16] = [
            (0x0f, 0x14, 250),
            (0x05, 0x0a, 250),
            (0x10, 0x05, 200),
            (0x14, 0x01, 100),
            (0x13, 0x01, 500),
            (0x18, 0x0a, 500),
            (0x15, 0x05, 500),
            (0x16, 0x01, 500),
            (0x11, 0x01, 300),
            (0x12, 0x01, 300),
            (0x1d, 0x01, 500),
            (0x17, 0x01, 500),
            (0x19, 0x01, 1500),
            (0x1a, 0x01, 2000),
            (0x1b, 0x01, 2500),
            (0x1c, 0x01, 3000),
        ];

        for (index, (item, grade, price)) in expected.into_iter().enumerate() {
            let row = &body[4 + index * ROW..4 + (index + 1) * ROW];

            // The kind the local item table is keyed by, then the id in it.
            assert_eq!(row[17], 8, "row {index} kind");
            assert_eq!(row[18], item, "row {index} item");
            assert_eq!(row[19], grade, "row {index} grade");
            assert_eq!(u32::from_le_bytes([row[21], row[22], row[23], row[24]]), price, "row {index} price");

            // Everything the row does not name is left for the title's own
            // factory to have set, and the server's two handles are nothing
            // this side has to invent.
            assert!(row[..17].iter().all(|&byte| byte == 0), "row {index} handles");
            assert_eq!(row[20], 0, "row {index}");
            assert!(row[25..].iter().all(|&byte| byte == 0), "row {index} tail");
        }
    }

    #[test]
    fn only_a_command_pair_whose_answer_is_known_is_answered() {
        use super::lgt_local_major_minor_response;

        // The keep-alive: the title's own dispatcher drops major 0, so an answer
        // to it would be an answer to nothing.
        assert_eq!(hero_lore_frame(0, 0x0a, &[]), [0x06, 0x00, 0x00, 0x00, 0x00, 0x0a]);
        assert_eq!(lgt_local_major_minor_response(&hero_lore_frame(0, 0x0a, &[])), None);

        // A command pair this cannot shape a reply to. Answering it would put
        // the title through a branch meant for a different exchange.
        assert_eq!(lgt_local_major_minor_response(&hero_lore_frame(5, 0x70, &[])), None);
        assert_eq!(lgt_local_major_minor_response(&hero_lore_frame(5, 0x41, &[])), None);
        assert_eq!(lgt_local_major_minor_response(&hero_lore_frame(0x14, 0x46, &[1])), None);

        // A length that is not the frame in hand.
        let mut wrong_length = hero_lore_frame(1, 1, &[4]);
        wrong_length[0] = 0x08;
        assert_eq!(lgt_local_major_minor_response(&wrong_length), None);

        // Too short for the dispatcher to read a command out of.
        assert_eq!(lgt_local_major_minor_response(&[0x05, 0x00, 0x00, 0x00, 0x01]), None);
        assert_eq!(lgt_local_major_minor_response(b""), None);

        // And the other protocols answered here are not mistaken for it - the
        // GAMEVIL packet's length is the record in hand as a u16, which as a u32
        // carries the command in its high half.
        assert_eq!(lgt_local_major_minor_response(&zenonia_purchase_request()), None);
        assert_eq!(lgt_local_major_minor_response(&legend_of_master_purchase_request()), None);
        assert_eq!(lgt_local_major_minor_response(b"CASH|0|demon|05590091|00029B60004|500|2034517541"), None);
        assert_eq!(lgt_local_major_minor_response(&[0xff, 0xff, 0x00, 0x06, 0x00, 0x68]), None);

        // Nor is it mistaken for one of them.
        let hello = hero_lore_frame(1, 1, &[4]);
        assert_eq!(lgt_local_granted_response(&hello), None);
        assert_eq!(lgt_local_cash_response(&hello), None);
        assert_eq!(lgt_local_gamevil_packet_response(&hello), None);
        assert_eq!(lgt_local_big_endian_record_response(&hello), None);
    }

    #[test]
    fn one_answer_covers_every_protocol_and_guesses_at_none() {
        // Each of the five, recognised by its own shape.
        assert!(response(&[0xff, 0xff, 0x06, 0x00, 0x20, 0x00]).is_some());
        assert!(response(b"CASH|0|demon|05590091|00029B60004|500|2034517541").is_some());
        assert!(response(&zenonia_purchase_request()).is_some());
        assert!(response(&legend_of_master_purchase_request()).is_some());
        assert!(response(&hero_lore_frame(1, 1, &[4])).is_some());

        // And nothing for a request that is none of them.
        assert_eq!(response(b"hello"), None);
        assert_eq!(response(&[0u8; 64]), None);
    }
}
