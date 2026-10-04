package com.jjongjjongs.minimobile;

import java.util.Map;
import java.util.TreeMap;

final class ControlPads {
    static final int[] LEGACY_CODES = {19, 20, 21, 22, 96, 97, 99, 100, 102, 104, 103, 105, 106, 107, 108, 109, 110};
    static final int DEFAULT_REVISION = 1;
    static final int[] LEGACY_TARGETS = {0, DEFAULT_REVISION, 2, 3, 4, 7, 18, 19, 5, 5, 6, 6, -1, -1, -1, -1, -1};
    static final String[] LEGACY_LABELS = {"D▲", "D▼", "D◀", "D▶", "A", "B", "X", "Y", "L1", "L2", "R1", "R2", "L3", "R3", "START", "SELECT", "HOME"};

    private ControlPads() {
    }

    static TreeMap<Integer, Integer> defaults() {
        TreeMap<Integer, Integer> treeMap = new TreeMap<>();
        for (int i = 0; i < 4; i += DEFAULT_REVISION) {
            treeMap.put(Integer.valueOf(LEGACY_CODES[i]), Integer.valueOf(i));
        }
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
