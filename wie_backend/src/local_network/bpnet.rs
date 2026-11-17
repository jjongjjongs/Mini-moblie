//! The authentication server 슈퍼프로야구2012 (SKT `0094744852`) checks its
//! subscriber with before the title screen.
//!
//! The title's network library (`_BPNET_SKTGXG`) asks Nexon's GXG gateway
//! ([`super::gxg`]) to carry it to `222.231.57.171`, port `20010`, and sends
//! its messages as the gateway's data. Each message either way is a
//! big-endian `u16` length counting the whole message, a big-endian `u32`
//! command, two bytes nobody reads and the command's body (`0x181e30` writes
//! the head; `0x1815ac` reads it back).
//!
//! The authentication is command `0x10000` (`0x181fc0`): the game's code
//! `0xe6`, the subscriber number, the build's versions, the product id and
//! the phone model. The library's own reader of the answer (`0x1824fc`)
//! takes 256 bytes; the title's (`0x11a124`) takes the same 256 bytes and
//! one result byte, 1 being authenticated - anything else is the
//! `인증에 실패` popup. The server is long gone, so every launch stopped
//! there.
//!
//! A shop purchase is command `0x40000` (`0x18244e`): the authentication's
//! fields, two bytes, the price (3900 for the first item) and the item as
//! big-endian `u32`s, and a 20-byte field. The library reads the answer's
//! two bytes and six words only to log them (`0x1828ca`); the title takes
//! the command itself as the purchase done (`0x11a112`) and shows the item's
//! popup. Left unanswered, the shop gave up on it.

use alloc::{vec, vec::Vec};

/// Where the title asks the gateway to carry it.
pub const TARGET: (&str, u16) = ("222.231.57.171", 20010);

const HEADER: usize = 8;

const AUTHENTICATE: u32 = 0x10000;
const NOTICE: usize = 256;
const AUTHENTICATED: u8 = 1;

const PURCHASE: u32 = 0x40000;
const PURCHASE_ANSWER: usize = 2 + 6 * 4;

/// The answer to the title's message, for the messages it is known for.
pub fn answer(data: &[u8]) -> Option<Vec<u8>> {
    let Some(command) = command(data) else {
        tracing::info!("bpnet: no message in {} bytes {:02x?}", data.len(), data);
        return None;
    };

    match command {
        AUTHENTICATE => {
            tracing::info!("bpnet: authenticated");
            let mut body = vec![0; NOTICE];
            body.push(AUTHENTICATED);
            Some(message(AUTHENTICATE, &body))
        }
        PURCHASE => {
            tracing::info!("bpnet: purchase granted");
            Some(message(PURCHASE, &[0; PURCHASE_ANSWER]))
        }
        command => {
            tracing::info!("bpnet: no answer known for command {command:#x}, {} bytes {:02x?}", data.len(), data);
            None
        }
    }
}

/// The command a whole message names.
fn command(data: &[u8]) -> Option<u32> {
    if data.len() < HEADER || u16::from_be_bytes([data[0], data[1]]) as usize != data.len() {
        return None;
    }
    Some(u32::from_be_bytes([data[2], data[3], data[4], data[5]]))
}

fn message(command: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + body.len());
    out.extend_from_slice(&((HEADER + body.len()) as u16).to_be_bytes());
    out.extend_from_slice(&command.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
mod tests {
    use super::answer;

    /// The authentication 슈퍼프로야구2012 sent, as the log has it.
    const AUTHENTICATION: &str = "00a7000100000000000000e60a32303130343631313932363900000000000000000000000000000000000000000000000000000000000000000000000000000076312e302e3000000000000000000000000000006e312e302e30000000000000000000000000000030303934373434383532000000000000000000005343482d573833300000000000000000000000000a00000000000000000000000000000000000000000000";

    /// The shop purchase 슈퍼프로야구2012 sent, as the log has it.
    const PURCHASE: &str = "00ae000400000000000000e60a32303130343631313932363900000000000000000000000000000000000000000000000000000000000000000000000000000076312e302e3000000000000000000000000000006e312e302e30000000000000000000000000000030303934373434383532000000000000000000005343482d57383330000000000000000000000000000000000f3c000000000000000000000000000000000000000000000000";

    fn bytes(hex: &str) -> alloc::vec::Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_authentication_is_granted() {
        let reply = answer(&bytes(AUTHENTICATION)).unwrap();

        assert_eq!(reply.len(), 8 + 256 + 1);
        assert_eq!(reply[..8], [0x01, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        assert!(reply[8..264].iter().all(|&x| x == 0));
        assert_eq!(reply[264], 1);
    }

    #[test]
    fn the_purchase_is_granted() {
        let reply = answer(&bytes(PURCHASE)).unwrap();

        assert_eq!(reply.len(), 8 + 26);
        assert_eq!(reply[..8], [0x00, 0x22, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
        assert!(reply[8..].iter().all(|&x| x == 0));
    }

    #[test]
    fn anything_else_is_left_alone() {
        let mut request = bytes(AUTHENTICATION);
        request[3] = 0x02;
        assert!(answer(&request).is_none());
        request[3] = 0x01;
        request.pop();
        assert!(answer(&request).is_none());
        assert!(answer(&[0x00, 0x04]).is_none());
    }
}
