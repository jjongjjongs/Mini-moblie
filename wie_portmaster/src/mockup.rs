//! Draft screens for the button settings, rendered to raw RGBA for review.

use wie_backend::canvas::{Color, TextAlignment};

use crate::library::{BAR, BAR_COLOR, HIGHLIGHT, LINE, MUTED, Screen, TEXT, rgb};

const ROW: Color = rgb(0x22, 0x2a, 0x33);
const ACCENT: Color = rgb(0x7d, 0xe0, 0xa8);
const PANEL: Color = rgb(0x1a, 0x20, 0x28);
const EDGE: Color = rgb(0x3c, 0x48, 0x54);

fn save(name: &str, width: u32, height: u32, rgba: Vec<u8>) {
    let dir = std::env::var("MOCKUP_DIR").unwrap();
    std::fs::write(format!("{dir}/{name}_{width}x{height}.rgba"), rgba).unwrap();
}

fn row_text(s: &mut Screen, y: i32, left: &str, right: &str, picked: bool, width: u32) {
    if picked {
        s.fill(0, y, width, LINE as u32, HIGHLIGHT);
    }
    let color = if picked { TEXT } else { MUTED };
    s.text(left, 8, y + 1, TextAlignment::Left, color);
    if !right.is_empty() {
        s.text(right, width as i32 - 8, y + 1, TextAlignment::Right, if picked { TEXT } else { ACCENT });
    }
}

#[test]
#[ignore]
fn render() {
    // 1. The game list, with the new hint.
    {
        let (w, h) = (320, 240);
        let mut s = Screen::new(w, h);
        s.bar(0, "MiniMobile", "1/3");
        for (i, name) in ["레전드 오브 마스터", "소울게이트", "제노니아2"].iter().enumerate() {
            row_text(&mut s, BAR + 4 + i as i32 * LINE, name, "", i == 0, w);
        }
        s.bar(h as i32 - BAR, "A 실행  A+B 설정", "SELECT+START 종료");
        save("1_list", w, h, s.rgba());
    }

    // 2. The menu A+B opens over a paused game.
    {
        let (w, h) = (232, 166);
        let mut s = Screen::new(w, h);
        s.fill(0, 0, w, h, PANEL);
        s.bar(0, "메뉴", "일시정지");
        let rows = [
            ("게임으로 돌아가기", ""),
            ("버튼 배치 바꾸기", ""),
            ("프리셋", "◀ 기본 ▶"),
            ("이 게임에 프리셋 고정", "끔"),
            ("A+B로 메뉴 열기", "켬"),
            ("게임 끝내기", ""),
        ];
        for (i, (left, right)) in rows.iter().enumerate() {
            row_text(&mut s, BAR + 4 + i as i32 * LINE, left, right, i == 1, w);
        }
        s.bar(h as i32 - BAR, "A 선택", "B 닫기");
        save("2_menu", w, h, s.rgba());
    }

    // 3. The mapping table, and 4. the key picker over it.
    let table = |s: &mut Screen, w: u32, h: u32| {
        s.bar(0, "버튼 배치", "프리셋: 기본 (바뀜)");
        let (c1, c2, c3) = (8, 92, 206);
        s.text("버튼", c1, BAR + 2, TextAlignment::Left, MUTED);
        s.text("그냥", c2, BAR + 2, TextAlignment::Left, MUTED);
        s.text("SELECT+", c3, BAR + 2, TextAlignment::Left, MUTED);
        s.fill(0, BAR + LINE + 1, w, 1, EDGE);
        let rows = [
            ("D▲", "▲", "2"),
            ("D▼", "▼", "8"),
            ("D◀", "◀", "4"),
            ("D▶", "▶", "6"),
            ("A", "확인", "5"),
            ("B", "취소", "0"),
            ("X", "*", "1"),
            ("Y", "#", "3"),
            ("L1", "좌소프트", "7"),
        ];
        let top = BAR + LINE + 4;
        for (i, (button, plain, select)) in rows.iter().enumerate() {
            let y = top + i as i32 * LINE;
            let picked = i == 4;
            if picked {
                s.fill(0, y, w, LINE as u32, ROW);
                s.fill(c2 - 4, y, (c3 - c2 - 8) as u32, LINE as u32, HIGHLIGHT);
            }
            s.text(button, c1, y + 1, TextAlignment::Left, TEXT);
            s.text(plain, c2, y + 1, TextAlignment::Left, if picked { TEXT } else { ACCENT });
            s.text(select, c3, y + 1, TextAlignment::Left, ACCENT);
        }
        s.text("▼", w as i32 - 8, top + 8 * LINE + 1, TextAlignment::Right, MUTED);
        s.bar(h as i32 - BAR, "A 바꾸기  ◀▶ 칸", "B 뒤로");
    };
    {
        let (w, h) = (320, 240);
        let mut s = Screen::new(w, h);
        table(&mut s, w, h);
        save("3_table", w, h, s.rgba());
    }
    {
        let (w, h) = (320, 240);
        let mut s = Screen::new(w, h);
        table(&mut s, w, h);
        // Dim what is behind, then the picker.
        s.fill(0, 0, w, h, Color { a: 0xb0, r: 0, g: 0, b: 0 });
        let (px, py, pw, ph) = (24, 22, 272u32, 196u32);
        s.fill(px - 1, py - 1, pw + 2, ph + 2, EDGE);
        s.fill(px, py, pw, ph, PANEL);
        s.fill(px, py, pw, BAR as u32, BAR_COLOR);
        s.text("A 버튼 (그냥)", px + 6, py + 2, TextAlignment::Left, TEXT);
        s.text("지금: 확인", px + pw as i32 - 6, py + 2, TextAlignment::Right, TEXT);

        // A handset's number pad on the left...
        let pad = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"];
        let (cw, ch) = (34, 24);
        let (gx, gy) = (px + 10, py + BAR + 8);
        for (i, label) in pad.iter().enumerate() {
            let (x, y) = (gx + (i % 3) as i32 * (cw + 4), gy + (i / 3) as i32 * (ch + 4));
            s.fill(x, y, cw as u32, ch as u32, ROW);
            s.text(label, x + cw / 2, y + 4, TextAlignment::Center, TEXT);
        }
        // ...and the other keys on the right.
        let keys = ["▲", "▼", "◀", "▶", "확인", "취소", "좌소프트", "우소프트", "통화", "종료"];
        let (kw, kh) = (64, 20);
        let (kx, ky) = (px + 130, py + BAR + 8);
        for (i, label) in keys.iter().enumerate() {
            let (x, y) = (kx + (i % 2) as i32 * (kw + 4), ky + (i / 2) as i32 * (kh + 3));
            let picked = *label == "확인";
            s.fill(x, y, kw as u32, kh as u32, if picked { HIGHLIGHT } else { ROW });
            s.text(label, x + kw / 2, y + 2, TextAlignment::Center, TEXT);
        }
        let (nx, ny) = (kx, ky + 5 * (kh + 3));
        s.fill(nx, ny, (2 * kw + 4) as u32, kh as u32, ROW);
        s.text("없음", nx + kw + 2, ny + 2, TextAlignment::Center, MUTED);

        s.fill(px, py + ph as i32 - BAR, pw, BAR as u32, BAR_COLOR);
        s.text("A 고르기", px + 6, py + ph as i32 - BAR + 2, TextAlignment::Left, TEXT);
        s.text("B 취소", px + pw as i32 - 6, py + ph as i32 - BAR + 2, TextAlignment::Right, TEXT);
        save("4_picker", w, h, s.rgba());
    }

    // 5. Presets.
    {
        let (w, h) = (320, 240);
        let mut s = Screen::new(w, h);
        s.bar(0, "프리셋", "2/4");
        let rows = [
            ("기본", "사용 중"),
            ("레전드 오브 마스터", "이 게임에 고정"),
            ("액션 게임", ""),
            ("숫자키 많이 쓰는 게임", ""),
            ("+ 지금 배치를 새 프리셋으로", ""),
        ];
        for (i, (left, right)) in rows.iter().enumerate() {
            row_text(&mut s, BAR + 4 + i as i32 * LINE, left, right, i == 1, w);
        }
        s.paragraph("새로 저장하면 지금 하던 게임\n이름으로 저장됩니다.", BAR + 4 + 6 * LINE + 4, MUTED);
        s.bar(h as i32 - BAR, "A 불러오기 X 덮어쓰기 Y 지우기", "B 뒤로");
        save("5_presets", w, h, s.rgba());
    }
}
