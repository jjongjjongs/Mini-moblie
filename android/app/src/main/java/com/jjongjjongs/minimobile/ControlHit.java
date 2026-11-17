package com.jjongjjongs.minimobile;

import java.util.List;

public final class ControlHit {

    public interface Region {
        int code();

        boolean contains(float f, float f2);
    }

    private ControlHit() {
    }

    public static <T extends Region> T pick(List<T> list, boolean[] zArr, float f, float f2, boolean z) {
        T t = null;
        for (int size = list.size() - 1; size >= 0; size--) {
            T t2 = list.get(size);
            int code = t2.code();
            if (code >= 0 && code < zArr.length && !zArr[code] && t2.contains(f, f2)) {
                if (z) {
                    return t2;
                }
                if (t != null) {
                    return null;
                }
                t = t2;
            }
        }
        return t;
    }
}
