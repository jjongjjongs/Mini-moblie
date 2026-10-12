package com.jjongjjongs.minimobile;

import android.annotation.SuppressLint;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;

/**
 * The handset's keypad, to choose the key to put pad buttons on: the soft
 * keys, the directions and the call keys on the left, the number pad on the
 * right, as the PC builds draw their keyboard and pad layouts. Each key carries
 * the pad buttons already on it in the layer shown; the one picked - by a tap,
 * or the pad's D-pad - is filled.
 *
 * <p>Drawn on a 320-unit-wide plan scaled to the view's width.
 */
@SuppressLint("ViewConstructor")
final class KeypadPickerView extends View {

    interface Listener {
        /** The key picked out, by a tap or the D-pad. */
        void focus(int key);

        /** The key chosen: tapped again, or A on the pad. */
        void choose(int key);
    }

    /** What each handset key is called, by key code. */
    private static final String[] KEY_LABELS = {
            "▲", "▼", "◀", "▶", "확인", "좌소프트", "우소프트", "뒤로",
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "#", "저장", "종료",
    };

    static String label(int key) {
        return key >= 0 && key < KEY_LABELS.length ? KEY_LABELS[key] : "없음";
    }

    /**
     * The keys by handset key code, column and row: three columns of the
     * left block, then three of the number pad.
     */
    private static final int[][] CELLS = {
            {5, 0, 0}, {0, 1, 0}, {6, 2, 0},
            {2, 0, 1}, {4, 1, 1}, {3, 2, 1},
            {20, 0, 2}, {1, 1, 2},
            {7, 1, 3},
            {9, 3, 0}, {10, 4, 0}, {11, 5, 0},
            {12, 3, 1}, {13, 4, 1}, {14, 5, 1},
            {15, 3, 2}, {16, 4, 2}, {17, 5, 2},
            {18, 3, 3}, {8, 4, 3}, {19, 5, 3},
    };

    private static final float PLAN_WIDTH = 320f;
    private static final float KEY_HEIGHT = 34f;
    private static final float PITCH = 38f;
    private static final float GAP = 4f;
    private static final float BLOCK_GAP = 14f;
    private static final float PLAN_HEIGHT = 3 * PITCH + KEY_HEIGHT + 2f;

    private final ControlStyle style;
    private final Listener listener;
    private final String[] notes = new String[CELLS.length];
    private final RectF[] rects = new RectF[CELLS.length];
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF box = new RectF();
    private int cursor;
    private boolean downOnPicked;

    /**
     * @param picked the key picked out to begin with
     * @param notes  by key code, what is on each key: the pad buttons
     */
    KeypadPickerView(ControlStyle style, int picked, String[] notes, Listener listener) {
        super(style.context);
        this.style = style;
        this.listener = listener;
        paint.setTypeface(Typeface.DEFAULT_BOLD);

        float keyWidth = (PLAN_WIDTH - 2 * 4f - BLOCK_GAP - 4 * GAP) / 6f;
        for (int i = 0; i < CELLS.length; i++) {
            int key = CELLS[i][0];
            int column = CELLS[i][1];
            float x = 4f + column * (keyWidth + GAP) + (column >= 3 ? BLOCK_GAP - GAP : 0f);
            float y = 1f + CELLS[i][2] * PITCH;
            rects[i] = new RectF(x, y, x + keyWidth, y + KEY_HEIGHT);
            if (key == picked) {
                cursor = i;
            }
        }
        setNotes(notes);
        setFocusable(false);
    }

    /** What is on each key, by key code, as it is now. */
    void setNotes(String[] byKey) {
        for (int i = 0; i < CELLS.length; i++) {
            int key = CELLS[i][0];
            notes[i] = key < byKey.length ? byKey[key] : null;
        }
        invalidate();
    }

    /** The key picked out. */
    int picked() {
        return CELLS[cursor][0];
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int width = MeasureSpec.getSize(widthSpec);
        setMeasuredDimension(width, Math.round(width * PLAN_HEIGHT / PLAN_WIDTH));
    }

    private float scale() {
        return getWidth() / PLAN_WIDTH;
    }

    private void text(Canvas canvas, String text, float x, float y, float size, int color, float room) {
        float s = scale();
        paint.setStyle(Paint.Style.FILL);
        paint.setTextSize(size * s);
        paint.setColor(color);
        paint.setTextAlign(Paint.Align.CENTER);
        String shown = text;
        if (paint.measureText(shown) > room * s) {
            while (shown.length() > 1 && paint.measureText(shown + "…") > room * s) {
                shown = shown.substring(0, shown.length() - 1);
            }
            shown = shown + "…";
        }
        Paint.FontMetrics metrics = paint.getFontMetrics();
        canvas.drawText(shown, x * s, y * s - (metrics.ascent + metrics.descent) / 2f, paint);
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float s = scale();
        int onAccent = style.light ? Color.WHITE : style.BG;
        int muted = style.MUTED;
        for (int i = 0; i < CELLS.length; i++) {
            int key = CELLS[i][0];
            RectF rect = rects[i];
            boolean focused = i == cursor;
            box.set(rect.left * s, rect.top * s, rect.right * s, rect.bottom * s);
            float radius = 7f * s;
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(focused ? style.GREEN : style.SOFT);
            canvas.drawRoundRect(box, radius, radius, paint);
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(Math.max(1f, s * 0.8f));
            paint.setColor(style.LINE);
            canvas.drawRoundRect(box, radius, radius, paint);

            String name = label(key);
            String note = notes[i];
            boolean none = note == null || note.isEmpty();
            float middle = rect.centerX();
            float room = rect.width() - 4f;
            int ink = focused ? onAccent : style.INK;
            text(canvas, name, middle, rect.top + 13f, name.length() > 2 ? 10.5f : 12.5f, ink, room);
            text(canvas, none ? "없음" : note, middle, rect.top + 26f, 7.5f, focused ? onAccent : none ? muted : style.DEEP, room);
        }
    }

    @SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(MotionEvent event) {
        int action = event.getActionMasked();
        float x = event.getX() / scale();
        float y = event.getY() / scale();
        int hit = -1;
        for (int i = 0; i < rects.length; i++) {
            if (x >= rects[i].left - 2 && x <= rects[i].right + 2 && y >= rects[i].top - 2 && y <= rects[i].bottom + 2) {
                hit = i;
            }
        }
        if (action == MotionEvent.ACTION_DOWN) {
            // A tap on the key picked chooses it; on another, picks it out.
            downOnPicked = hit >= 0 && hit == cursor;
        }
        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_MOVE) {
            if (hit >= 0 && hit != cursor) {
                cursor = hit;
                invalidate();
                listener.focus(CELLS[hit][0]);
            }
            return true;
        }
        if (action == MotionEvent.ACTION_UP && hit >= 0 && hit == cursor && downOnPicked) {
            listener.choose(CELLS[hit][0]);
        }
        return true;
    }

    /**
     * A pad button pressed with the keypad up: the D-pad moves along it, A
     * chooses. Whether it was one of those.
     */
    boolean padKey(int keyCode) {
        switch (keyCode) {
            case KeyEvent.KEYCODE_DPAD_UP:
            case KeyEvent.KEYCODE_DPAD_DOWN:
            case KeyEvent.KEYCODE_DPAD_LEFT:
            case KeyEvent.KEYCODE_DPAD_RIGHT:
                cursor = step(cursor, keyCode);
                invalidate();
                listener.focus(CELLS[cursor][0]);
                return true;
            case KeyEvent.KEYCODE_BUTTON_A:
            case KeyEvent.KEYCODE_DPAD_CENTER:
                listener.choose(CELLS[cursor][0]);
                return true;
            default:
                return false;
        }
    }

    /**
     * The key a direction leads to: of the keys wholly past this one's edge
     * that way, the nearest, distance across the direction counting double.
     */
    private int step(int from, int keyCode) {
        RectF origin = rects[from];
        int best = from;
        float bestDistance = Float.MAX_VALUE;
        for (int i = 0; i < rects.length; i++) {
            RectF rect = rects[i];
            boolean past;
            float along;
            float across;
            switch (keyCode) {
                case KeyEvent.KEYCODE_DPAD_UP:
                    past = rect.bottom <= origin.top;
                    along = origin.centerY() - rect.centerY();
                    across = rect.centerX() - origin.centerX();
                    break;
                case KeyEvent.KEYCODE_DPAD_DOWN:
                    past = rect.top >= origin.bottom;
                    along = rect.centerY() - origin.centerY();
                    across = rect.centerX() - origin.centerX();
                    break;
                case KeyEvent.KEYCODE_DPAD_LEFT:
                    past = rect.right <= origin.left;
                    along = origin.centerX() - rect.centerX();
                    across = rect.centerY() - origin.centerY();
                    break;
                default:
                    past = rect.left >= origin.right;
                    along = rect.centerX() - origin.centerX();
                    across = rect.centerY() - origin.centerY();
                    break;
            }
            float distance = along + 2 * Math.abs(across);
            if (past && distance < bestDistance) {
                best = i;
                bestDistance = distance;
            }
        }
        return best;
    }
}
