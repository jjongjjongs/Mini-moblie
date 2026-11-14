//! 트릭스터's community server (LGT `0003263B`), answered in process.
//!
//! The community menu opens an LGT billing socket - `MC_netSocketConnect` in
//! billing mode 1 to `210.217.196.90:10052` - and writes its requests through
//! the carrier gateway's header. The server is long gone; dialed, the login
//! went unanswered and the title sat on its connecting screen.
//!
//! A frame each way is `[u16le length][u16le command][body]`, the length
//! counting the body alone. The title builds requests with `0x8164` and reads
//! replies in `0x8340`, which waits for `length + 4` bytes, then `0x826c`,
//! which takes the body's first byte as the result: 0 (or `0x29`) is success,
//! with the reply's data behind it; anything else is an error code and a
//! message. Integers in a body are little-endian (`0x2e60c`, `0x2e61c`).
//!
//! The community opens with two requests:
//!
//! - `0x191`, the login: the handset's twelve-byte number (`0x8af8`). Its
//!   reply data is a sixteen-byte session key, which `0x8b88` keeps at the
//!   context's `+5` and sends with every request after.
//! - `0x2bd`, sent at once on a successful login with the key and a byte
//!   `0x64` (`0x8ce4`). `0x8d58` reads its reply as a u32 and a count of
//!   strings, each `[u16le length][bytes]`; answered with none, the title goes
//!   on into the community.
//!
//! The shop buys over the same socket, with no login before it:
//!
//! - `0x65`, a purchase: the handset's number, then the item and one byte
//!   more (`0x9cec`; `07 00` on the device log). `0x9d78` reads its reply as a
//!   byte, a count of online coupons, and that many `0x42`-byte coupon
//!   records. With no coupons the title hands the item over itself and says
//!   `'%s' 획득하셧습니다` (`0x1557c`); one or two are the PC game's coupons
//!   (`온라인 아이템을 획득하셧습니다|아이템:%s|쿠폰번호:%s`), which there is
//!   nothing behind any more.
//!
//! Every other request is left unanswered and traced, so a device log names
//! the next one to shape.

use alloc::vec::Vec;

/// The title's application id.
pub const AID: &str = "0003263B";

const HEADER: usize = 4;

const LOG_IN: u16 = 0x191;
const AFTER_LOG_IN: u16 = 0x2bd;
const PURCHASE: u16 = 0x65;

/// The result byte a successful reply opens its body with.
const OK: u8 = 0;

/// The session key the login hands out. The server made one per login; the
/// title only keeps it and sends it back.
const SESSION_KEY: [u8; 16] = *b"MINIMOBILE-LOCAL";

/// The reply to one request frame, for the requests this knows.
pub fn response(request: &[u8]) -> Option<Vec<u8>> {
    let length = u16::from_le_bytes([*request.first()?, *request.get(1)?]) as usize;
    let command = u16::from_le_bytes([*request.get(2)?, *request.get(3)?]);
    if request.len() != HEADER + length {
        return None;
    }

    match command {
        LOG_IN => {
            tracing::info!("트릭스터 community: logged in");
            Some(reply(command, &SESSION_KEY))
        }
        AFTER_LOG_IN => {
            // A u32, then how many strings follow: none.
            let mut data = Vec::new();
            data.extend_from_slice(&0u32.to_le_bytes());
            data.push(0);

            Some(reply(command, &data))
        }
        PURCHASE => {
            tracing::info!("트릭스터 shop: purchase granted");
            // A byte the title does not look at, then no coupons.
            Some(reply(command, &[0, 0]))
        }
        _ => None,
    }
}

fn reply(command: u16, data: &[u8]) -> Vec<u8> {
    let length = 1 + data.len();
    let mut reply = Vec::with_capacity(HEADER + length);
    reply.extend_from_slice(&(length as u16).to_le_bytes());
    reply.extend_from_slice(&command.to_le_bytes());
    reply.push(OK);
    reply.extend_from_slice(data);

    reply
}

#[cfg(test)]
mod tests {
    use super::response;

    /// The login off the device log: the handset's number, twelve bytes.
    #[test]
    fn the_login_is_answered_with_a_session_key() {
        let login = [
            0x0c, 0x00, 0x91, 0x01, b'0', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'2',
        ];

        let reply = response(&login).unwrap();

        assert_eq!(&reply[..4], &[17, 0, 0x91, 0x01]);
        assert_eq!(reply[4], 0, "success");
        assert_eq!(reply.len(), 4 + 1 + 16);
    }

    /// The request the title sends next, with the key and `0x64`.
    #[test]
    fn the_request_after_the_login_is_answered_with_nothing_listed() {
        let mut request = alloc::vec![0x11, 0x00, 0xbd, 0x02];
        request.extend_from_slice(b"MINIMOBILE-LOCAL");
        request.push(0x64);

        assert_eq!(response(&request).unwrap(), [6, 0, 0xbd, 0x02, 0, 0, 0, 0, 0, 0]);
    }

    /// A purchase off the device log: the number, item 7 and a zero.
    #[test]
    fn a_purchase_is_granted_with_no_coupons() {
        let purchase = [
            0x0e, 0x00, 0x65, 0x00, b'0', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'1', b'2', 0x07, 0x00,
        ];

        assert_eq!(response(&purchase).unwrap(), [3, 0, 0x65, 0x00, 0, 0, 0]);
    }

    #[test]
    fn an_unknown_or_cut_frame_is_left_alone() {
        assert!(response(&[0x00, 0x00, 0x21, 0x03]).is_none());
        assert!(response(&[0x0c, 0x00, 0x91, 0x01, b'0']).is_none());
    }
}
