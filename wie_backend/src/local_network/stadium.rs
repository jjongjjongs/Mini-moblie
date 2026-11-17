//! The point shop 스타디움2011 아시아 (SKT `0090410665`) reaches through
//! [`super::gxg`].
//!
//! The title asks the gateway for `222.231.31.45`, port `16424`, and sends its
//! requests as the gateway's data. Each message either way is `ffff`, a
//! little-endian `u16` length counting the whole message, a little-endian
//! `u16` command and a body. The reader (`0x10a52`) takes the four header
//! bytes, makes a buffer of the length (`0xf2de`), fills it from offset 4 and
//! hands it to `0x10864`, which reads the command at offset 4. A handful of
//! commands have handlers of their own; anything else goes to the handler of
//! the shop's state (`+0x20` of `0x10f25a0`).
//!
//! A point purchase is the shop's state 1: `0xf246` builds command `0x66` -
//! the MIN, a byte, the package as a `u16` and one byte more - and the answer
//! goes to `0xff82`, which reads one result byte. 0 is the purchase done
//! (popup `0x75`); 1 to 6 are its failures.

use alloc::vec::Vec;

/// Where the title asks the gateway to carry it.
pub const TARGET: (&str, u16) = ("222.231.31.45", 16424);

const MAGIC: [u8; 2] = [0xff, 0xff];
const HEADER: usize = 6;

const BUY_POINTS: u16 = 0x66;
const BOUGHT: u8 = 0;

fn message(command: u16, body: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(HEADER + body.len());
    message.extend_from_slice(&MAGIC);
    message.extend_from_slice(&((HEADER + body.len()) as u16).to_le_bytes());
    message.extend_from_slice(&command.to_le_bytes());
    message.extend_from_slice(body);
    message
}

/// The shop's answer to the title's point purchase, or `None` for anything
/// else it sends this server.
pub fn point_answer(request: &[u8]) -> Option<Vec<u8>> {
    if request.len() < HEADER || request[..2] != MAGIC || u16::from_le_bytes([request[2], request[3]]) as usize != request.len() {
        return None;
    }

    match u16::from_le_bytes([request[4], request[5]]) {
        // Answered with the request's own command, which `0x10864` has no
        // handler of its own for, so it reaches the state's.
        BUY_POINTS => {
            tracing::info!("stadium: granting the points");
            Some(message(BUY_POINTS, &[BOUGHT]))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::point_answer;

    #[test]
    fn a_purchase_is_granted() {
        // The request as the title sent it on the device.
        let mut request = [0xff, 0xff, 0x15, 0, 0x66, 0].to_vec();
        request.extend_from_slice(b"01046119269");
        request.extend_from_slice(&[0, 4, 0, 0]);
        assert_eq!(point_answer(&request).unwrap(), [0xff, 0xff, 7, 0, 0x66, 0, 0]);
    }

    #[test]
    fn other_requests_are_left_alone() {
        assert_eq!(point_answer(&[0xff, 0xff, 6, 0, 0x67, 0]), None);
        // A length that is not the message's.
        assert_eq!(point_answer(&[0xff, 0xff, 9, 0, 0x66, 0]), None);
    }
}
