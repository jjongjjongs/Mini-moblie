//! The 캔디샵 of 메이플스토리 Live and 메이플스토리 시그너스기사단, which their SKT
//! builds reach through Nexon's GXG gateway ([`super::GxgEndpoint`]), and
//! 시그너스's LGT build through its billing socket
//! ([`crate::billing::lgt_local_marked_command_response`]), built by the same
//! code (`0xc208` to `0xc29a` there).
//!
//! The shop's shelves were the server's: it sent the goods, their names and
//! their prices, and the title only draws them. What the server sold, though,
//! each title still carries - its own item tables keep every cash item by
//! number, and the purchase code knows several of them by number too (Live's
//! `0x368a6`: 7 widens the 창고, 14 adds a save slot, 13 resets the nickname,
//! 11 the match record; 시그너스's `0x132b6` the same at 0xdb, 0xee, 0x106 and
//! 0xdf). So the shelves here are those items, and the shop draws each with the
//! name, icon and description its tables give it.
//!
//! Every frame is Nexon's `[u32 length][0xffff][u16 command][body]`, little
//! end first, and every answer opens with a result byte the title goes on from
//! at zero. The two titles share the shop's code, but 시그너스 numbers its
//! commands two higher (see [`Title::commands`]):
//!
//! ```text
//! Live    시그너스
//! 0x0028  0x002a  the shop's tabs     -> result, six (u8, u32, u32)
//! 0x002c  0x002e  one tab's goods     -> result, count, the goods
//! 0x002e  0x0030  buy (tab, slot)     -> result, count, what it gives
//! 0x0030  0x0032  confirm (tab, slot) -> result, u32 candy left
//! ```
//!
//! Live's readers are `0x363ce`, `0x36498`, `0x366f2` and `0x368a6`;
//! 시그너스's `0x13b4e`, `0x13a58`, `0x13114` and `0x132b6`. Which title is
//! asking is told by the name its authentication (`0x2711`) carries, since the
//! same command can mean a purchase to one and its confirmation to the other.

use alloc::vec::Vec;

/// The candy the subscriber holds. A purchase never spends it: the candy was
/// bought with money the emulator does not take, the same way every other
/// shop here grants what it is asked for.
pub(crate) const CANDY: u32 = 1_000_000;

const HEADER: usize = 8;
const MARKER: [u8; 2] = [0xff, 0xff];

const SESSION_REQUEST: u16 = 0x0000;
const AUTH_REQUEST: u16 = 0x2711;

/// The result every answer opens with.
const GRANTED: u8 = 0;

/// How many tabs the tab reader reads.
const TABS: u8 = 6;

/// The session's body as both titles send it: the subscriber's number and
/// fourteen bytes of their own behind it.
const SESSION_BODY: usize = 11 + 14;

/// An item's kind as the purchase code takes it (`+0x14`): zero makes an
/// equipment item, anything else an etc item with a count.
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

/// The title on the other end, as its authentication names it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Title {
    Live,
    Cygnus,
}

/// The four commands a title's shop sends.
struct Commands {
    tabs: u16,
    goods: u16,
    buy: u16,
    confirm: u16,
}

impl Title {
    fn named(name: &[u8]) -> Option<Self> {
        match name {
            b"MapleLive" => Some(Self::Live),
            b"MapleCygnus" => Some(Self::Cygnus),
            _ => None,
        }
    }

    fn commands(self) -> Commands {
        match self {
            Self::Live => Commands {
                tabs: 0x28,
                goods: 0x2c,
                buy: 0x2e,
                confirm: 0x30,
            },
            Self::Cygnus => Commands {
                tabs: 0x2a,
                goods: 0x2e,
                buy: 0x30,
                confirm: 0x32,
            },
        }
    }

    /// The goods on one tab, which the title numbers from one. The four tabs
    /// it asks for are drawn as 아이템, 커뮤니티, 미니게임 and 기타.
    fn shelf(self, tab: u8) -> &'static [Good] {
        let shelves: &[&[Good]; 4] = match self {
            Self::Live => &LIVE,
            Self::Cygnus => &CYGNUS,
        };
        tab.checked_sub(1).and_then(|tab| shelves.get(tab as usize)).copied().unwrap_or(&[])
    }
}

/// Live's shelves. Its etc table (`/data/ietc/etc0.dat`) keeps the cash items
/// from 퀵배송 이용권 to 모래시계 that nothing drops, and its equipment table
/// (`/data/iequip/equip0.dat`) the accessories it marks `0x0d`: the 대전게임
/// and 물약자동먹기 ones and 메소드랍 x2 to MP스틸.
const LIVE: [&[Good]; 4] = [
    &[
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
    ],
    &[
        etc(5),  // 퀵배송 이용권
        etc(6),  // 광고판이용권
        etc(24), // 광고판+아이템첨부
        etc(13), // 닉네임초기화
        etc(22), // 이름색변경아이템
    ],
    &[
        equip(0), // 미니게임 슬롯 +1
        equip(1), // 아이템효과증가
        equip(2), // 아이템차단20%
        etc(11),  // 전적초기화
    ],
    &[
        etc(7),  // 창고인벤늘리기
        etc(14), // 세이브슬롯늘리기
    ],
];

/// 시그너스's shelves. Its one item table (`/data/item_etc.dat`, 191 bytes an
/// item) keeps every cash item together, the accessories included: 217 to 238
/// and 257, 258 and 262.
const CYGNUS: [&[Good]; 4] = [
    &[
        etc(220), // 초기화주문서
        etc(221), // 되돌리기주문서
        etc(222), // 카르마의가위
        etc(234), // 리버스
        etc(257), // AP초기화
        etc(258), // SP초기화
        etc(230), // 메소드랍 x2
        etc(231), // 기타아이템 x2
        etc(232), // 장비드랍확률 증가
        etc(233), // 경험치추가획득(30%)
        etc(235), // 공격강화
        etc(236), // HP스틸
        etc(237), // MP스틸
        etc(227), // HP물약자동먹기
        etc(228), // MP물약자동먹기
        etc(229), // 물약자동먹기셋트
    ],
    &[
        etc(217), // 퀵배송 이용권
        etc(218), // 광고판이용권
        etc(262), // 닉네임초기화
    ],
    &[
        etc(224), // 미니게임 슬롯 +1
        etc(225), // 아이템효과증가
        etc(226), // 아이템차단20%
        etc(223), // 전적초기화
    ],
    &[
        etc(219), // 창고인벤늘리기
        etc(238), // 세이브슬롯늘리기
    ],
];

fn frame(command: u16, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER + body.len());
    frame.extend_from_slice(&((HEADER + body.len()) as u32).to_le_bytes());
    frame.extend_from_slice(&MARKER);
    frame.extend_from_slice(&command.to_le_bytes());
    frame.extend_from_slice(body);
    frame
}

/// One good as the goods reader takes it off a shelf.
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

/// What a purchase gives, as the purchase reader takes it and the
/// confirmation hands it over: the kind, the item, how many and for how many
/// days.
fn grant_record(answer: &mut Vec<u8>, good: &Good) {
    answer.push(good.kind);
    answer.extend_from_slice(&good.id.to_le_bytes());
    answer.extend_from_slice(&1u32.to_le_bytes());
    answer.extend_from_slice(&0u16.to_le_bytes());
}

/// The shop behind the gateway, which learns which title it serves from the
/// title's authentication.
#[derive(Default)]
pub struct CandyShop {
    title: Option<Title>,
    /// The frames seen and the 피그미 hatched so far, which pick what the
    /// next 피그미 hatches into: the emulator has no clock to draw by, but how
    /// a session has gone before its hatching differs from one to the next.
    frames: u32,
    hatched: u32,
}

impl CandyShop {
    /// What the shop answers `data`, or `None` for a frame that is not one of
    /// its own.
    pub fn answer(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        let length = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
        if data.len() < HEADER || length != data.len() || data[4..6] != MARKER {
            return None;
        }

        let command = u16::from_le_bytes([data[6], data[7]]);
        let body = &data[HEADER..];
        self.frames = self.frames.wrapping_add(1);

        // The title's name, the first of the two strings its authentication
        // carries. The authentication itself is answered elsewhere.
        if command == AUTH_REQUEST {
            let length = u16::from_le_bytes(body.get(..2)?.try_into().ok()?) as usize;
            if let Some(title) = body.get(2..2 + length).and_then(Title::named) {
                self.title = Some(title);
            }
            return None;
        }

        let mut answer = alloc::vec![GRANTED];

        // The session answer the rest of this family takes is the result, a
        // message length and four bytes; these two titles' (Live's `0x36b58`)
        // take ten bytes past the result and then the candy, which is where
        // the shop's balance comes from.
        if command == SESSION_REQUEST {
            if body.len() != SESSION_BODY || !body[..11].iter().all(u8::is_ascii_digit) {
                return None;
            }
            answer.extend_from_slice(&[0; 10]);
            answer.extend_from_slice(&CANDY.to_le_bytes());
            return Some(frame(command + 1, &answer));
        }

        if self.title == Some(Title::Live) && command == super::pigmy::REQUEST {
            let draw = self.frames.wrapping_mul(0x9e37_79b9).wrapping_add(self.hatched);
            let answer = super::pigmy::answer(body, draw)?;
            self.hatched = self.hatched.wrapping_add(1);
            return Some(frame(command + 1, &answer));
        }

        self.title?.answer(data)
    }
}

impl Title {
    /// What this title's shop is answered for one of its four commands, or
    /// `None` for anything else.
    pub(crate) fn answer(self, data: &[u8]) -> Option<Vec<u8>> {
        let length = u32::from_le_bytes(data.get(..4)?.try_into().ok()?) as usize;
        if data.len() < HEADER || length != data.len() || data[4..6] != MARKER {
            return None;
        }

        let command = u16::from_le_bytes([data[6], data[7]]);
        let body = &data[HEADER..];
        let commands = self.commands();
        let mut answer = alloc::vec![GRANTED];

        if command == commands.tabs && body.is_empty() {
            for tab in 1..=TABS {
                answer.push(tab);
                answer.extend_from_slice(&(self.shelf(tab).len() as u32).to_le_bytes());
                answer.extend_from_slice(&0u32.to_le_bytes());
            }
        } else if command == commands.goods && body.len() == 1 {
            // The tab, and that tab's goods behind their count.
            let goods = self.shelf(body[0]);
            answer.push(goods.len() as u8);
            for (slot, good) in goods.iter().enumerate() {
                shelf_record(&mut answer, slot as u8, good);
            }
        } else if command == commands.buy && body.len() == 2 {
            // The tab and the slot of what was chosen, and what it gives.
            let good = self.shelf(body[0]).get(body[1] as usize)?;
            answer.push(1);
            grant_record(&mut answer, good);
        } else if command == commands.confirm && body.len() == 2 {
            // The same two bytes again, and the candy left.
            answer.extend_from_slice(&CANDY.to_le_bytes());
        } else {
            return None;
        }

        Some(frame(command + 1, &answer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(command: u16, body: &[u8]) -> Vec<u8> {
        frame(command, body)
    }

    fn shop(name: &[u8]) -> CandyShop {
        let mut auth = (name.len() as u16).to_le_bytes().to_vec();
        auth.extend_from_slice(name);
        auth.extend_from_slice(&auth.clone());

        let mut shop = CandyShop::default();
        assert_eq!(shop.answer(&request(AUTH_REQUEST, &auth)), None);
        shop
    }

    #[test]
    fn the_session_carries_the_candy() {
        let mut body = b"01089483847".to_vec();
        body.extend_from_slice(&[1, 1, 1, 1, 0, 1, 0, 3, 0, 3, 0, 3, 0, 3]);

        let answer = CandyShop::default().answer(&request(SESSION_REQUEST, &body)).unwrap();
        assert_eq!(&answer[6..8], &1u16.to_le_bytes());
        assert_eq!(answer[HEADER], GRANTED);
        assert_eq!(&answer[HEADER + 11..], &CANDY.to_le_bytes());
    }

    #[test]
    fn live_lists_a_tab_and_sells_from_it() {
        let mut shop = shop(b"MapleLive");

        let shelf = shop.answer(&request(0x2c, &[3])).unwrap();
        assert_eq!(&shelf[6..8], &0x2du16.to_le_bytes());
        assert_eq!(&shelf[HEADER..HEADER + 2], &[GRANTED, 4]);
        // slot 0, two empty strings, no price, then 미니게임 슬롯 +1.
        assert_eq!(&shelf[HEADER + 2..HEADER + 18], &[0, 0, 0, 0, 0, 0, 0, EQUIP, 0, 0, 1, 0, 0, 0, 0, 0]);

        let bought = shop.answer(&request(0x2e, &[4, 1])).unwrap();
        assert_eq!(&bought[HEADER..], &[GRANTED, 1, ETC, 14, 0, 1, 0, 0, 0, 0, 0]);
        assert_eq!(shop.answer(&request(0x2e, &[4, 2])), None);

        let confirmed = shop.answer(&request(0x30, &[4, 1])).unwrap();
        assert_eq!(&confirmed[6..8], &0x31u16.to_le_bytes());
    }

    #[test]
    fn cygnus_numbers_its_commands_two_higher() {
        let mut shop = shop(b"MapleCygnus");

        let tabs = shop.answer(&request(0x2a, &[])).unwrap();
        assert_eq!(&tabs[6..8], &0x2bu16.to_le_bytes());

        let shelf = shop.answer(&request(0x2e, &[4])).unwrap();
        assert_eq!(&shelf[HEADER..HEADER + 2], &[GRANTED, 2]);

        // 세이브슬롯늘리기, 238.
        let bought = shop.answer(&request(0x30, &[4, 1])).unwrap();
        assert_eq!(&bought[6..8], &0x31u16.to_le_bytes());
        assert_eq!(&bought[HEADER..], &[GRANTED, 1, ETC, 238, 0, 1, 0, 0, 0, 0, 0]);

        let confirmed = shop.answer(&request(0x32, &[4, 1])).unwrap();
        assert_eq!(&confirmed[HEADER + 1..], &CANDY.to_le_bytes());
    }

    #[test]
    fn live_s_pigmy_hatches_into_equipment() {
        let mut shop = shop(b"MapleLive");

        let hatched = shop.answer(&request(0x58, &[2, 0])).unwrap();
        assert_eq!(&hatched[6..8], &0x59u16.to_le_bytes());
        assert_eq!(hatched[HEADER], GRANTED);
        assert_eq!(&hatched[HEADER + 5..HEADER + 13], &[0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(hatched.len(), HEADER + 60);

        // 시그너스 numbers its commands otherwise.
        assert_eq!(self::shop(b"MapleCygnus").answer(&request(0x58, &[2, 0])), None);
    }

    #[test]
    fn other_frames_are_left_alone() {
        let mut shop = CandyShop::default();
        assert_eq!(shop.answer(&request(0x2c, &[1])), None);
        assert_eq!(shop.answer(&request(0x0004, &[0; 5])), None);
        assert_eq!(shop.answer(&[0, 9, 0, 0x34, 0, 0xff, 0xff, 0xff, 0xff]), None);
    }
}
