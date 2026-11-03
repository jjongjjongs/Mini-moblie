//! The billing server 원조낚시광3 (SKT `0087312415`) converts its trial
//! through.
//!
//! The title asks Nexon's GXG gateway ([`super::gxg`]) to carry it to
//! `222.122.160.61`, port `11001`, and sends its requests as the gateway's
//! data. `0x1193ca` writes each one's head as `sprintf("%s%s%c%c", "SKT",
//! MIN, 0x1b, kind)`, and the kind's own fields follow. 정식판 전환 is kind
//! `0x0d`: two bytes, the price (4000) as a little-endian word, the product's
//! id, `70060` and the product's name.
//!
//! The answer is read by `0x11926e`: a little-endian word it keeps and never
//! looks at again, the kind, then the kind's fields from `+5`
//! (`0x118d96`). For `0x0d` (`0x119160`) that is one byte: 1, with the title
//! waiting on a conversion, clears its trial flag; anything else is
//! `0x118d36`'s failure. The server is long gone, so the conversion sat
//! waiting.
//!
//! A shop purchase is kind `3` (sent from `0x1080a0` and `0x10f570`): a flag
//! byte, the item, the price as a little-endian word, the product's id and
//! name. Its answer (`0x118f3a`) is one result byte the shop reads through
//! `0x11a04e`. 1 is granted: the shop charges the price through the gateway
//! (`0x119b24`), asks again and, granted once more, hands the item over
//! (`0x10744c`). 0 and 2 to 5 are its failure popups `0x47` to `0x4b`.

use alloc::{vec, vec::Vec};

/// Where the title asks the gateway to carry it.
pub const TARGET: (&str, u16) = ("222.122.160.61", 11001);

const CARRIER: &[u8; 3] = b"SKT";
const HEAD_MARK: u8 = 0x1b;

const CONVERSION: u8 = 0x0d;
const CONVERTED: u8 = 1;

const PURCHASE: u8 = 0x03;
const PURCHASED: u8 = 1;

/// The answer to the title's request, for the requests it is known for.
pub fn answer(data: &[u8]) -> Option<Vec<u8>> {
    let Some(kind) = kind(data) else {
        tracing::info!("fisher: no request in {} bytes {:02x?}", data.len(), data);
        return None;
    };

    match kind {
        CONVERSION => {
            tracing::info!("fisher: converted");
            Some(reply(CONVERSION, &[CONVERTED]))
        }
        PURCHASE => {
            tracing::info!("fisher: purchase granted");
            Some(reply(PURCHASE, &[PURCHASED]))
        }
        kind => {
            tracing::info!("fisher: no answer known for kind {kind:#x}, {} bytes {:02x?}", data.len(), data);
            None
        }
    }
}

/// The kind a request's head names: past `SKT`, the `MIN`'s digits and the
/// mark.
fn kind(data: &[u8]) -> Option<u8> {
    let rest = data.strip_prefix(CARRIER)?;
    let digits = rest.iter().take_while(|x| x.is_ascii_digit()).count();
    if digits == 0 || *rest.get(digits)? != HEAD_MARK {
        return None;
    }
    rest.get(digits + 1).copied()
}

/// An answer: the word the title keeps - its length here - the kind and its
/// fields.
fn reply(kind: u8, fields: &[u8]) -> Vec<u8> {
    let length = (4 + 1 + fields.len()) as u32;
    let mut out = vec![];
    out.extend_from_slice(&length.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(fields);
    out
}

#[cfg(test)]
mod tests {
    use super::answer;

    /// The conversion 원조낚시광3 sent, as the log has it.
    const CONVERSION: &str = "534b54313034363131393236391b0d0101a00f000039303030353234393338003730303630bff8c1b6b3acbdc3b1a43328b9abc3bc295fc0cfb9dd5f46726565496e7374616c6c000000000000";

    /// The shop purchase 원조낚시광3 sent, as the log has it.
    const PURCHASE: &str = "534b54313034363131393236391b03010200000001d00700003930303035323439353200c7c1b7cec6e4bcc5b3ce20c6d0c5b0c1f600000000000000000000000000000000000000000000003730303630bff8c1b6b3acbdc3b1a43328b9abc3bc295fc0cfb9dd5f46726565496e7374616c6c000000000000";

    fn bytes(hex: &str) -> alloc::vec::Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_conversion_is_answered_converted() {
        let reply = answer(&bytes(CONVERSION)).unwrap();

        assert_eq!(reply, [6, 0, 0, 0, 0x0d, 1]);
    }

    #[test]
    fn the_purchase_is_granted() {
        let reply = answer(&bytes(PURCHASE)).unwrap();

        assert_eq!(reply, [6, 0, 0, 0, 0x03, 1]);
    }

    #[test]
    fn anything_else_is_left_alone() {
        let mut request = bytes(CONVERSION);
        request[14] = 0x05;
        assert!(answer(&request).is_none());
        assert!(answer(b"KTF0104611926\x1b\x0d").is_none());
        assert!(answer(b"SKT").is_none());
    }
}
