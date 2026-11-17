package com.jjongjjongs.minimobile;

import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

public final class ControlInput {
    private final Sink sink;
    private final Map<String, Integer> sources = new HashMap();
    private final int[] counts = new int[21];

    public interface Sink {
        void key(int i, boolean z);
    }

    public ControlInput(Sink sink) {
        this.sink = sink;
    }

    public synchronized boolean pressed(int i) {
        if (i >= 0) {
            if (i < this.counts.length) {
                if (this.counts[i] > 0) {
                    return true;
                }
            }
        }
        return false;
    }

    public synchronized void releaseAll() {
        this.sources.clear();
        for (int i = 0; i < this.counts.length; i++) {
            if (this.counts[i] != 0) {
                this.counts[i] = 0;
                this.sink.key(i, false);
            }
        }
    }

    public synchronized void releasePrefix(String str) {
        Iterator<Map.Entry<String, Integer>> it = this.sources.entrySet().iterator();
        while (it.hasNext()) {
            Map.Entry<String, Integer> next = it.next();
            if (next.getKey().startsWith(str)) {
                int intValue = next.getValue().intValue();
                it.remove();
                int[] iArr = this.counts;
                int i = iArr[intValue] - 1;
                iArr[intValue] = i;
                if (i == 0) {
                    this.sink.key(intValue, false);
                }
            }
        }
    }

    public synchronized void set(String str, int i, boolean z) {
        Integer num = this.sources.get(str);
        if (num != null && (!z || num.intValue() != i)) {
            this.sources.remove(str);
            int[] iArr = this.counts;
            int intValue = num.intValue();
            int i2 = iArr[intValue] - 1;
            iArr[intValue] = i2;
            if (i2 == 0) {
                this.sink.key(num.intValue(), false);
            }
            num = null;
        }
        if (z && i >= 0 && i < this.counts.length && num == null) {
            this.sources.put(str, Integer.valueOf(i));
            int[] iArr2 = this.counts;
            int i3 = iArr2[i];
            iArr2[i] = i3 + 1;
            if (i3 == 0) {
                this.sink.key(i, true);
            }
        }
    }
}
