package com.jjongjjongs.minimobile;

import android.content.Context;
import android.content.SharedPreferences;
import android.view.KeyEvent;

import java.util.ArrayList;
import java.util.List;

/**
 * Which handset key each gamepad button presses.
 *
 * <p>The table is kept the way the runtime reads it - one handset key per pad
 * button - because that is the question every key event asks, and because it
 * is what makes the rule "a button drives one key" true by construction. The
 * settings screen shows the inverse, one row per handset key, so a pad key can
 * be read off the thing the player is looking for; a handset key naming more
 * than one button is just that many entries pointing at it, which is how L1 and
 * L2 have always both reached the left soft key.
 *
 * <p>A pad button assigned to nothing is {@link #UNASSIGNED}, and the event for
 * it is left to the system.
 */
final class PadMapping {

    /** A pad button that presses no handset key. */
    static final int UNASSIGNED = -1;

    // Pad buttons, in the order the picker lays them out: the D-pad, the four
    // face buttons, the shoulders and triggers, then the three a pad puts
    // around its middle.
    static final int PAD_DPAD_UP = 0;
    static final int PAD_DPAD_DOWN = 1;
    static final int PAD_DPAD_LEFT = 2;
    static final int PAD_DPAD_RIGHT = 3;
    static final int PAD_A = 4;
    static final int PAD_L2 = 9;
    static final int PAD_R2 = 11;

    static final int PAD_COUNT = 15;

    /** The Android key code each pad button arrives as. */
    private static final int[] PAD_KEY_CODES = {
            KeyEvent.KEYCODE_DPAD_UP,
            KeyEvent.KEYCODE_DPAD_DOWN,
            KeyEvent.KEYCODE_DPAD_LEFT,
            KeyEvent.KEYCODE_DPAD_RIGHT,
            KeyEvent.KEYCODE_BUTTON_A,
            KeyEvent.KEYCODE_BUTTON_B,
            KeyEvent.KEYCODE_BUTTON_X,
            KeyEvent.KEYCODE_BUTTON_Y,
            KeyEvent.KEYCODE_BUTTON_L1,
            KeyEvent.KEYCODE_BUTTON_L2,
            KeyEvent.KEYCODE_BUTTON_R1,
            KeyEvent.KEYCODE_BUTTON_R2,
            KeyEvent.KEYCODE_BUTTON_START,
            KeyEvent.KEYCODE_BUTTON_SELECT,
            KeyEvent.KEYCODE_BUTTON_MODE,
    };

    /** What the picker calls each pad button. */
    static final String[] PAD_LABELS = {
            "D▲", "D▼", "D◀", "D▶",
            "A", "B", "X", "Y",
            "L1", "L2", "R1", "R2",
            "START", "SELECT", "HOME",
    };

    /** A word of warning under a button, or null for one that needs none. */
    static final String[] PAD_NOTES = {
            null, null, null, null,
            null, null, null, null,
            null, null, null, null,
            null, null, "기기따라 제한",
    };

    /**
     * Where the pad starts out, which is where it has always been: the D-pad on
     * the directions, A on the confirm key, B on back, X and Y on the two keys
     * a handset put below its pad, and each shoulder pair on the soft key over
     * it. `기본값으로` puts exactly this back.
     */
    private static final int[] DEFAULTS = {
            MainActivity.CODE_UP, MainActivity.CODE_DOWN,
            MainActivity.CODE_LEFT, MainActivity.CODE_RIGHT,
            MainActivity.CODE_OK, MainActivity.CODE_CLEAR,
            MainActivity.CODE_STAR, MainActivity.CODE_HASH,
            MainActivity.CODE_SOFT_L, MainActivity.CODE_SOFT_L,
            MainActivity.CODE_SOFT_R, MainActivity.CODE_SOFT_R,
            UNASSIGNED, UNASSIGNED, UNASSIGNED,
    };

    private static final String PREFS = "pad_mapping";
    private static final String KEY_PREFIX = "pad.";

    private final int[] assignment = DEFAULTS.clone();

    private PadMapping() {
    }

    /** The mapping the player last saved, or the default one. */
    static PadMapping load(Context context) {
        PadMapping mapping = new PadMapping();
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);

        for (int pad = 0; pad < PAD_COUNT; pad++) {
            int stored = prefs.getInt(KEY_PREFIX + PAD_LABELS[pad], DEFAULTS[pad]);
            mapping.assignment[pad] = MainActivity.isHandsetKey(stored) ? stored : UNASSIGNED;
        }

        return mapping;
    }

    void save(Context context) {
        SharedPreferences.Editor editor = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit();

        for (int pad = 0; pad < PAD_COUNT; pad++) {
            editor.putInt(KEY_PREFIX + PAD_LABELS[pad], assignment[pad]);
        }

        editor.apply();
    }

    /** A working copy, so a screen can be left without keeping what it changed. */
    PadMapping copy() {
        PadMapping other = new PadMapping();
        System.arraycopy(assignment, 0, other.assignment, 0, PAD_COUNT);
        return other;
    }

    void copyFrom(PadMapping other) {
        System.arraycopy(other.assignment, 0, assignment, 0, PAD_COUNT);
    }

    void resetToDefaults() {
        System.arraycopy(DEFAULTS, 0, assignment, 0, PAD_COUNT);
    }

    /** Whether two mappings say the same thing, so a screen knows if it changed. */
    boolean sameAs(PadMapping other) {
        for (int pad = 0; pad < PAD_COUNT; pad++) {
            if (assignment[pad] != other.assignment[pad]) {
                return false;
            }
        }

        return true;
    }

    /** The handset key a pad button presses, or {@link #UNASSIGNED}. */
    int handsetKeyOf(int pad) {
        return pad < 0 || pad >= PAD_COUNT ? UNASSIGNED : assignment[pad];
    }

    /**
     * The handset key an arriving pad event presses.
     *
     * <p>`DPAD_CENTER` is taken as A: a pad that reports its stick click or its
     * centre button that way is naming the key a player pressed to confirm, and
     * every pad that has both sends A for the face button anyway.
     */
    int handsetKeyForKeyCode(int keyCode) {
        if (keyCode == KeyEvent.KEYCODE_DPAD_CENTER) {
            return assignment[PAD_A];
        }

        for (int pad = 0; pad < PAD_COUNT; pad++) {
            if (PAD_KEY_CODES[pad] == keyCode) {
                return assignment[pad];
            }
        }

        return UNASSIGNED;
    }

    /**
     * Gives a pad button to a handset key, taking it from whatever key had it.
     *
     * <p>One button presses one key: a button left on two keys would send both
     * at once, and no game asks for that.
     */
    void assign(int pad, int handsetKey) {
        if (pad < 0 || pad >= PAD_COUNT) {
            return;
        }

        assignment[pad] = MainActivity.isHandsetKey(handsetKey) ? handsetKey : UNASSIGNED;
    }

    /** The pad buttons that press a handset key, in the picker's order. */
    List<Integer> padsFor(int handsetKey) {
        List<Integer> pads = new ArrayList<>();

        for (int pad = 0; pad < PAD_COUNT; pad++) {
            if (assignment[pad] == handsetKey) {
                pads.add(pad);
            }
        }

        return pads;
    }

    /** What a settings row shows on the right: `L1 · L2`, or 없음. */
    String labelFor(int handsetKey) {
        StringBuilder label = new StringBuilder();

        for (int pad : padsFor(handsetKey)) {
            if (label.length() > 0) {
                label.append(" · ");
            }
            label.append(PAD_LABELS[pad]);
        }

        return label.length() == 0 ? "없음" : label.toString();
    }
}
