//! The carrier billing frames a plain socket carries, answered in process.
//!
//! Some titles do not open a `MC_netBillSocket` for a purchase: they open an
//! ordinary stream socket to the billing server and write the carrier's billing
//! frame over it by hand. The frame is the one [`crate::billing`] already reads -
//! `[0xffff][u16 length][u16 type][body]`, the length counting the whole frame -
//! so the answer is the same `granted` reply the billing gateway builds, only
//! here the socket carries no `WPBill` header around it.
//!
//! 질주쾌감 스케쳐2's shop is one: a purchase opens `222.231.31.45:28013` and
//! writes `ff ff 12 00 68 00 <subscriber> 03`, the `0x68` purchase command, then
//! reads the reply. Dialed at a dead server the read never answers and the shop
//! shows `구매 실패`; answered here it reads the `granted` frame and the purchase
//! goes through.
//!
//! Host-gated to the servers a title is known to reach this way, so no ordinary
//! connection is touched. Each complete frame is answered by
//! [`crate::billing::response`]; a frame it does not recognise is left
//! unanswered, the same as anywhere else.

use alloc::{boxed::Box, vec::Vec};

use super::{LocalConnection, LocalEndpoint, LocalRead};

/// A billing frame's fixed head: the `0xffff` marker and the `u16` length that
/// follows it. The length counts the whole frame, marker included.
const FRAME_HEAD: usize = 4;

/// A frame longer than this is refused rather than buffered without bound.
const MAX_FRAME: usize = 4096;

/// How a title lays out the frames it writes over the socket.
#[derive(Clone, Copy)]
enum Framing {
    /// `[0xffff][u16le length counting the whole frame][body]` - the carrier
    /// billing frame 질주쾌감 스케쳐2 and its family write by hand.
    WpBillMarker,
    /// `[u16be length counting the whole frame][body]` - the GP4 login family
    /// (컴투스), where the length is the first field and there is no marker. See
    /// [`crate::billing::lgt_local_apf2_response`].
    BigEndianLength,
    /// `[KP][u16le length counting the whole frame][...]` - the tagged record
    /// 크로이센 buys with. See [`crate::billing::lgt_local_tagged_record_response`].
    KpTagged,
    /// The GP4 frames 템페스트 writes through the `FastRelay` library. See
    /// [`crate::billing::ktf_local_tempest_response`].
    Relay,
}

/// Answers the carrier billing frames a title writes over a plain socket to one
/// of these servers.
pub struct BillingGatewayEndpoint {
    name: &'static str,
    host: &'static str,
    port: u16,
    framing: Framing,
}

impl BillingGatewayEndpoint {
    /// Answers `host:port` for the `0xffff`-marked carrier billing frame. Both
    /// are the server a title dials, spelled the way it spells them (a dotted
    /// quad for a title that resolved its own host).
    pub const fn new(name: &'static str, host: &'static str, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            framing: Framing::WpBillMarker,
        }
    }

    /// Answers `host:port` for the big-endian length-prefixed GP4 login family -
    /// 액션퍼즐패밀리2, 미니게임천국4 and the other 컴투스 titles that open a plain
    /// socket to `211.115.66.250:15133` and write `[u16be length][u16 type]
    /// [0x30 ...]` frames with no marker in front.
    pub const fn new_length_prefixed(name: &'static str, host: &'static str, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            framing: Framing::BigEndianLength,
        }
    }

    /// Answers `host:port` for the `KP` tagged record - 크로이센's KTF build,
    /// which opens a plain socket to its shop server and writes the same record
    /// its LGT build hands the billing gateway.
    pub const fn new_kp_tagged(name: &'static str, host: &'static str, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            framing: Framing::KpTagged,
        }
    }

    /// Answers `host:port` for a title that reaches its server through the
    /// `FastRelay` carrier library - 템페스트's 정품인증.
    pub const fn new_relay(name: &'static str, host: &'static str, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            framing: Framing::Relay,
        }
    }
}

impl LocalEndpoint for BillingGatewayEndpoint {
    fn name(&self) -> &str {
        self.name
    }

    fn accepts(&self, scheme: &str, host: &str, port: u16) -> bool {
        scheme == "socket" && host == self.host && port == self.port
    }

    fn open(&self, _scheme: &str, _host: &str, _port: u16) -> Box<dyn LocalConnection> {
        Box::new(BillingGatewayConnection {
            peer: alloc::format!("{}:{}", self.host, self.port),
            framing: self.framing,
            pending: Vec::new(),
            outgoing: Vec::new(),
        })
    }
}

struct BillingGatewayConnection {
    peer: alloc::string::String,
    framing: Framing,
    /// What the title has written that is not yet a whole frame.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl BillingGatewayConnection {
    /// Takes one whole billing frame off the front of `pending`, or `None` when
    /// a whole one has not arrived yet.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
        match self.framing {
            Framing::WpBillMarker => self.take_wpbill_frame(),
            Framing::BigEndianLength | Framing::Relay => self.take_length_prefixed_frame(),
            Framing::KpTagged => self.take_kp_tagged_frame(),
        }
    }

    /// `[KP][u16le length][...]`, the length counting the whole frame. Bytes in
    /// front of a tag are dropped, and so is a tag whose length could not be a
    /// frame, rather than buffered forever.
    fn take_kp_tagged_frame(&mut self) -> Option<Vec<u8>> {
        const TAG: &[u8] = b"KP";
        /// The tag, the length, the shape, the record byte and one more.
        const HEADER: usize = 8;

        while self.pending.len() >= 2 && !self.pending.starts_with(TAG) {
            self.pending.remove(0);
        }

        if self.pending.len() < FRAME_HEAD {
            return None;
        }

        let length = u16::from_le_bytes([self.pending[2], self.pending[3]]) as usize;
        if !(HEADER..=MAX_FRAME).contains(&length) {
            self.pending.drain(..2);
            return None;
        }

        if self.pending.len() < length {
            return None;
        }

        Some(self.pending.drain(..length).collect())
    }

    /// `[0xffff][u16le length][body]`. A frame that is not one of these - no
    /// `0xffff` marker, or a length that describes nothing this could be - is
    /// dropped a byte at a time rather than buffered forever.
    fn take_wpbill_frame(&mut self) -> Option<Vec<u8>> {
        while self.pending.len() >= 2 && (self.pending[0] != 0xff || self.pending[1] != 0xff) {
            self.pending.remove(0);
        }

        if self.pending.len() < FRAME_HEAD {
            return None;
        }

        // The length the title wrote, little end first - which is the order
        // 질주쾌감 스케쳐2 and the rest of this family write it in. It counts the
        // whole frame.
        let length = u16::from_le_bytes([self.pending[2], self.pending[3]]) as usize;
        if !(FRAME_HEAD..=MAX_FRAME).contains(&length) {
            // Not a length this frame could carry; step over the marker and try
            // again rather than wait for bytes that will never make it whole.
            self.pending.drain(..2);
            return None;
        }

        if self.pending.len() < length {
            return None;
        }

        Some(self.pending.drain(..length).collect())
    }

    /// `[u16be length][body]`, the length the first field and counting the whole
    /// frame - the GP4 login family's shape. There is no marker to resync on, so
    /// a length too small to be a frame steps over one byte and tries again.
    fn take_length_prefixed_frame(&mut self) -> Option<Vec<u8>> {
        if self.pending.len() < 2 {
            return None;
        }

        // A GP4 frame is at least its own two-byte length and a two-byte type.
        let length = u16::from_be_bytes([self.pending[0], self.pending[1]]) as usize;
        if !(4..=MAX_FRAME).contains(&length) {
            self.pending.remove(0);
            return None;
        }

        if self.pending.len() < length {
            return None;
        }

        Some(self.pending.drain(..length).collect())
    }
}

impl LocalConnection for BillingGatewayConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // One write is not always one frame, so take whole frames only and
        // leave any tail for the write that completes it.
        while let Some(frame) = self.take_frame() {
            let reply = match self.framing {
                Framing::Relay => crate::billing::ktf_local_tempest_response(&frame),
                _ => crate::billing::response(&frame),
            };
            match reply {
                Some(reply) => {
                    tracing::info!(
                        "billing gateway {}: {} -> {}",
                        self.peer,
                        crate::billing::bill_frame_trace(&frame),
                        crate::billing::bill_frame_trace(&reply)
                    );
                    self.outgoing.extend_from_slice(&reply);
                }
                None => {
                    tracing::info!(
                        "billing gateway {}: no reply shaped for {}",
                        self.peer,
                        crate::billing::bill_frame_trace(&frame)
                    );
                }
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

#[cfg(test)]
mod tests {
    use super::{BillingGatewayConnection, Framing, LocalConnection, LocalRead};
    use alloc::{vec, vec::Vec};

    fn connection() -> BillingGatewayConnection {
        BillingGatewayConnection {
            peer: "test:1".into(),
            framing: Framing::WpBillMarker,
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    fn gp4_connection() -> BillingGatewayConnection {
        BillingGatewayConnection {
            peer: "211.115.66.250:15133".into(),
            framing: Framing::BigEndianLength,
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    fn kp_connection() -> BillingGatewayConnection {
        BillingGatewayConnection {
            peer: "222.231.57.145:56000".into(),
            framing: Framing::KpTagged,
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    /// 크로이센's purchase, split across two writes, is taken whole and granted
    /// in the twelve bytes its reader asks for.
    #[test]
    fn a_kp_tagged_purchase_is_granted() {
        let purchase: &[u8] = &[
            0x4b, 0x50, 0x24, 0x00, 0x08, 0x00, 0x02, 0x00, 0x30, 0x31, 0x30, 0x32, 0x33, 0x38, 0x36, 0x37, 0x36, 0x36, 0x39, 0x00, 0x30, 0x30, 0x30,
            0x32, 0x43, 0x43, 0x43, 0x42, 0x30, 0x30, 0x37, 0x00, 0xdc, 0x05, 0x00, 0x00,
        ];

        let mut connection = kp_connection();
        connection.write(&purchase[..10]);
        assert!(!connection.readable());
        connection.write(&purchase[10..]);

        assert_eq!(
            read_all(&mut connection),
            [0x4b, 0x50, 0x0c, 0x00, 0x08, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00]
        );
    }

    fn read_all(connection: &mut BillingGatewayConnection) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        match connection.read(&mut out) {
            LocalRead::Data(read) => out[..read].to_vec(),
            other => panic!("expected data, got {other:?}"),
        }
    }

    /// 질주쾌감 스케쳐2's purchase, byte for byte off the wire, is answered with
    /// the family's `granted` frame - the same message type plus one, a zero
    /// status.
    #[test]
    fn a_purchase_is_granted() {
        let purchase: &[u8] = &[
            0xff, 0xff, 0x12, 0x00, 0x68, 0x00, 0x30, 0x31, 0x30, 0x34, 0x36, 0x31, 0x31, 0x39, 0x32, 0x36, 0x39, 0x03,
        ];

        let mut connection = connection();
        connection.write(purchase);

        assert_eq!(read_all(&mut connection), vec![0xff, 0xff, 0x07, 0x00, 0x69, 0x00, 0x00]);
    }

    /// A frame split across two writes is answered once it is whole, and not
    /// before.
    #[test]
    fn a_split_frame_is_answered_when_whole() {
        let purchase: &[u8] = &[
            0xff, 0xff, 0x12, 0x00, 0x68, 0x00, 0x30, 0x31, 0x30, 0x34, 0x36, 0x31, 0x31, 0x39, 0x32, 0x36, 0x39, 0x03,
        ];

        let mut connection = connection();
        connection.write(&purchase[..5]);
        assert!(!connection.readable(), "nothing to read before the frame is whole");

        connection.write(&purchase[5..]);
        assert!(connection.readable(), "the whole frame is answered");
    }

    /// A GP4 login frame - a big-endian length that counts the whole frame, a
    /// two-byte type, a `0x30` at the head of the payload - is read off the
    /// length-prefixed framing and answered with the family's granted reply:
    /// the echoed type under a length-eight frame with an all-zero status.
    #[test]
    fn a_gp4_login_is_granted() {
        // The 73-byte type-0 login the capture caught, trimmed to its header -
        // what `lgt_local_apf2_response` keys on - padded back to its length.
        let mut login = vec![0x00, 0x49, 0x00, 0x00, 0x30, 0x03, 0xf9];
        login.resize(0x49, 0x00);

        let mut connection = gp4_connection();
        connection.write(&login);

        assert_eq!(read_all(&mut connection), vec![0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    /// The length-prefixed framing waits for the whole frame before it answers,
    /// the same as the marked one.
    #[test]
    fn a_split_gp4_frame_is_answered_when_whole() {
        let frame: &[u8] = &[0x00, 0x05, 0x00, 0x01, 0x30];

        let mut connection = gp4_connection();
        connection.write(&frame[..3]);
        assert!(!connection.readable(), "nothing before the frame is whole");

        connection.write(&frame[3..]);
        assert_eq!(read_all(&mut connection), vec![0x00, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
    }
}
