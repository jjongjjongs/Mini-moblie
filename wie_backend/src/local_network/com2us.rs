//! Com2uS's star shop, which 초코초코타이쿤2 (SKT `9000516933`) reaches
//! through [`super::gxg`].
//!
//! The title asks the gateway for `211.115.66.252`, port `15135`, and sends
//! its requests as the gateway's data. Each is a `u16be` length counting the
//! whole message, a byte, a type byte, a byte, and a body; the library's
//! decoder (`0x1297c`) reads an answer the same way and turns its type into
//! the event the shop's callback (`0x1d5a0`) goes on. A purchase of stars
//! (`0x1df08`, state `0x1006db0`) runs three steps:
//!
//! | sent | built at  | body                         | answer | event  |
//! |------|-----------|------------------------------|--------|--------|
//! | 0x00 | `0x6e480` | `03ee`, the build, the MIN   | 0xc8   | `0x22` |
//! | 0xc9 | `0x6e360` | `0014`, the stars, ...       | 0xca   | `0x24` |
//! | 0xcb | `0x6e314` | the `u32` and 8 bytes of 0xca| 0xcc   | `0x26` |
//!
//! `0xc8` has no body and moves the shop on to `0xc9`. `0xca` carries a `u32`
//! and eight bytes the title keeps and sends back in `0xcb`. `0xcc` carries
//! the stars bought as a `u32`, which `0x1d642` adds to the title's own and
//! shows as `[별 구매 완료]`. The stars travel in the `u32` of `0xca`, so the
//! last step knows them without anything kept here.

use alloc::{vec, vec::Vec};

/// Where the title asks the gateway to carry it.
pub const TARGET: (&str, u16) = ("211.115.66.252", 15135);

const HEADER: usize = 5;

const OPEN: u8 = 0x00;
const OPEN_COMMAND: u16 = 0x03ee;
const OPENED: u8 = 0xc8;
const ORDER: u8 = 0xc9;
const ORDERED: u8 = 0xca;
const CONFIRM: u8 = 0xcb;
const CONFIRMED: u8 = 0xcc;

fn message(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut message = vec![0, 0, 0, kind, 1];
    message.extend_from_slice(body);
    let length = message.len() as u16;
    message[..2].copy_from_slice(&length.to_be_bytes());
    message
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The shop's answer to one of the title's purchase requests, or `None` for
/// anything else it sends this server.
pub fn star_answer(request: &[u8]) -> Option<Vec<u8>> {
    if request.len() < HEADER || u16::from_be_bytes([request[0], request[1]]) as usize != request.len() {
        return None;
    }
    let body = &request[HEADER..];

    match request[3] {
        OPEN if body.get(..2) == Some(&OPEN_COMMAND.to_be_bytes()[..]) => Some(message(OPENED, &[])),
        ORDER => {
            let stars = be_u32(body, 2)?;
            tracing::info!("com2us: granting {stars} stars");
            let mut answer = stars.to_be_bytes().to_vec();
            answer.extend_from_slice(&[0; 8]);
            Some(message(ORDERED, &answer))
        }
        CONFIRM => Some(message(CONFIRMED, &be_u32(body, 0)?.to_be_bytes())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::star_answer;

    fn decode(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn a_purchase_runs_through() {
        // The first request, as the title sent it.
        let open = decode(
            "006700000103ee000000074354325f534b545f7630312e30302e353200000030313034363131393236390000000000000000000000000000000000000000000000000000000000000043484f5441325f534b545f465245455f494e5354414c4c00000000000000",
        );
        assert_eq!(star_answer(&open).unwrap(), [0, 5, 0, 0xc8, 1]);

        // 300 stars, as `0x6e360` builds it.
        let order = [0, 0x0d, 0, 0xc9, 1, 0, 0x14, 0, 0, 0x01, 0x2c, 0, 0];
        let ordered = star_answer(&order).unwrap();
        assert_eq!(&ordered[..5], [0, 0x11, 0, 0xca, 1]);
        assert_eq!(ordered.len(), 0x11);

        // `0x6e314` sends back what `0xca` carried.
        let mut confirm = vec![0, 0x11, 0, 0xcb, 1];
        confirm.extend_from_slice(&ordered[5..]);
        assert_eq!(star_answer(&confirm).unwrap(), [0, 9, 0, 0xcc, 1, 0, 0, 0x01, 0x2c]);
    }

    #[test]
    fn other_requests_are_left_alone() {
        // The SMS question the same server is asked: the same type, another
        // command.
        let mut sms = decode("004900000003f9000000074354325f534b545f7630312e30302e3532");
        sms.resize(0x49, 0);
        assert_eq!(star_answer(&sms), None);
    }
}
