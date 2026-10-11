package com.jjongjjongs.minimobile;

import android.view.KeyEvent;

import java.util.Map;
import java.util.TreeMap;

final class ControlPads {
    static final int[] LEGACY_CODES = {19, 20, 21, 22, 96, 97, 99, 100, 102, 104, 103, 105, 106, 107, 108, 109, 110};
    static final int DEFAULT_REVISION = 1;
    static final int[] LEGACY_TARGETS = {0, DEFAULT_REVISION, 2, 3, 4, 7, 18, 19, 5, 5, 6, 6, -1, -1, -1, -1, -1};
    static final String[] LEGACY_LABELS = {"D▲", "D▼", "D◀", "D▶", "A", "B", "X", "Y", "L1", "L2", "R1", "R2", "L3", "R3", "START", "SELECT", "HOME"};

    /**
     * The mapping revision that added a phone's own keys: the number pad, the
     * call key and the soft keys of a handset that runs Android with a real
     * keypad (a folder phone), or of a keyboard. They share the table with a
     * pad's buttons, so the pad mapping screen also re-points them.
     */
    static final int PHONE_REVISION = 2;

    /**
     * The phone keys and the handset key each presses: 0-9, * and #, the
     * D-pad's centre and Enter as OK, call as 저장 (the
     * handset's SEND), the soft keys, menu as the left soft key, and the
     * clear (Delete) key as 뒤로가기.
     */
    static TreeMap<Integer, Integer> phoneDefaults() {
        TreeMap<Integer, Integer> keys = new TreeMap<>();
        for (int digit = 0; digit <= 9; digit++) {
            keys.put(KeyEvent.KEYCODE_0 + digit, 8 + digit);
        }
        keys.put(KeyEvent.KEYCODE_STAR, 18);
        keys.put(KeyEvent.KEYCODE_POUND, 19);
        keys.put(KeyEvent.KEYCODE_DPAD_UP, 0);
        keys.put(KeyEvent.KEYCODE_DPAD_DOWN, 1);
        keys.put(KeyEvent.KEYCODE_DPAD_LEFT, 2);
        keys.put(KeyEvent.KEYCODE_DPAD_RIGHT, 3);
        keys.put(KeyEvent.KEYCODE_DPAD_CENTER, 4);
        keys.put(KeyEvent.KEYCODE_ENTER, 4);
        keys.put(KeyEvent.KEYCODE_CALL, 20);
        keys.put(KeyEvent.KEYCODE_SOFT_LEFT, 5);
        keys.put(KeyEvent.KEYCODE_MENU, 5);
        keys.put(KeyEvent.KEYCODE_SOFT_RIGHT, 6);
        keys.put(KeyEvent.KEYCODE_DEL, 7);
        return keys;
    }

    /** Whether a key code is one of a phone's own keys (see {@link #phoneDefaults}). */
    static boolean isPhoneKey(int keyCode) {
        return phoneDefaults().containsKey(keyCode);
    }

    /**
     * Adds the phone keys to a mapping from before they were in it, leaving
     * any key it already maps as it is. Whether anything was added.
     */
    static boolean addPhoneKeys(Map<Integer, Integer> map, int revision) {
        if (revision >= PHONE_REVISION) {
            return false;
        }
        boolean added = false;
        for (Map.Entry<Integer, Integer> entry : phoneDefaults().entrySet()) {
            if (!map.containsKey(entry.getKey())) {
                map.put(entry.getKey(), entry.getValue());
                added = true;
            }
        }
        return added;
    }

    /**
     * The gamepad buttons the layout screen draws, as the key codes they
     * arrive as, in its order: the D-pad, the face buttons, the shoulders and
     * triggers, the sticks pressed in, START. SELECT is not among them - held,
     * it switches the buttons to their second layer - and neither is HOME.
     */
    static final int[] PAD_CODES = {
            KeyEvent.KEYCODE_DPAD_UP, KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT, KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_BUTTON_A, KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X, KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_L1, KeyEvent.KEYCODE_BUTTON_R1,
            KeyEvent.KEYCODE_BUTTON_L2, KeyEvent.KEYCODE_BUTTON_R2,
            KeyEvent.KEYCODE_BUTTON_THUMBL, KeyEvent.KEYCODE_BUTTON_THUMBR,
            KeyEvent.KEYCODE_BUTTON_START,
    };

    static final String[] PAD_NAMES = {
            "D▲", "D▼", "D◀", "D▶", "A", "B", "X", "Y",
            "L1", "R1", "L2", "R2", "L3", "R3", "START",
    };

    /** SELECT: held, the pad's buttons press their second layer's keys. */
    static final int SELECT = KeyEvent.KEYCODE_BUTTON_SELECT;

    /**
     * Where a preset keeps its second layer: each key code this far up, in
     * the same table as the first, so a preset stays one map.
     */
    static final int SELECT_OFFSET = 4096;

    /**
     * The second layer a pad starts with: the number pad, as the PC and
     * handheld builds have it - the D-pad on 2 8 4 6, A on 5, B on 0, X and Y
     * on 1 and 3, the shoulders on 7 and 9 and the triggers on * and #.
     */
    static TreeMap<Integer, Integer> selectDefaults() {
        TreeMap<Integer, Integer> keys = new TreeMap<>();
        keys.put(KeyEvent.KEYCODE_DPAD_UP, 10);
        keys.put(KeyEvent.KEYCODE_DPAD_DOWN, 16);
        keys.put(KeyEvent.KEYCODE_DPAD_LEFT, 12);
        keys.put(KeyEvent.KEYCODE_DPAD_RIGHT, 14);
        keys.put(KeyEvent.KEYCODE_BUTTON_A, 13);
        keys.put(KeyEvent.KEYCODE_DPAD_CENTER, 13);
        keys.put(KeyEvent.KEYCODE_BUTTON_B, 8);
        keys.put(KeyEvent.KEYCODE_BUTTON_X, 9);
        keys.put(KeyEvent.KEYCODE_BUTTON_Y, 11);
        keys.put(KeyEvent.KEYCODE_BUTTON_L1, 15);
        keys.put(KeyEvent.KEYCODE_BUTTON_R1, 17);
        keys.put(KeyEvent.KEYCODE_BUTTON_L2, 18);
        keys.put(KeyEvent.KEYCODE_BUTTON_R2, 19);
        return keys;
    }

    /** Whether a key code is one of a pad's buttons (the D-pad's centre as A). */
    static boolean isPadCode(int keyCode) {
        if (keyCode == KeyEvent.KEYCODE_DPAD_CENTER) {
            return true;
        }
        for (int code : LEGACY_CODES) {
            if (code == keyCode) {
                return true;
            }
        }
        return false;
    }

    /**
     * Puts a first layer's pad buttons back where {@link #defaults} has them,
     * leaving the phone's own keys - which share the table - as they are.
     */
    static void resetPadButtons(Map<Integer, Integer> map) {
        TreeMap<Integer, Integer> defaults = defaults();
        for (int code : LEGACY_CODES) {
            map.remove(code);
        }
        map.remove(KeyEvent.KEYCODE_DPAD_CENTER);
        for (Map.Entry<Integer, Integer> entry : defaults.entrySet()) {
            if (isPadCode(entry.getKey())) {
                map.put(entry.getKey(), entry.getValue());
            }
        }
    }

    private ControlPads() {
    }

    static TreeMap<Integer, Integer> defaults() {
        TreeMap<Integer, Integer> treeMap = new TreeMap<>();
        for (int i = 0; i < 4; i += DEFAULT_REVISION) {
            treeMap.put(Integer.valueOf(LEGACY_CODES[i]), Integer.valueOf(i));
        }
        addPhoneKeys(treeMap, 0);
        return treeMap;
    }

    static TreeMap<Integer, Integer> fromLegacy(int[] iArr) {
        TreeMap<Integer, Integer> treeMap = new TreeMap<>();
        for (int i = 0; i < LEGACY_CODES.length; i += DEFAULT_REVISION) {
            if (iArr[i] >= 0 && iArr[i] < 21) {
                treeMap.put(Integer.valueOf(LEGACY_CODES[i]), Integer.valueOf(iArr[i]));
            }
        }
        Integer num = treeMap.get(96);
        if (num != null) {
            treeMap.put(23, num);
        }
        return treeMap;
    }

    static boolean upgradeDefault(Map<Integer, Integer> map, int i) {
        if (i >= DEFAULT_REVISION || !map.equals(fromLegacy(LEGACY_TARGETS))) {
            return false;
        }
        map.clear();
        map.putAll(defaults());
        return true;
    }
}
