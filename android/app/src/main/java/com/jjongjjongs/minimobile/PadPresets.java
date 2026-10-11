package com.jjongjjongs.minimobile;

import android.content.Context;
import android.content.SharedPreferences;

import java.util.ArrayList;
import java.util.List;

/**
 * The named mappings a player keeps, and which of them the pad is on.
 *
 * <p>A game wants its own pad: a shooter that needs the number keys under the
 * triggers is not the mapping a tycoon wants, and re-pointing seventeen buttons
 * every time a game changes is the reason the screen was worth opening at all.
 * So a mapping is kept under a name, and the key-mapping screen edits whichever
 * one is active.
 *
 * <p>The active preset and the live mapping - the one
 * {@link PadMapping#load(Context)} reads and the runtime presses keys through -
 * are kept saying the same thing: choosing a preset writes it to both, and
 * saving the screen writes the edited mapping to both. So "has this screen been
 * changed" is answered by comparing against the live mapping, the way leaving
 * the screen already answered it, and a pad that is unplugged mid-edit still
 * presses what the player last settled on.
 *
 * <p>Everything lives in {@link PadMapping#PREFS}, each preset's mapping under
 * a prefix of its own.
 */
final class PadPresets {

    /** How many presets a player may keep. */
    static final int MAX_PRESETS = 8;

    /** How long a preset's name may be. */
    static final int MAX_NAME = 12;

    private static final String KEY_COUNT = "preset.count";
    private static final String KEY_ACTIVE = "preset.active";

    /** One preset: what it is called, and the mapping it holds. */
    static final class Preset {
        String name;
        final PadMapping mapping;

        private Preset(String name, PadMapping mapping) {
            this.name = name;
            this.mapping = mapping;
        }
    }

    private final List<Preset> presets = new ArrayList<>();
    private int active;

    private PadPresets() {
    }

    /**
     * The presets the player last saved.
     *
     * <p>An install that predates these has a mapping and no presets, so `live`
     * becomes the one preset it is under: the player finds the keys they set,
     * under a name, rather than finding the defaults back.
     */
    static PadPresets load(Context context, PadMapping live) {
        SharedPreferences prefs = context.getSharedPreferences(PadMapping.PREFS, Context.MODE_PRIVATE);
        PadPresets held = new PadPresets();

        int count = Math.min(Math.max(prefs.getInt(KEY_COUNT, 0), 0), MAX_PRESETS);
        for (int index = 0; index < count; index++) {
            String stored = prefs.getString(nameKey(index), null);
            String name = stored == null || stored.trim().isEmpty() ? defaultName(index) : stored;
            held.presets.add(new Preset(name, PadMapping.read(prefs, mappingPrefix(index))));
        }

        if (held.presets.isEmpty()) {
            held.presets.add(new Preset(defaultName(0), live.copy()));
            held.active = 0;
            held.save(context);

            return held;
        }

        held.active = Math.max(0, Math.min(prefs.getInt(KEY_ACTIVE, 0), held.presets.size() - 1));

        return held;
    }

    void save(Context context) {
        SharedPreferences.Editor editor =
                context.getSharedPreferences(PadMapping.PREFS, Context.MODE_PRIVATE).edit();

        editor.putInt(KEY_COUNT, presets.size());
        editor.putInt(KEY_ACTIVE, active);

        for (int index = 0; index < presets.size(); index++) {
            Preset preset = presets.get(index);
            editor.putString(nameKey(index), preset.name);
            preset.mapping.write(editor, mappingPrefix(index));
        }

        for (int index = presets.size(); index < MAX_PRESETS; index++) {
            editor.remove(nameKey(index));
            PadMapping.forget(editor, mappingPrefix(index));
        }

        editor.apply();
    }

    int size() {
        return presets.size();
    }

    Preset at(int index) {
        return presets.get(index);
    }

    int activeIndex() {
        return active;
    }

    Preset active() {
        return presets.get(active);
    }

    String activeName() {
        return active().name;
    }

    /** Whether `index` names a preset, which a screen built from a stale list may not. */
    boolean holds(int index) {
        return index >= 0 && index < presets.size();
    }

    void setActive(int index) {
        if (holds(index)) {
            active = index;
        }
    }

    boolean canAdd() {
        return presets.size() < MAX_PRESETS;
    }

    /** The last preset is not deletable: the pad has to be on something. */
    boolean canRemove() {
        return presets.size() > 1;
    }

    /** Adds `mapping` under `name` and makes it the active one. */
    void add(String name, PadMapping mapping) {
        if (!canAdd()) {
            return;
        }

        presets.add(new Preset(name, mapping.copy()));
        active = presets.size() - 1;
    }

    void remove(int index) {
        if (!holds(index) || !canRemove()) {
            return;
        }

        presets.remove(index);

        // The one after it takes its place, so deleting the preset a player is
        // on leaves them on its neighbour rather than on the first in the list.
        if (active > index || active >= presets.size()) {
            active = Math.max(0, Math.min(active - (active > index ? 1 : 0), presets.size() - 1));
        }
    }

    /** A name for a new preset that no preset is already under. */
    String suggestName() {
        for (int number = presets.size() + 1; number <= MAX_PRESETS + 1; number++) {
            String candidate = "프리셋 " + number;
            if (!isTaken(candidate)) {
                return candidate;
            }
        }

        return "프리셋";
    }

    private boolean isTaken(String name) {
        for (Preset preset : presets) {
            if (preset.name.equals(name)) {
                return true;
            }
        }

        return false;
    }

    /**
     * What a name typed into the rename box becomes.
     *
     * <p>Trimmed and cut to length, and an empty one falls back to the preset's
     * place in the list - a preset with no name at all is one a player cannot
     * tell from another.
     */
    static String cleanName(String typed, int index) {
        String name = typed == null ? "" : typed.trim();

        if (name.length() > MAX_NAME) {
            name = name.substring(0, MAX_NAME);
        }

        return name.isEmpty() ? defaultName(index) : name;
    }

    private static String defaultName(int index) {
        return index == 0 ? "기본" : "프리셋 " + (index + 1);
    }

    private static String nameKey(int index) {
        return "preset." + index + ".name";
    }

    private static String mappingPrefix(int index) {
        return "preset." + index + ".pad.";
    }
}
