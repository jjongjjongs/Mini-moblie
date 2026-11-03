//! The 세중 authentication 1등은니꺼 (SKT `0086426673`) runs before its
//! 정식판 전환.
//!
//! The title asks Nexon's GXG gateway ([`super::gxg`]) to carry it to
//! `222.122.158.175`, port `10497`, and sends one 48-byte request as the
//! gateway's data:
//!
//! ```text
//!   +0x00  u32be  48, the request's length
//!   +0x04  u8     1 - a request
//!   +0x05  u8     1
//!   +0x06  ...    the handset's number, spaces, "NH", the title's own fields
//! ```
//!
//! The gateway's data callback (`GXG_CBTYPE_RECVDATA`, `0x144e4a`) waits for
//! the answer's byte at `+4` to be set and goes on by it: 2 logs `세중 인증
//! 성공`, marks the title authenticated and moves to the charge (which the
//! gateway's own `CP` answers); 3 logs `세중 인증 실패` and gives up. The
//! server is long gone, so nothing came back and the conversion never began.
//!
//! The answer is the request with `+4` set to 2.

use alloc::vec::Vec;

/// Where the title asks the gateway to carry it.
pub const TARGET: (&str, u16) = ("222.122.158.175", 10497);

const REQUEST_SIZE: usize = 48;
const KIND_AT: usize = 4;
const REQUEST: u8 = 1;
const AUTHENTICATED: u8 = 2;

/// The answer to the title's authentication: authenticated.
pub fn answer(data: &[u8]) -> Option<Vec<u8>> {
    let whole =
        data.len() == REQUEST_SIZE && u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize == REQUEST_SIZE && data[KIND_AT] == REQUEST;
    if !whole {
        tracing::info!("sejung: no reply shaped for {} bytes {:02x?}", data.len(), data);
        return None;
    }

    tracing::info!("sejung: authenticated");
    let mut answer = data.to_vec();
    answer[KIND_AT] = AUTHENTICATED;
    Some(answer)
}

#[cfg(test)]
mod tests {
    use super::answer;

    /// The request 1등은니꺼 sent, as the log has it.
    const REQUEST: &str = "0000003001013130343631313932363920202020202000004e4874657374202020203031000000000000000000000000";

    fn bytes(hex: &str) -> alloc::vec::Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_authentication_is_answered_authenticated() {
        let request = bytes(REQUEST);
        let reply = answer(&request).unwrap();

        assert_eq!(reply.len(), 48);
        assert_eq!(reply[4], 2);
        assert_eq!(reply[..4], request[..4]);
        assert_eq!(reply[5..], request[5..]);
    }

    #[test]
    fn anything_else_is_left_alone() {
        assert!(answer(&[0; 12]).is_none());
        let mut request = bytes(REQUEST);
        request[4] = 2;
        assert!(answer(&request).is_none());
    }
}
