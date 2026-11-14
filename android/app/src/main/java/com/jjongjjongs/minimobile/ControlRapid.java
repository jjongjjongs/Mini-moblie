package com.jjongjjongs.minimobile;

import com.jjongjjongs.minimobile.ControlInput;

public final class ControlRapid implements ControlInput.Sink {
    public static final int DEFAULT_PERIOD_MS = 200;
    public static final int[] PERIODS_MS = {500, 333, DEFAULT_PERIOD_MS, 125, 100};
    public static final int[] RATES = {2, 3, 5, 8, 10};
    private boolean scheduled;
    private final Scheduler scheduler;
    private final ControlInput.Sink sink;
    private final boolean[] enabled = new boolean[21];
    private final boolean[] held = new boolean[21];
    private final boolean[] output = new boolean[21];
    private final long[] next = new long[21];
    private int periodMs = DEFAULT_PERIOD_MS;
    private final Runnable pulse = new Runnable() { // from class: com.jjongjjongs.minimobile.ControlRapid.1
        @Override // java.lang.Runnable
        public void run() {
            ControlRapid.this.tick();
        }
    };

    public interface Scheduler {
        boolean allowed();

        void cancel(Runnable runnable);

        long now();

        void post(Runnable runnable, long j);
    }

    public ControlRapid(ControlInput.Sink sink, Scheduler scheduler) {
        this.sink = sink;
        this.scheduler = scheduler;
    }

    private void emit(int i, boolean z) {
        if (this.output[i] != z) {
            this.output[i] = z;
            this.sink.key(i, z);
        }
    }

    private int phaseMs(boolean z) {
        return z ? this.periodMs / 2 : this.periodMs - (this.periodMs / 2);
    }

    public static int rate(int i) {
        for (int i2 = 0; i2 < PERIODS_MS.length; i2++) {
            if (PERIODS_MS[i2] == i) {
                return RATES[i2];
            }
        }
        return 5;
    }

    private void schedule() {
        if (this.scheduled) {
            this.scheduler.cancel(this.pulse);
            this.scheduled = false;
        }
        if (!this.scheduler.allowed()) {
            stop();
            return;
        }
        long j = Long.MAX_VALUE;
        for (int i = 0; i < this.held.length; i++) {
            if (this.held[i] && this.enabled[i]) {
                j = Math.min(j, this.next[i]);
            }
        }
        if (j != Long.MAX_VALUE) {
            this.scheduled = true;
            this.scheduler.post(this.pulse, Math.max(1L, j - this.scheduler.now()));
        }
    }

    public static boolean supported(int i) {
        for (int i2 : PERIODS_MS) {
            if (i2 == i) {
                return true;
            }
        }
        return false;
    }

    /* JADX INFO: Access modifiers changed from: private */
    public void tick() {
        synchronized (this) {
            this.scheduled = false;
            if (!this.scheduler.allowed()) {
                stop();
                return;
            }
            long now = this.scheduler.now();
            for (int i = 0; i < this.held.length; i++) {
                if (this.held[i] && this.enabled[i] && now >= this.next[i]) {
                    emit(i, !this.output[i]);
                    this.next[i] = phaseMs(this.output[i]) + now;
                }
            }
            schedule();
        }
    }

    public synchronized void configure(boolean[] zArr, int i) {
        if (zArr != null) {
            if (zArr.length == this.enabled.length && supported(i)) {
                boolean z = this.periodMs != i;
                this.periodMs = i;
                long now = this.scheduler.now();
                if (!this.scheduler.allowed()) {
                    System.arraycopy(zArr, 0, this.enabled, 0, this.enabled.length);
                    stop();
                    return;
                }
                for (int i2 = 0; i2 < this.enabled.length; i2++) {
                    boolean z2 = this.enabled[i2] ^ zArr[i2];
                    this.enabled[i2] = zArr[i2];
                    if (this.held[i2]) {
                        if (!this.enabled[i2]) {
                            emit(i2, true);
                            this.next[i2] = 0;
                        } else if (z2 || z) {
                            this.next[i2] = phaseMs(this.output[i2]) + now;
                        }
                    }
                }
                schedule();
                return;
            }
        }
        throw new IllegalArgumentException("Invalid rapid-fire settings");
    }

    @Override // com.jjongjjongs.minimobile.ControlInput.Sink
    public synchronized void key(int i, boolean z) {
        if (i >= 0) {
            if (i < this.held.length) {
                if (!z) {
                    this.held[i] = false;
                    this.next[i] = 0;
                    emit(i, false);
                    schedule();
                    return;
                }
                if (!this.held[i] && this.scheduler.allowed()) {
                    this.held[i] = true;
                    emit(i, true);
                    this.next[i] = this.enabled[i] ? this.scheduler.now() + phaseMs(true) : 0L;
                    schedule();
                }
            }
        }
    }

    public synchronized void stop() {
        if (this.scheduled) {
            this.scheduler.cancel(this.pulse);
            this.scheduled = false;
        }
        for (int i = 0; i < this.held.length; i++) {
            this.held[i] = false;
            this.next[i] = 0;
            emit(i, false);
        }
    }
}
