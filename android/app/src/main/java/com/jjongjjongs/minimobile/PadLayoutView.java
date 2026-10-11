package com.jjongjjongs.minimobile;

import android.annotation.SuppressLint;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.view.MotionEvent;
import android.view.View;

/**
 * The pad's buttons on a drawing of a gamepad, each with the handset key it
 * presses, as the PC and handheld builds draw them: tabs for the plain layer
 * and the one SELECT holds, the gamepad, and under it the button picked with
 * its key in both layers.
 *
 * <p>Drawn on a 320-unit-wide plan scaled to the view's width. A tap on a
 * button picks it out; a tap on the one picked, or on a key in the strip,
 * asks {@link Listener#change} for a new key.
 */
@SuppressLint("ViewConstructor")
final class PadLayoutView extends View {

    interface Listener {
        /** The player wants a new key for button `index` in one layer. */
        void change(int index, boolean withSelect);
    }

    /** What each handset key is called on a button, by key code. */
    static final String[] KEY_LABELS = {
            "▲", "▼", "◀", "▶", "확인", "좌소프트", "우소프트", "뒤로",
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "#", "저장",
    };

    /**
     * Where each of {@link ControlPads#PAD_CODES} sits on the plan: x, y,
     * width, height and how round.
     */
    private static final float[][] PLACES = {
            {51, 57, 29, 23, 5}, {51, 105, 29, 23, 5}, {20, 81, 29, 23, 5}, {82, 81, 29, 23, 5},
            {268, 79, 35, 28, 14}, {237, 103, 35, 28, 14}, {237, 55, 35, 28, 14}, {206, 79, 35, 28, 14},
            {82, 29, 56, 22, 6}, {182, 29, 56, 22, 6}, {22, 29, 56, 22, 6}, {242, 29, 56, 22, 6},
            {116, 95, 37, 37, 18.5f}, {167, 95, 37, 37, 18.5f},
            {161, 64, 48, 20, 10},
    };

    private static final float[] SELECT_PLACE = {111, 64, 48, 20, 10};
    private static final float PLAN_WIDTH = 320f;
    private static final float PLAN_HEIGHT = 196f;
    private static final float STRIP_TOP = 149f;

    private final ControlStyle style;
    private final ControlPatch.Session session;
    private final Listener listener;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF box = new RectF();

    /** The button picked, in {@link ControlPads#PAD_CODES}' order. */
    int index = 4;
    /** Whether the layer shown is the one SELECT holds. */
    boolean withSelect;

    /** The tabs and the strip's two keys, where they were last drawn, on the plan. */
    private final RectF[] tabs = {new RectF(), new RectF()};
    private final RectF[] slots = {new RectF(), new RectF()};

    PadLayoutView(ControlStyle style, ControlPatch.Session session, Listener listener) {
        super(style.context);
        this.style = style;
        this.session = session;
        this.listener = listener;
        paint.setTypeface(Typeface.DEFAULT_BOLD);
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int width = MeasureSpec.getSize(widthSpec);
        setMeasuredDimension(width, Math.round(width * PLAN_HEIGHT / PLAN_WIDTH));
    }

    private float scale() {
        return getWidth() / PLAN_WIDTH;
    }

    /** Ink on the accent colour: white on the green, the panel on the cyan. */
    private int onAccent() {
        return style.light ? Color.WHITE : style.BG;
    }

    /** The key a button presses in one layer, and whether it is its own there. */
    private int keyOf(int code, boolean layer) {
        Integer key = layer ? session.selectMapping.get(code) : null;
        if (key == null) {
            key = session.mapping.get(code);
        }
        return key == null ? -1 : key;
    }

    private boolean ownIn(int code, boolean layer) {
        return (layer ? session.selectMapping : session.mapping).containsKey(code);
    }

    static String label(int key) {
        return key >= 0 && key < KEY_LABELS.length ? KEY_LABELS[key] : "없음";
    }

    private void rounded(float x, float y, float width, float height, float radius, int fill, int edge) {
        float s = scale();
        box.set(x * s, y * s, (x + width) * s, (y + height) * s);
        paint.setStyle(Paint.Style.FILL);
        paint.setColor(fill);
        canvasRef.drawRoundRect(box, radius * s, radius * s, paint);
        if (edge != 0) {
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(Math.max(1f, s * 0.8f));
            paint.setColor(edge);
            canvasRef.drawRoundRect(box, radius * s, radius * s, paint);
            paint.setStyle(Paint.Style.FILL);
        }
    }

    /** `text` centred on `x`, its middle on `y`, cut short to `room`. */
    private void text(String text, float x, float y, float size, int color, float room, Paint.Align align) {
        float s = scale();
        paint.setStyle(Paint.Style.FILL);
        paint.setTextSize(size * s);
        paint.setColor(color);
        paint.setTextAlign(align);
        String shown = text;
        while (shown.length() > 1 && paint.measureText(shown) > room * s) {
            shown = shown.substring(0, shown.length() - 1);
        }
        if (!shown.equals(text)) {
            shown = shown.substring(0, Math.max(1, shown.length() - 1)) + "…";
        }
        Paint.FontMetrics metrics = paint.getFontMetrics();
        canvasRef.drawText(shown, x * s, y * s - (metrics.ascent + metrics.descent) / 2f, paint);
    }

    private float width(String text, float size) {
        paint.setTextSize(size);
        return paint.measureText(text);
    }

    private Canvas canvasRef;

    @Override
    protected void onDraw(Canvas canvas) {
        canvasRef = canvas;
        int muted = style.MUTED;
        int none = Color.argb(150, Color.red(muted), Color.green(muted), Color.blue(muted));

        // The tabs.
        String[] tabLabels = {"그냥 누를 때", "SELECT 누른 채"};
        float tabX = 8f;
        for (int tab = 0; tab < 2; tab++) {
            boolean on = (tab == 1) == withSelect;
            float tabWidth = width(tabLabels[tab], 9.5f) + 18f;
            tabs[tab].set(tabX, 2f, tabX + tabWidth, 19f);
            rounded(tabX, 3f, tabWidth, 15f, 7.5f, on ? style.GREEN : style.SOFT, on ? 0 : style.LINE);
            text(tabLabels[tab], tabX + tabWidth / 2f, 10.5f, 9.5f, on ? onAccent() : muted, tabWidth, Paint.Align.CENTER);
            tabX += tabWidth + 4f;
        }

        // The gamepad.
        rounded(12f, 25f, 296f, 118f, 30f, style.SOFTER, style.LINE);
        for (int button = 0; button < PLACES.length; button++) {
            float[] place = PLACES[button];
            int code = ControlPads.PAD_CODES[button];
            boolean picked = button == index;
            int key = keyOf(code, withSelect);
            boolean own = key >= 0 && ownIn(code, withSelect);
            rounded(place[0], place[1], place[2], place[3], place[4],
                    picked ? style.GREEN : style.SOFT, picked ? style.GREEN : style.LINE);
            float middle = place[0] + place[2] / 2f;
            float top = place[1] + (place[3] - 19f) / 2f;
            text(ControlPads.PAD_NAMES[button], middle, top + 4.5f, 7.5f, picked ? onAccent() : muted, place[2] - 4f, Paint.Align.CENTER);
            int ink = picked ? onAccent() : own ? style.DEEP : none;
            text(label(key), middle, top + 14f, 8.5f, ink, place[2] - 4f, Paint.Align.CENTER);
        }
        rounded(SELECT_PLACE[0], SELECT_PLACE[1], SELECT_PLACE[2], SELECT_PLACE[3], SELECT_PLACE[4], style.SOFT, style.LINE);
        float selectMiddle = SELECT_PLACE[0] + SELECT_PLACE[2] / 2f;
        text("SELECT", selectMiddle, SELECT_PLACE[1] + 5.5f, 7.5f, muted, SELECT_PLACE[2] - 4f, Paint.Align.CENTER);
        text("숫자 전환", selectMiddle, SELECT_PLACE[1] + 14.5f, 7.5f, muted, SELECT_PLACE[2] - 4f, Paint.Align.CENTER);

        // The button picked, and its key in each layer.
        rounded(8f, STRIP_TOP, 304f, 44f, 7f, style.SOFTER, style.LINE);
        String name = ControlPads.PAD_NAMES[index];
        float big = Math.max(width(name, 11.5f) + 20f, 34f);
        rounded(17f, STRIP_TOP + 8f, big, 28f, 14f, style.GREEN, 0);
        text(name, 17f + big / 2f, STRIP_TOP + 22f, 11.5f, onAccent(), big, Paint.Align.CENTER);
        float slotX = 17f + big + 8f;
        text("→", slotX, STRIP_TOP + 22f, 11.5f, muted, 20f, Paint.Align.LEFT);
        slotX += 20f;
        String[] slotLabels = {"그냥", "SELECT+"};
        int code = ControlPads.PAD_CODES[index];
        for (int layer = 0; layer < 2; layer++) {
            boolean on = (layer == 1) == withSelect;
            int key = keyOf(code, layer == 1);
            boolean own = key >= 0 && ownIn(code, layer == 1);
            String shown = label(key);
            float slotWidth = Math.max(width(shown, 11.5f) + 16f, 56f);
            text(slotLabels[layer], slotX, STRIP_TOP + 10f, 9f, muted, 80f, Paint.Align.LEFT);
            rounded(slotX, STRIP_TOP + 18f, slotWidth, 20f, 5f, on ? style.SOFT : style.SOFTER, on ? style.GREEN : style.LINE);
            text(shown, slotX + slotWidth / 2f, STRIP_TOP + 28f, 11.5f, own ? (on ? style.INK : style.DEEP) : none, slotWidth, Paint.Align.CENTER);
            slots[layer].set(slotX, STRIP_TOP, slotX + slotWidth, STRIP_TOP + 44f);
            slotX += slotWidth + 12f;
        }
        canvasRef = null;
    }

    /** Picks out the button for a pad key code; whether it is one drawn. */
    boolean pickCode(int keyCode) {
        for (int button = 0; button < ControlPads.PAD_CODES.length; button++) {
            if (ControlPads.PAD_CODES[button] == keyCode) {
                index = button;
                invalidate();
                return true;
            }
        }
        return false;
    }

    @SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(MotionEvent event) {
        if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
            return true;
        }
        if (event.getActionMasked() != MotionEvent.ACTION_UP) {
            return true;
        }
        float x = event.getX() / scale();
        float y = event.getY() / scale();
        for (int tab = 0; tab < 2; tab++) {
            if (tabs[tab].contains(x, y)) {
                withSelect = tab == 1;
                invalidate();
                return true;
            }
        }
        for (int layer = 0; layer < 2; layer++) {
            if (slots[layer].contains(x, y)) {
                withSelect = layer == 1;
                invalidate();
                listener.change(index, withSelect);
                return true;
            }
        }
        for (int button = 0; button < PLACES.length; button++) {
            float[] place = PLACES[button];
            // A little slack round each button, for a finger.
            if (x >= place[0] - 3 && x <= place[0] + place[2] + 3 && y >= place[1] - 3 && y <= place[1] + place[3] + 3) {
                if (button == index) {
                    listener.change(index, withSelect);
                } else {
                    index = button;
                    invalidate();
                }
                return true;
            }
        }
        return true;
    }
}
