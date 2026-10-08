//! Draft screens for the Windows build, rendered to raw RGBA for review.

use wie_backend::canvas::{Color, TextAlignment};

use crate::library::{BAR, BAR_COLOR, HIGHLIGHT, LINE, MUTED, Screen, TEXT, rgb};

const ROW: Color = rgb(0x22, 0x2a, 0x33);
const ACCENT: Color = rgb(0x7d, 0xe0, 0xa8);
const PANEL: Color = rgb(0x1a, 0x20, 0x28);
const EDGE: Color = rgb(0x3c, 0x48, 0x54);
const DIM: Color = Color { a: 0xb0, r: 0, g: 0, b: 0 };
const W: u32 = 320;
const H: u32 = 240;

fn save(name: &str, screen: Screen) {
    let dir = std::env::var("MOCKUP_DIR").unwrap();
    std::fs::write(format!("{dir}/{name}.rgba"), screen.rgba()).unwrap();
}

fn row(s: &mut Screen, y: i32, left: &str, right: &str, picked: bool, width: u32) {
    if picked {
        s.fill(0, y, width, LINE as u32, HIGHLIGHT);
    }
    s.text(left, 8, y + 1, TextAlignment::Left, if picked { TEXT } else { MUTED });
    if !right.is_empty() {
        s.text(right, width as i32 - 8, y + 1, TextAlignment::Right, if picked { TEXT } else { ACCENT });
    }
}

fn panel(s: &mut Screen, x: i32, y: i32, w: u32, h: u32, title: &str, note: &str, hint: &str, back: &str) {
    s.fill(x - 1, y - 1, w + 2, h + 2, EDGE);
    s.fill(x, y, w, h, PANEL);
    s.fill(x, y, w, BAR as u32, BAR_COLOR);
    s.text(title, x + 6, y + 2, TextAlignment::Left, TEXT);
    s.text(note, x + w as i32 - 6, y + 2, TextAlignment::Right, TEXT);
    let by = y + h as i32 - BAR;
    s.fill(x, by, w, BAR as u32, BAR_COLOR);
    s.text(hint, x + 6, by + 2, TextAlignment::Left, TEXT);
    s.text(back, x + w as i32 - 6, by + 2, TextAlignment::Right, TEXT);
}

const KEYBOARD: [(&str, &str, &str); 12] = [
    ("▲", "방향키 ▲", ""),
    ("▼", "방향키 ▼", ""),
    ("◀", "방향키 ◀", ""),
    ("▶", "방향키 ▶", ""),
    ("확인", "Enter", "Space"),
    ("취소", "Backspace", ""),
    ("좌소프트", "왼쪽 Shift", "["),
    ("우소프트", "오른쪽 Shift", "]"),
    ("1", "1", "키패드 1"),
    ("2", "2", "키패드 2"),
    ("3", "3", "키패드 3"),
    ("4", "4", "키패드 4"),
];

fn keyboard_table(s: &mut Screen, picked: usize, second: bool, status: &str) {
    s.bar(0, "키보드 배치", "프리셋: 기본");
    let (c1, c2, c3) = (8, 100, 212);
    s.text("폰 키", c1, BAR + 2, TextAlignment::Left, MUTED);
    s.text("키 1", c2, BAR + 2, TextAlignment::Left, MUTED);
    s.text("키 2", c3, BAR + 2, TextAlignment::Left, MUTED);
    s.fill(0, BAR + LINE + 1, W, 1, EDGE);
    let top = BAR + LINE + 4;
    for (i, (key, one, two)) in KEYBOARD.iter().enumerate().take(if status.is_empty() { 9 } else { 8 }) {
        let y = top + i as i32 * LINE;
        if i == picked {
            s.fill(0, y, W, LINE as u32, ROW);
            let (x, w) = if second { (c3 - 4, W as i32 - c3 + 4) } else { (c2 - 4, c3 - c2 - 4) };
            s.fill(x, y, w as u32, LINE as u32, HIGHLIGHT);
        }
        s.text(key, c1, y + 1, TextAlignment::Left, TEXT);
        s.text(one, c2, y + 1, TextAlignment::Left, ACCENT);
        s.text(
            if two.is_empty() { "-" } else { two },
            c3,
            y + 1,
            TextAlignment::Left,
            if two.is_empty() { MUTED } else { ACCENT },
        );
    }
    s.text("▼", W as i32 - 8, top + 8 * LINE + 1, TextAlignment::Right, MUTED);
    if !status.is_empty() {
        s.fill(0, H as i32 - 2 * BAR, W, BAR as u32, PANEL);
        s.text(status, 8, H as i32 - 2 * BAR + 2, TextAlignment::Left, ACCENT);
    }
    s.bar(H as i32 - BAR, "Enter 바꾸기  Del 지우기", "Esc 뒤로");
}

#[test]
#[ignore]
fn render() {
    // A. First run: no games yet.
    {
        let mut s = Screen::new(W, H);
        s.bar(0, "MiniMobile", "");
        s.paragraph("게임이 없습니다.", BAR + 22, TEXT);
        s.paragraph(
            "게임 파일(.zip, .jar)을\n이 창에 끌어다 놓으면 추가됩니다.\n\n또는 games 폴더에 넣으세요.",
            BAR + 52,
            MUTED,
        );
        let (bx, by, bw) = (90, 170, 140);
        s.fill(bx, by, bw, 22, ROW);
        s.text("F2 games 폴더 열기", bx + bw as i32 / 2, by + 3, TextAlignment::Center, ACCENT);
        s.bar(H as i32 - BAR, "Esc 메뉴", "F11 전체화면");
        save("a_first_run", s);
    }

    // B. The list, with a game just dropped on the window.
    {
        let mut s = Screen::new(W, H);
        s.bar(0, "MiniMobile", "2/4");
        let games = ["레전드 오브 마스터", "소울게이트", "제노니아2", "크로이센"];
        for (i, name) in games.iter().enumerate() {
            let note = if i == 3 { "새로 추가" } else { "" };
            row(&mut s, BAR + 4 + i as i32 * LINE, name, note, i == 1, W);
        }
        s.fill(0, H as i32 - 2 * BAR, W, BAR as u32, PANEL);
        s.text("추가했습니다: 크로이센.zip", 8, H as i32 - 2 * BAR + 2, TextAlignment::Left, ACCENT);
        s.bar(H as i32 - BAR, "Enter 실행  Esc 메뉴", "F11 전체화면");
        save("b_list", s);
    }

    // C. A toast shown for a few seconds as a game starts.
    {
        let (w, h) = (228, 22);
        let mut s = Screen::new(w, h);
        s.fill(0, 0, w, h, EDGE);
        s.fill(1, 1, w - 2, h - 2, PANEL);
        s.text("Esc 메뉴 · F11 전체화면", w as i32 / 2, 3, TextAlignment::Center, TEXT);
        save("c_toast", s);
    }

    // D. Esc in a game: the pause menu.
    {
        let (w, h) = (264, 2 * BAR as u32 + 8 + 7 * LINE as u32);
        let mut s = Screen::new(w, h);
        panel(&mut s, 0, 0, w, h, "메뉴", "일시정지", "Enter 선택", "Esc 닫기");
        let items = [
            ("게임으로 돌아가기", ""),
            ("키보드 배치 바꾸기", ""),
            ("패드 버튼 배치 바꾸기", ""),
            ("프리셋", "◀ 기본 ▶"),
            ("이 게임에 프리셋 고정", "끔"),
            ("화면", "◀ 창 3배 ▶"),
            ("게임 끝내기", ""),
        ];
        for (i, (left, right)) in items.iter().enumerate() {
            row(&mut s, BAR + 4 + i as i32 * LINE, left, right, i == 1, w);
        }
        save("d_menu", s);
    }

    // E. The keyboard table, a key just moved off another.
    {
        let mut s = Screen::new(W, H);
        keyboard_table(&mut s, 4, true, "Space를 '5'에서 옮겨 왔습니다.");
        save("e_keyboard", s);
    }

    // F. Waiting for a key.
    {
        let mut s = Screen::new(W, H);
        keyboard_table(&mut s, 4, true, "");
        s.fill(0, 0, W, H, DIM);
        let (x, y, w, h) = (40, 66, 240u32, 108u32);
        panel(&mut s, x, y, w, h, "확인 (키 2)", "지금: Space", "Esc 취소", "Del 없음");
        s.text("쓸 키를 누르세요", x + w as i32 / 2, y + BAR + 18, TextAlignment::Center, TEXT);
        s.text("…", x + w as i32 / 2, y + BAR + 42, TextAlignment::Center, ACCENT);
        save("f_capture", s);
    }

    // G. The pad table, as on the handheld.
    {
        let mut s = Screen::new(W, H);
        s.bar(0, "패드 버튼 배치", "프리셋: 기본");
        let (c1, c2, c3) = (8, 92, 206);
        s.text("버튼", c1, BAR + 2, TextAlignment::Left, MUTED);
        s.text("그냥", c2, BAR + 2, TextAlignment::Left, MUTED);
        s.text("SELECT+", c3, BAR + 2, TextAlignment::Left, MUTED);
        s.fill(0, BAR + LINE + 1, W, 1, EDGE);
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
        for (i, (b, p, sel)) in rows.iter().enumerate() {
            let y = top + i as i32 * LINE;
            if i == 0 {
                s.fill(0, y, W, LINE as u32, ROW);
                s.fill(c2 - 4, y, (c3 - c2 - 4) as u32, LINE as u32, HIGHLIGHT);
            }
            s.text(b, c1, y + 1, TextAlignment::Left, TEXT);
            s.text(p, c2, y + 1, TextAlignment::Left, ACCENT);
            s.text(sel, c3, y + 1, TextAlignment::Left, ACCENT);
        }
        s.bar(H as i32 - BAR, "Enter 바꾸기  ◀▶ 칸", "Esc 뒤로");
        save("g_pad", s);
    }

    // H. Presets: keyboard and pad together.
    {
        let mut s = Screen::new(W, H);
        s.bar(0, "프리셋", "2/3");
        let rows = [
            ("기본", "사용 중"),
            ("레전드 오브 마스터", "이 게임"),
            ("크로이센", ""),
            ("+ 지금 배치를 새 프리셋으로", ""),
        ];
        for (i, (left, right)) in rows.iter().enumerate() {
            row(&mut s, BAR + 4 + i as i32 * LINE, left, right, i == 1, W);
        }
        s.paragraph("프리셋에는 키보드와 패드 배치가\n함께 저장됩니다.", BAR + 4 + 5 * LINE + 6, MUTED);
        s.bar(H as i32 - BAR, "Enter 불러오기 F2 덮어쓰기 Del 지우기", "");
        save("h_presets", s);
    }
}
