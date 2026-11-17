package com.jjongjjongs.minimobile;

import android.content.Context;

/**
 * Whether the game list and the screens over it are dark or light: the
 * player's choice, made with the button on the list and kept from one run
 * to the next. The game screen and its keypad have their own look.
 */
final class LibraryLook {
    private static final String PREFS = "mini_library_look";
    private static final String KEY = "dark";

    private LibraryLook() {
    }

    static boolean dark(Context context) {
        return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(KEY, false);
    }

    static void setDark(Context context, boolean dark) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(KEY, dark).apply();
    }
}
