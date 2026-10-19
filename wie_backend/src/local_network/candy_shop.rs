//! 메이플스토리 Live's 캔디샵, which its SKT build reaches through Nexon's GXG
//! gateway ([`super::GxgEndpoint`]).
//!
//! The shop's shelves were the server's: it sent the goods, their names and
//! their prices, and the title only draws them. What the server sold, though,
//! the title still carries - its own item tables (`/data/ietc/etc0.dat` and
//! `/data/iequip/equip0.dat`) keep every cash item by number, and the purchase
//! code knows several of them by number too (`0x368a6`: 7 widens the 창고, 14
//! adds a save slot, 13 resets the nickname, 11 the match record). So the
//! shelves here are those items, and the shop draws each with the name, icon
//! and description its tables give it.
//!
//! Every frame is Nexon's `[u32 length][0xffff][u16 command][body]`, little
//! end first, and every answer opens with a result byte the title goes on from
//! at zero. The dispatcher is `0x36cfe`; the shop's commands are handled by
//! `0x3639e`.
//!
//! ```text
//! 0x0001  the session's answer   result, ten bytes, u32 candy  (0x36b58)
//! 0x0028  the shop's tabs     -> 0x29  result, six (u8, u32, u32)  (0x363ce)
//! 0x002c  one tab's goods     -> 0x2d  result, count, the goods   (0x36498)
//! 0x002e  buy (tab, slot)     -> 0x2f  result, count, what it gives (0x366f2)
//! 0x0030  confirm (tab, slot) -> 0x31  result, u32 candy left       (0x368a6)
//! ```

use alloc::vec::Vec;

/// The candy the subscriber holds. A purchase never spends it: the candy was
/// bought with money the emulator does not take, the same way every other
/// shop here grants what it is asked for.
const CANDY: u32 = 1_000_000;

const HEADER: usize = 8;
const MARKER: [u8; 2] = [0xff, 0xff];

const SESSION_REQUEST: u16 = 0x0000;
const TABS_REQUEST: u16 = 0x0028;
const GOODS_REQUEST: u16 = 0x002c;
const BUY_REQUEST: u16 = 0x002e;
const CONFIRM_REQUEST: u16 = 0x0030;

/// The result every answer opens with.
const GRANTED: u8 = 0;

/// How many tabs `0x363ce` reads.
const TABS: u8 = 6;

/// The session's body as Live sends it: the subscriber's number and fourteen
/// bytes of its own behind it.
const LIVE_SESSION_BODY: usize = 11 + 14;

/// An item's kind as the purchase code takes it (`+0x14`): zero makes an
/// equipment item (`0x1fad8`), anything else an etc item with a count
/// (`0x1f9be`).
const EQUIP: u8 = 0;
const ETC: u8 = 1;

/// One good on a shelf.
struct Good {
    kind: u8,
    id: u16,
}

const fn etc(id: u16) -> Good {
    Good { kind: ETC, id }
}

const fn equip(id: u16) -> Good {
    Good { kind: EQUIP, id }
}

/// The shop's four shelves, under the tabs the title draws for them -
/// 아이템, 커뮤니티, 미니게임 and 기타 - and stocked with the items its tables
/// describe as bought rather than found: the etc items from 퀵배송 이용권 to
/// 모래시계 that nothing drops, and the equipment marked `0x0d`, the
/// 대전게임 and 물약자동먹기 accessories and 메소드랍 x2 to MP스틸.
const ITEMS: [Good; 19] = [
    etc(8),     // 초기화주문서
    etc(9),     // 되돌리기주문서
    etc(10),    // 카르마의가위
    etc(12),    // 리버스
    etc(15),    // AP초기화
    etc(16),    // SP초기화
    etc(21),    // 몬스터소화제
    etc(23),    // 미라클큐브
    etc(25),    // 모래시계
    equip(805), // 메소드랍 x2
    equip(806), // 기타아이템 x2
    equip(807), // 장비드랍확률 증가
    equip(808), // 경험치추가획득(30%)
    equip(809), // 공격강화
    equip(810), // HP스틸
    equip(811), // MP스틸
    equip(3),   // HP물약자동먹기
    equip(4),   // MP물약자동먹기
    equip(5),   // 물약자동먹기셋트
];
const COMMUNITY: [Good; 5] = [
    etc(5),  // 퀵배송 이용권
    etc(6),  // 광고판이용권
    etc(24), // 광고판+아이템첨부
    etc(13), // 닉네임초기화
    etc(22), // 이름색변경아이템
];
const MINIGAME: [Good; 4] = [
    equip(0), // 미니게임 슬롯 +1
    equip(1), // 아이템효과증가
    equip(2), // 아이템차단20%
    etc(11),  // 전적초기화
];
const OTHER: [Good; 2] = [
    etc(7),  // 창고인벤늘리기
    etc(14), // 세이브슬롯늘리기
];

/// The goods on one tab, which the title numbers from one.
fn shelf(tab: u8) -> &'static [Good] {
    match tab {
        1 => &ITEMS,
        2 => &COMMUNITY,
        3 => &MINIGAME,
        4 => &OTHER,
        _ => &[],
    }
}

fn frame(command: u16, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER + body.len());
    frame.extend_from_slice(&((HEADER + body.len()) as u32).to_le_bytes());
    frame.extend_from_slice(&MARKER);
    frame.extend_from_slice(&command.to_le_bytes());
    frame.extend_from_slice(body);
    frame
}

/// One good as `0x36498` reads it off a shelf.
///
/// The name and description are the server's text, but the shop draws the
/// ones its own tables give the item and leaves these unread, so they go out
/// empty. The price is nothing, since nothing is charged. A period of zero
/// days is drawn as 무기한.
fn shelf_record(answer: &mut Vec<u8>, slot: u8, good: &Good) {
    answer.push(slot);
    answer.push(0); // the name's length
    answer.push(0); // the description's
    answer.extend_from_slice(&0u32.to_le_bytes()); // price
    grant_record(answer, good);
    answer.push(0);
}

/// What a purchase gives, as `0x366f2` reads it and `0x368a6` hands it over:
/// the kind, the item, how many and for how many days.
fn grant_record(answer: &mut Vec<u8>, good: &Good) {
    answer.push(good.kind);
    answer.extend_from_slice(&good.id.to_le_bytes());
    answer.extend_from_slice(&1u32.to_le_bytes());
    answer.extend_from_slice(&0u16.to_le_bytes());
}

/// What Live's shop is answered, or `None` for a frame that is not one of its.
pub fn answer(data: &[u8]) -> Option<Vec<u8>> {
    let length = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
    if data.len() < HEADER || length != data.len() || data[4..6] != MARKER {
        return None;
    }

    let command = u16::from_le_bytes([data[6], data[7]]);
    let body = &data[HEADER..];

    let mut answer = alloc::vec![GRANTED];
    match command {
        // The session answer the rest of this family takes is the result, a
        // message length and four bytes; Live's (`0x36b58`) takes ten bytes
        // past the result and then its candy, which is where the shop's
        // balance comes from.
        SESSION_REQUEST if body.len() == LIVE_SESSION_BODY && body[..11].iter().all(u8::is_ascii_digit) => {
            answer.extend_from_slice(&[0; 10]);
            answer.extend_from_slice(&CANDY.to_le_bytes());
        }
        // Six tabs as `0x363ce` reads them: a number and two `u32`s. Only the
        // first four are asked for their goods.
        TABS_REQUEST if body.is_empty() => {
            for tab in 1..=TABS {
                answer.push(tab);
                answer.extend_from_slice(&(shelf(tab).len() as u32).to_le_bytes());
                answer.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        // The tab, and that tab's goods behind their count.
        GOODS_REQUEST if body.len() == 1 => {
            let goods = shelf(body[0]);
            answer.push(goods.len() as u8);
            for (slot, good) in goods.iter().enumerate() {
                shelf_record(&mut answer, slot as u8, good);
            }
        }
        // The tab and the slot of what was chosen, and what it gives.
        BUY_REQUEST if body.len() == 2 => {
            let good = shelf(body[0]).get(body[1] as usize)?;
            answer.push(1);
            grant_record(&mut answer, good);
        }
        // The same two bytes again, and the candy left.
        CONFIRM_REQUEST if body.len() == 2 => answer.extend_from_slice(&CANDY.to_le_bytes()),
        _ => return None,
    }

    Some(frame(command + 1, &answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(command: u16, body: &[u8]) -> Vec<u8> {
        frame(command, body)
    }

    #[test]
    fn the_session_carries_the_candy() {
        let mut body = b"01089483847".to_vec();
        body.extend_from_slice(&[1, 1, 1, 1, 0, 1, 0, 3, 0, 3, 0, 3, 0, 3]);

        let answer = answer(&request(SESSION_REQUEST, &body)).unwrap();
        assert_eq!(&answer[6..8], &1u16.to_le_bytes());
        assert_eq!(answer[HEADER], GRANTED);
        assert_eq!(&answer[HEADER + 11..], &CANDY.to_le_bytes());
    }

    #[test]
    fn a_tab_lists_its_goods_and_a_slot_buys_one() {
        let shelf = answer(&request(GOODS_REQUEST, &[3])).unwrap();
        assert_eq!(&shelf[HEADER..HEADER + 2], &[GRANTED, MINIGAME.len() as u8]);
        // slot 0, two empty strings, no price, then 미니게임 슬롯 +1.
        assert_eq!(&shelf[HEADER + 2..HEADER + 18], &[0, 0, 0, 0, 0, 0, 0, EQUIP, 0, 0, 1, 0, 0, 0, 0, 0]);

        let bought = answer(&request(BUY_REQUEST, &[4, 1])).unwrap();
        assert_eq!(&bought[HEADER..], &[GRANTED, 1, ETC, 14, 0, 1, 0, 0, 0, 0, 0]);

        assert_eq!(answer(&request(BUY_REQUEST, &[4, 2])), None);
    }

    #[test]
    fn other_frames_are_left_alone() {
        assert_eq!(answer(&request(0x0004, &[0; 5])), None);
        assert_eq!(answer(&[0, 9, 0, 0x34, 0, 0xff, 0xff, 0xff, 0xff]), None);
    }
}
