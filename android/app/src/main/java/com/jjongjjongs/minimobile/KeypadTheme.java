package com.jjongjjongs.minimobile;

import android.content.Context;
import android.graphics.Color;

/**
 * How the on-screen keypad looks: one of three faces the player picks from
 * 조작 설정 → 키패드 디자인, for every title at once.
 *
 * <p>Only colours and corner shapes live here. Where the keys are is the
 * layout's business and does not change with the face, so a player's own
 * key positions survive a change of look.
 */
final class KeypadTheme {
    static final int GOLD = 0;
    static final int GLASS = 1;
    static final int SILVER = 2;

    static final String[] NAMES = {"골드", "다크 글래스", "실버"};
    static final String[] DESCRIPTIONS = {
            "금색 각인에 따뜻한 갈색 바탕. 지금까지의 키패드를 다듬은 모양",
            "앱 화면과 같은 검정·청록. 테두리 없는 둥근 키",
            "은색 폴더폰처럼 밝은 바탕, 알약 모양 숫자키",
    };

    private static final String PREFS = "mini_keypad_style";
    private static final String KEY = "theme";

    final int id;
    final int trayTop;
    final int trayBottom;

    final int faceTop;
    final int faceBottom;
    /** 0 for a key without an outline. */
    final int edge;
    final int ink;
    final int subInk;
    final int pressedTop;
    final int pressedBottom;
    final int pressedInk;
    /** Drawn a little below the face so it reads as raised; 0 for none. */
    final int shadow;
    final int radiusDp;
    /** Number keys as pills rather than rounded squares. */
    final boolean pillNumbers;

    /**
     * The function row: L, R, 저장 and 뒤로가기. A hollow row is an outline
     * over the tray rather than a key face.
     */
    final boolean hollowFunctions;
    final int fnEdge;
    final int fnInk;
    final boolean pillFunctions;
    final int fnRadiusDp;
    /** 저장 filled with this rather than the ordinary face; 0 for the ordinary face. */
    final int saveFill;
    final int saveEdge;
    final int saveInk;
    final int backEdge;
    final int backInk;

    /** The direction disc: its face, rim, the seams between quarters and the arrows. */
    final int ringTop;
    final int ringBottom;
    final int ringEdge;
    final int ringSeam;
    final int arrow;
    final int arrowPressed;
    /** Laid over a pressed quarter. */
    final int wedgePressed;
    final int okTop;
    final int okBottom;
    final int okEdge;
    final int okInk;

    private KeypadTheme(int id) {
        this.id = id;
        switch (id) {
            case GLASS:
                trayTop = trayBottom = Color.rgb(17, 18, 22);
                faceTop = faceBottom = Color.rgb(34, 37, 45);
                edge = 0;
                ink = Color.rgb(236, 238, 242);
                subInk = Color.rgb(125, 134, 150);
                pressedTop = pressedBottom = Color.rgb(58, 142, 154);
                pressedInk = Color.WHITE;
                shadow = 0;
                radiusDp = 16;
                pillNumbers = false;
                hollowFunctions = true;
                fnEdge = Color.rgb(52, 56, 68);
                fnInk = Color.rgb(198, 202, 211);
                pillFunctions = true;
                fnRadiusDp = 16;
                saveFill = Color.rgb(84, 199, 214);
                saveEdge = Color.rgb(84, 199, 214);
                saveInk = Color.rgb(12, 42, 47);
                backEdge = Color.rgb(74, 47, 49);
                backInk = Color.rgb(255, 138, 128);
                ringTop = ringBottom = Color.rgb(27, 29, 35);
                ringEdge = Color.rgb(44, 48, 58);
                ringSeam = Color.rgb(42, 46, 55);
                arrow = Color.rgb(154, 163, 178);
                arrowPressed = Color.rgb(84, 199, 214);
                wedgePressed = Color.argb(51, 84, 199, 214);
                okTop = okBottom = Color.rgb(43, 47, 57);
                okEdge = 0;
                okInk = Color.rgb(236, 238, 242);
                break;
            case SILVER:
                trayTop = Color.rgb(215, 219, 224);
                trayBottom = Color.rgb(185, 191, 199);
                faceTop = Color.rgb(251, 252, 253);
                faceBottom = Color.rgb(227, 231, 236);
                edge = Color.rgb(154, 162, 173);
                ink = Color.rgb(28, 35, 48);
                subInk = Color.rgb(93, 102, 117);
                pressedTop = Color.rgb(205, 211, 219);
                pressedBottom = Color.rgb(191, 198, 207);
                pressedInk = Color.rgb(28, 35, 48);
                shadow = Color.rgb(141, 148, 158);
                radiusDp = 12;
                pillNumbers = true;
                hollowFunctions = false;
                fnEdge = edge;
                fnInk = ink;
                pillFunctions = false;
                fnRadiusDp = 12;
                saveFill = 0;
                saveEdge = edge;
                saveInk = Color.rgb(27, 143, 58);
                backEdge = edge;
                backInk = Color.rgb(200, 53, 43);
                ringTop = Color.rgb(244, 246, 248);
                ringBottom = Color.rgb(184, 190, 199);
                ringEdge = Color.rgb(143, 151, 162);
                ringSeam = 0;
                arrow = Color.rgb(58, 66, 80);
                arrowPressed = Color.rgb(11, 99, 201);
                wedgePressed = Color.argb(41, 11, 99, 201);
                okTop = Color.rgb(223, 227, 232);
                okBottom = Color.rgb(195, 201, 209);
                okEdge = Color.rgb(143, 151, 162);
                okInk = Color.rgb(28, 35, 48);
                break;
            case GOLD:
            default:
                trayTop = Color.rgb(22, 19, 15);
                trayBottom = Color.rgb(42, 36, 29);
                faceTop = Color.rgb(58, 50, 41);
                faceBottom = Color.rgb(41, 35, 28);
                edge = Color.rgb(110, 90, 58);
                ink = Color.rgb(236, 208, 155);
                subInk = Color.rgb(165, 142, 100);
                pressedTop = Color.rgb(93, 78, 58);
                pressedBottom = Color.rgb(74, 62, 47);
                pressedInk = Color.rgb(255, 243, 214);
                shadow = Color.rgb(13, 11, 8);
                radiusDp = 13;
                pillNumbers = false;
                hollowFunctions = false;
                fnEdge = edge;
                fnInk = ink;
                pillFunctions = true;
                fnRadiusDp = 13;
                saveFill = 0;
                saveEdge = edge;
                saveInk = Color.rgb(143, 210, 143);
                backEdge = edge;
                backInk = Color.rgb(231, 118, 108);
                ringTop = Color.rgb(53, 45, 37);
                ringBottom = Color.rgb(43, 37, 30);
                ringEdge = Color.rgb(110, 90, 58);
                ringSeam = Color.argb(140, 110, 90, 58);
                arrow = Color.rgb(236, 208, 155);
                arrowPressed = Color.WHITE;
                wedgePressed = Color.argb(56, 233, 196, 127);
                okTop = Color.rgb(67, 57, 48);
                okBottom = Color.rgb(47, 40, 32);
                okEdge = Color.rgb(140, 113, 72);
                okInk = Color.rgb(236, 208, 155);
                break;
        }
    }

    private static final KeypadTheme[] ALL = {new KeypadTheme(GOLD), new KeypadTheme(GLASS), new KeypadTheme(SILVER)};

    static KeypadTheme of(int id) {
        return id >= 0 && id < ALL.length ? ALL[id] : ALL[GOLD];
    }

    /** The face the player chose, gold until they choose. */
    static int saved(Context context) {
        int id = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getInt(KEY, GOLD);
        return id >= 0 && id < ALL.length ? id : GOLD;
    }

    static void save(Context context, int id) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putInt(KEY, id).apply();
    }
}
