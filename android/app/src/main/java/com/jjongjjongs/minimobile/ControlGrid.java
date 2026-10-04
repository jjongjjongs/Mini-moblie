package com.jjongjjongs.minimobile;

public final class ControlGrid {
    public static final int DEFAULT_DP = 8;
    public static final int[] SPACINGS = {4, 8, 12, 16, 24};

    private ControlGrid() {
    }

    private static float[] axis(float f, float f2, float f3, float f4, float f5, boolean z) {
        float retainEdgeGrid;
        float nearest;
        float min = Math.min(f3, Math.max(1, (int) Math.ceil((f5 / f4) - 1.0E-6d)) * f4);
        if (z) {
            nearest = retainEdgeGrid(f, f3, f4, 0.0f, f3 - min);
            retainEdgeGrid = nearest(f2, f4, min, f3 - nearest);
        } else {
            retainEdgeGrid = retainEdgeGrid(f2, f3, f4, min, f3);
            nearest = nearest(f, f4, 0.0f, f3 - retainEdgeGrid);
        }
        return new float[]{nearest, retainEdgeGrid};
    }

    private static float nearest(float f, float f2, float f3, float f4) {
        float max = Math.max(f3, Math.min(f4, f));
        float max2 = Math.max(f3, Math.min(f4, Math.round(max / f2) * f2));
        return f4 - max <= Math.abs(max2 - max) ? f4 : max2;
    }

    private static float retainEdgeGrid(float f, float f2, float f3, float f4, float f5) {
        float round = f2 - (Math.round((f2 - f) / f3) * f3);
        return (f < f4 || f > f5 || Math.abs(round - f) >= 0.001f) ? nearest(f, f3, f4, f5) : Math.max(f4, Math.min(f5, round));
    }

    public static float[] snap(float f, float f2, float f3, float f4, float f5, float f6, float f7, float f8, boolean z) {
        if (f7 <= 0.0f || f5 <= 0.0f || f6 <= 0.0f || Float.isInfinite(f7)) {
            throw new IllegalArgumentException("Invalid grid extent");
        }
        float[] axis = axis(f, f3, f5, f7, f8, z);
        float[] axis2 = axis(f2, f4, f6, f7, f8, z);
        return new float[]{axis[0], axis2[0], axis[1], axis2[1]};
    }

    public static boolean supported(int i) {
        for (int i2 : SPACINGS) {
            if (i2 == i) {
                return true;
            }
        }
        return false;
    }
}
