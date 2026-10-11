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
 * The handset's keypad, to choose the key a gamepad button presses: the soft
 * keys, the directions and the call keys on the left, the number pad on the
 * right, as the PC builds draw their keyboard layout. Each key carries the pad
 * buttons already on it; the key the button has now is outlined, the one the
 * pad's D-pad has reached is filled.
 *
 * <p>Drawn on a 320-unit-wide plan scaled to the view's width.
 */
@SuppressLint("ViewConstructor")
final class KeypadPickerView extends View {

    interface Listener {
        /** The key chosen, or -1 for none. */
        void choose(int key);
    }

    /**
     * The keys by handset key code, column and row: three columns of the
     * left block, then three of the number pad. -1 is "none".
     */
    private static final int[][] CELLS = {
            {5, 0, 0}, {0, 1, 0}, {6, 2, 0},
            {2, 0, 1}, {4, 1, 1}, {3, 2, 1},
            {20, 0, 2}, {1, 1, 2},
            {7, 1, 3}, {-1, 2, 3},
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
    private final int current;
    private final String[] notes = new String[CELLS.length];
    private final String noneNote;
    private final RectF[] rects = new RectF[CELLS.length];
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF box = new RectF();
    private int cursor;

    /**
     * @param current the key the button has now, -1 for none
     * @param notes   by key code, what is on each key already: the pad buttons
     * @param noneNote the line under "none"
     */
    KeypadPickerView(ControlStyle style, int current, String[] notes, String noneNote, Listener listener) {
        super(style.context);
        this.style = style;
        this.listener = listener;
        this.current = current;
        this.noneNote = noneNote;
        paint.setTypeface(Typeface.DEFAULT_BOLD);

        float keyWidth = (PLAN_WIDTH - 2 * 4f - BLOCK_GAP - 4 * GAP) / 6f;
        for (int i = 0; i < CELLS.length; i++) {
            int key = CELLS[i][0];
            int column = CELLS[i][1];
            float x = 4f + column * (keyWidth + GAP) + (column >= 3 ? BLOCK_GAP - GAP : 0f);
            float y = 1f + CELLS[i][2] * PITCH;
            rects[i] = new RectF(x, y, x + keyWidth, y + KEY_HEIGHT);
            this.notes[i] = key >= 0 && key < notes.length ? notes[key] : null;
            if (key == current) {
                cursor = i;
            }
        }
        setFocusable(false);
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
            boolean now = key == current;
            box.set(rect.left * s, rect.top * s, rect.right * s, rect.bottom * s);
            float radius = 7f * s;
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(focused ? style.GREEN : key < 0 ? style.SOFTER : style.SOFT);
            canvas.drawRoundRect(box, radius, radius, paint);
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(Math.max(1f, s * (now ? 2f : 0.8f)));
            paint.setColor(now ? style.GREEN : style.LINE);
            canvas.drawRoundRect(box, radius, radius, paint);

            String name = key < 0 ? "없음" : PadLayoutView.label(key);
            String note = key < 0 ? noneNote : notes[i];
            float middle = rect.centerX();
            float room = rect.width() - 4f;
            int ink = focused ? onAccent : key < 0 ? muted : style.INK;
            if (note == null || note.isEmpty()) {
                text(canvas, name, middle, rect.centerY(), name.length() > 2 ? 10.5f : 13f, ink, room);
            } else {
                text(canvas, name, middle, rect.top + 13f, name.length() > 2 ? 10.5f : 12.5f, ink, room);
                text(canvas, note, middle, rect.top + 26f, 7.5f, focused ? onAccent : style.DEEP, room);
            }
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
        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_MOVE) {
            if (hit >= 0 && hit != cursor) {
                cursor = hit;
                invalidate();
            }
            return true;
        }
        if (action == MotionEvent.ACTION_UP && hit >= 0) {
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
