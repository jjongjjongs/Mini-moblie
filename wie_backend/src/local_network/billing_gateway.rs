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

/// Answers the carrier billing frames a title writes over a plain socket to one
/// of these servers.
pub struct BillingGatewayEndpoint {
    name: &'static str,
    host: &'static str,
    port: u16,
}

impl BillingGatewayEndpoint {
    /// Answers `host:port`. Both are the server a title dials, spelled the way
    /// it spells them (a dotted quad for a title that resolved its own host).
    pub const fn new(name: &'static str, host: &'static str, port: u16) -> Self {
        Self { name, host, port }
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
            pending: Vec::new(),
            outgoing: Vec::new(),
        })
    }
}

struct BillingGatewayConnection {
    peer: alloc::string::String,
    /// What the title has written that is not yet a whole frame.
    pending: Vec<u8>,
    /// What is waiting to be read back.
    outgoing: Vec<u8>,
}

impl BillingGatewayConnection {
    /// Takes one whole billing frame off the front of `pending`, or `None` when
    /// a whole one has not arrived yet. A frame that is not one of these - no
    /// `0xffff` marker, or a length that describes nothing this could be - is
    /// dropped a byte at a time rather than buffered forever.
    fn take_frame(&mut self) -> Option<Vec<u8>> {
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
}

impl LocalConnection for BillingGatewayConnection {
    fn write(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);

        // One write is not always one frame, so take whole frames only and
        // leave any tail for the write that completes it.
        while let Some(frame) = self.take_frame() {
            match crate::billing::response(&frame) {
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
    use super::{BillingGatewayConnection, LocalConnection, LocalRead};
    use alloc::{vec, vec::Vec};

    fn connection() -> BillingGatewayConnection {
        BillingGatewayConnection {
            peer: "test:1".into(),
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
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
}
