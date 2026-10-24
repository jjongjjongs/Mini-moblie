//! The download a KTF title asks for before it will start.
//!
//! Some titles ship their data separately: the program is small and everything
//! it draws and reads arrives over the air the first time it runs. 드래곤로드 is
//! one, and it offers the download every launch - `게임진행을 위해 데이터파일을
//! 다운받습니다. 계속할까요?` - with no way past it, because 아니오 quits.
//!
//! The server it asks is gone. What it would have fetched, though, is in the
//! archive already: a package that has been through that download once carries
//! the answer in its `P/` directory, which is the handset's own copy of what
//! arrived. So the exchange is answered here out of those files, and the title
//! writes them into its databases exactly as it would have written what came
//! off the wire.
//!
//! # The protocol
//!
//! Every frame both ways is
//!
//! ```text
//!   +0  u32 little endian  total length, counting this header
//!   +4  u16                0xffff, which the title checks
//!   +6  u16                message id
//!   +8  payload
//! ```
//!
//! A request also names the id it will accept back, and the title's receive
//! path drops the connection unless the reply carries exactly that one - so an
//! answer that echoes the request's own id, which is what a generic answer
//! does, is refused.
//!
//! | sent | awaited | what |
//! |---|---|---|
//! | 1600 | 1601 | what is there to fetch |
//! | 1602 | 1603 | this file, from this offset |
//! | 1604 | 1605 | more of the same file, from this offset |
//! | 30101 | - | a heartbeat, which expects nothing |
//!
//! `1601` answers with a status, two words the title keeps, a count, and that
//! many twenty-eight byte records:
//!
//! ```text
//!   +0   u32   a stamp, which the title compares with the one it stored
//!   +4   u8    kind: its low seven bits, 0 being "fetch this"
//!   +8   u32   size
//!   +12  char  name[16]
//! ```
//!
//! The top bit of that kind byte is what lets the title keep what it already
//! has: with it set it looks the name up locally and, if the stamp and the size
//! both match, marks the file done without asking for a byte of it. Clear - as
//! every record here leaves it - every file is fetched afresh.
//!
//! `1602` carries `[u32 offset][the record]` and `1603` answers `[u32 status]
//! [u32 length][that many bytes]`. A status of `0x83` is what the title reads
//! as the end of a file; anything else non-zero is an error it shows.
//!
//! One answer is one piece, not one file. The title writes what arrives, adds
//! it to what it has, and while that is short of the size the listing gave it
//! asks again with `1604` - the same question as `1602` carrying the offset
//! alone, because the file is the one already under way. Its answer is `1605`,
//! which the title hands to the very same handler as `1603` and so has the very
//! same body.
//!
//! See `WIE_KTF_DRAGONLORD_DOWNLOAD_NOTES.md` for where each of those was read
//! out of the title's own code.

use alloc::{boxed::Box, collections::BTreeMap, format, string::String, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// The download gateway 드래곤로드 dials. Its own configuration names
/// `kt68wipiwicgs.magicn.com`, but the address it opens is this one.
const FUNTER_HOST: &str = "211.115.203.17";
const FUNTER_PORT: u16 = 15102;

/// The magic every frame carries after its length.
const MAGIC: u16 = 0xffff;

const MSG_LIST: u16 = 1600;
const MSG_LIST_REPLY: u16 = 1601;
const MSG_FETCH: u16 = 1602;
const MSG_FETCH_REPLY: u16 = 1603;
const MSG_MORE: u16 = 1604;
const MSG_MORE_REPLY: u16 = 1605;
const MSG_HEARTBEAT: u16 = 30101;

/// The status that ends a file rather than failing it.
const STATUS_END: u32 = 0x83;

/// Bytes of a file one answer carries.
///
/// The title's read loop takes at most `0x800` from the socket at a time into a
/// fixed staging area and copies each of those into a buffer it mallocs for
/// whatever length the frame's header declared, so a longer frame costs another
/// turn of that loop and nothing else. What it does save is turns of the
/// title's own: it asks for the next piece from its read callback, which comes
/// round about every fifth frame it draws, so a piece a kilobyte long put a
/// three hundred kilobyte file four minutes away. It also writes what arrives
/// straight through to the database, whole file at a time, so fewer and larger
/// pieces are fewer passes over the same bytes.
const CHUNK: usize = 8192;

/// How long a record's name field is, padded with NULs.
const NAME_LEN: usize = 16;

/// One record of a listing.
const RECORD_LEN: usize = 28;

/// Answers the download for whatever the archive already carries.
pub struct FunterEndpoint {
    files: BTreeMap<String, Vec<u8>>,
    name: String,
}

impl FunterEndpoint {
    /// Takes the names and bytes the archive ships, which for a KTF package is
    /// its `P/` directory with the `P/` dropped.
    ///
    /// A name longer than a record's field is left out rather than truncated:
    /// the title would ask for the truncated name and be told there is no such
    /// file, which is a worse answer than never offering it.
    pub fn new(files: BTreeMap<String, Vec<u8>>) -> Self {
        let files: BTreeMap<_, _> = files.into_iter().filter(|(name, _)| name.len() < NAME_LEN).collect();

        Self {
            name: format!("funter({FUNTER_HOST}:{FUNTER_PORT}, {} files)", files.len()),
            files,
        }
    }

    /// Whether there is anything here to answer a download with.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl LocalEndpoint for FunterEndpoint {
    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == FUNTER_HOST && port == FUNTER_PORT
    }

    fn open(&self, _: &str, _: &str, _: u16) -> Box<dyn LocalConnection> {
        Box::new(FunterConnection {
            files: self.files.clone(),
            current: None,
            pending: Vec::new(),
            outgoing: Vec::new(),
        })
    }
}

struct FunterConnection {
    files: BTreeMap<String, Vec<u8>>,
    /// The file a `1604` means, which names none of its own.
    current: Option<String>,
    /// What the title has written and this has not made a whole frame of yet.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl FunterConnection {
    /// Puts a frame on the wire, ahead of its header.
    fn reply(&mut self, id: u16, payload: &[u8]) {
        let total = (payload.len() + 8) as u32;

        self.outgoing.extend_from_slice(&total.to_le_bytes());
        self.outgoing.extend_from_slice(&MAGIC.to_le_bytes());
        self.outgoing.extend_from_slice(&id.to_le_bytes());
        self.outgoing.extend_from_slice(payload);
    }

    /// Answers one request.
    fn handle(&mut self, id: u16, payload: &[u8]) {
        match id {
            MSG_LIST => {
                let listing = self.listing();
                tracing::info!("funter: listing {} files", self.files.len());
                self.reply(MSG_LIST_REPLY, &listing);
            }
            MSG_FETCH => self.fetch(payload),
            MSG_MORE => self.more(payload),
            // A heartbeat asks for nothing back, and answering one with a frame
            // the title is not waiting for would be a reply it drops the
            // connection over.
            MSG_HEARTBEAT => {}
            _ => tracing::warn!("funter: no answer for message {id}"),
        }
    }

    /// The `1601` payload: a status, the two words the title keeps, a count,
    /// and a record for every file.
    fn listing(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(16 + self.files.len() * RECORD_LEN);

        payload.extend_from_slice(&0u32.to_le_bytes()); // status
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(&(self.files.len() as u32).to_le_bytes());

        for (name, data) in &self.files {
            // A zeroed record is already the one the title fetches: kind 0
            // with the top bit clear, which is "download this, all of it". A
            // kind it does not know is a record it steps over without asking
            // for a byte, so the size has to land on the word at +8 and not
            // the one before it, where it reads as the kind instead.
            let mut record = [0u8; RECORD_LEN];
            record[8..12].copy_from_slice(&(data.len() as u32).to_le_bytes());
            record[12..12 + name.len()].copy_from_slice(name.as_bytes());

            payload.extend_from_slice(&record);
        }

        payload
    }

    /// The `1603` answer to `[u32 offset][the record]`: the next bytes of the
    /// file that record names.
    fn fetch(&mut self, payload: &[u8]) {
        let Some(request) = payload.get(..4 + RECORD_LEN) else {
            tracing::warn!("funter: a fetch of {} bytes names no file", payload.len());

            return self.reply(MSG_FETCH_REPLY, &1u32.to_le_bytes());
        };

        let offset = u32::from_le_bytes(request[..4].try_into().unwrap()) as usize;
        self.current = Some(record_name(&request[4..]));

        self.piece(MSG_FETCH_REPLY, offset);
    }

    /// The `1605` answer to `[u32 offset]`: more of the file the last `1602`
    /// named, which is the only one the title is writing.
    fn more(&mut self, payload: &[u8]) {
        let Some(offset) = payload.get(..4) else {
            tracing::warn!("funter: a continuation of {} bytes carries no offset", payload.len());

            return self.reply(MSG_MORE_REPLY, &1u32.to_le_bytes());
        };

        self.piece(MSG_MORE_REPLY, u32::from_le_bytes(offset.try_into().unwrap()) as usize);
    }

    /// One piece of the file under way, or the end status when there are no
    /// bytes left of it.
    fn piece(&mut self, id: u16, offset: usize) {
        let Some(name) = self.current.clone() else {
            tracing::warn!("funter: asked for more of no file");

            return self.reply(id, &1u32.to_le_bytes());
        };

        let Some(data) = self.files.get(&name) else {
            tracing::warn!("funter: {name:?} is not in this archive");

            return self.reply(id, &1u32.to_le_bytes());
        };

        if offset >= data.len() {
            tracing::debug!("funter: {name:?} finished at {offset} bytes");

            return self.reply(id, &STATUS_END.to_le_bytes());
        }

        let end = (offset + CHUNK).min(data.len());
        let chunk = &data[offset..end];

        tracing::debug!("funter: {name:?} {offset}..{end} of {}", data.len());

        let mut answer = Vec::with_capacity(8 + chunk.len());
        answer.extend_from_slice(&0u32.to_le_bytes()); // status
        answer.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        answer.extend_from_slice(chunk);

        self.reply(id, &answer);
    }
}

/// The name out of a record: NUL-padded, sixteen bytes at its end.
fn record_name(record: &[u8]) -> String {
    let field = record.get(12..RECORD_LEN).unwrap_or_default();
    let end = field.iter().position(|&byte| byte == 0).unwrap_or(field.len());

    String::from_utf8_lossy(&field[..end]).into_owned()
}

impl LocalConnection for FunterConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // Whole frames only: a title writing in pieces is one this has to wait
        // for, the same way the socket under it would.
        while self.pending.len() >= 8 {
            let total = u32::from_le_bytes(self.pending[..4].try_into().unwrap()) as usize;
            if total < 8 || self.pending.len() < total {
                break;
            }

            let frame = self.pending.drain(..total).collect::<Vec<_>>();
            let id = u16::from_le_bytes(frame[6..8].try_into().unwrap());

            self.handle(id, &frame[8..]);
        }
    }

    fn read(&mut self, out: &mut [u8]) -> LocalRead {
        if self.outgoing.is_empty() {
            return LocalRead::Pending;
        }

        let taken = out.len().min(self.outgoing.len());
        out[..taken].copy_from_slice(&self.outgoing[..taken]);
        self.outgoing.drain(..taken);

        LocalRead::Data(taken)
    }

    fn readable(&self) -> bool {
        !self.outgoing.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeMap, string::ToString, vec, vec::Vec};

    use super::{FunterEndpoint, LocalConnection, LocalEndpoint, LocalRead, MAGIC, RECORD_LEN, STATUS_END};

    fn frame(id: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&MAGIC.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(payload);

        out
    }

    fn read_frame(connection: &mut dyn LocalConnection) -> (u16, Vec<u8>) {
        let mut header = [0u8; 8];
        let LocalRead::Data(8) = connection.read(&mut header) else {
            panic!("no header");
        };

        let total = u32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
        assert_eq!(u16::from_le_bytes(header[4..6].try_into().unwrap()), MAGIC);

        let mut payload = vec![0u8; total - 8];
        if !payload.is_empty() {
            let LocalRead::Data(taken) = connection.read(&mut payload) else {
                panic!("no payload");
            };
            assert_eq!(taken, payload.len());
        }

        (u16::from_le_bytes(header[6..8].try_into().unwrap()), payload)
    }

    fn endpoint() -> FunterEndpoint {
        // Longer than one piece, so a fetch of it has to be continued.
        let files = [("dragon.vlu".to_string(), vec![7u8; 20000]), ("script.csd".to_string(), vec![1u8; 4])]
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        FunterEndpoint::new(files)
    }

    /// The listing names every file the archive carries, with its length.
    #[test]
    fn a_listing_names_what_the_archive_has() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);
        connection.write(&frame(1600, &[0; 12]));

        let (id, payload) = read_frame(&mut *connection);
        assert_eq!(id, 1601);
        assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 0, "status");
        assert_eq!(u32::from_le_bytes(payload[12..16].try_into().unwrap()), 2, "count");

        let record = &payload[16..16 + RECORD_LEN];
        assert_eq!(record[4] & 0x7f, 0, "kind");
        assert_eq!(u32::from_le_bytes(record[8..12].try_into().unwrap()), 20000);
        assert_eq!(&record[12..22], b"dragon.vlu");
    }

    /// A fetch hands back the file a piece at a time, each next piece asked for
    /// by the offset alone, until the file is whole.
    #[test]
    fn a_file_comes_back_in_pieces() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);
        connection.write(&frame(1600, &[0; 12]));
        let _ = read_frame(&mut *connection);

        let mut record = [0u8; RECORD_LEN];
        record[8..12].copy_from_slice(&20000u32.to_le_bytes());
        record[12..22].copy_from_slice(b"dragon.vlu");

        let mut request = 0u32.to_le_bytes().to_vec();
        request.extend_from_slice(&record);
        connection.write(&frame(1602, &request));

        let mut taken = Vec::new();
        for piece in 0..8 {
            let (id, payload) = read_frame(&mut *connection);
            assert_eq!(id, if piece == 0 { 1603 } else { 1605 });
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 0, "status");

            let length = u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;
            assert_eq!(length, payload.len() - 8);
            taken.extend_from_slice(&payload[8..]);

            if taken.len() >= 20000 {
                break;
            }

            // What the title asks next: this file, from where it has got to.
            connection.write(&frame(1604, &(taken.len() as u32).to_le_bytes()));
        }

        assert_eq!(taken, vec![7u8; 20000]);
    }

    /// Past the end of the file is the status the title reads as its end, not
    /// an error and not somebody else's bytes.
    #[test]
    fn past_the_end_of_a_file_is_the_end() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);

        let mut record = [0u8; RECORD_LEN];
        record[12..22].copy_from_slice(b"script.csd");
        let mut request = 0u32.to_le_bytes().to_vec();
        request.extend_from_slice(&record);
        connection.write(&frame(1602, &request));
        let _ = read_frame(&mut *connection);

        connection.write(&frame(1604, &4u32.to_le_bytes()));

        let (id, payload) = read_frame(&mut *connection);
        assert_eq!(id, 1605);
        assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), STATUS_END);
    }

    /// A name the archive does not carry is refused rather than answered with
    /// somebody else's bytes.
    #[test]
    fn a_file_the_archive_lacks_is_refused() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);

        let mut record = [0u8; RECORD_LEN];
        record[12..20].copy_from_slice(b"nowhere.");
        let mut request = 0u32.to_le_bytes().to_vec();
        request.extend_from_slice(&record);
        connection.write(&frame(1602, &request));

        let (id, payload) = read_frame(&mut *connection);
        assert_eq!(id, 1603);
        assert_ne!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 0);
    }

    /// A heartbeat is not a question, and answering one would be a frame the
    /// title is not waiting for.
    #[test]
    fn a_heartbeat_is_left_alone() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);
        connection.write(&frame(30101, &[]));

        assert!(!connection.readable());
        assert_eq!(connection.read(&mut [0u8; 8]), LocalRead::Pending);
    }

    /// A title that writes its request in pieces is waited for.
    #[test]
    fn a_frame_split_across_writes_is_still_one_frame() {
        let mut connection = endpoint().open("socket", "211.115.203.17", 15102);
        let request = frame(1600, &[0; 12]);

        connection.write(&request[..5]);
        assert!(!connection.readable());

        connection.write(&request[5..]);
        assert_eq!(read_frame(&mut *connection).0, 1601);
    }

    /// Only the address measured off the title.
    #[test]
    fn only_the_download_gateway_is_answered() {
        let endpoint = endpoint();

        assert!(endpoint.accepts("socket", "211.115.203.17", 15102));
        assert!(!endpoint.accepts("socket", "211.115.203.17", 15103));
        assert!(!endpoint.accepts("http", "211.115.203.17", 15102));
        assert!(!endpoint.accepts("socket", "222.237.78.175", 15102));
    }

    /// A name a record cannot carry is left out of the listing.
    #[test]
    fn a_name_too_long_for_a_record_is_not_offered() {
        let files = [("a-very-long-file-name.dat".to_string(), vec![0u8; 1])].into_iter().collect();

        assert!(FunterEndpoint::new(files).is_empty());
    }
}
