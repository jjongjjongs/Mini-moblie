package com.jjongjjongs.minimobile;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.Typeface;
import android.view.MotionEvent;
import android.view.VelocityTracker;
import android.view.View;
import android.view.ViewConfiguration;
import android.widget.OverScroller;

/**
 * The game-speed ruler: a strip of ticks, one for every tenth from
 * {@link #MIN} to {@link #MAX}, slid sideways under a fixed needle. The value
 * is whichever tick sits under the needle.
 *
 * <p>A finger drags it, a flick throws it on, and wherever it comes to rest it
 * settles onto the nearest tick, so the value is always a whole tenth. Small
 * ticks mark tenths, taller ones halves and the tallest whole speeds, which
 * are labelled along with the halves.
 */
final class SpeedRuler extends View {
    /** The ends of the ruler, in tenths: 0.1x and 4x. */
    static final int MIN = 1;
    static final int MAX = 40;

    interface Listener {
        /** The tick under the needle changed, by the finger or a call. */
        void onTenths(int tenths);
    }

    private final float spacing;
    private final Paint tick = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint label = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint needle = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path arrow = new Path();
    private final OverScroller scroller;
    private final int touchSlop;
    private final int minFling;
    private final int maxFling;
    private final int ink;
    private final int subInk;
    private final int accent;

    /** How far along the ruler the needle is, in pixels from {@link #MIN}. */
    private float offset;
    private int reported = -1;
    private Listener listener;

    private VelocityTracker velocity;
    private float lastX;
    private float downX;
    private boolean dragging;

    SpeedRuler(Context context, int ink, int subInk, int accent) {
        super(context);
        float density = context.getResources().getDisplayMetrics().density;
        this.spacing = 12 * density;
        this.ink = ink;
        this.subInk = subInk;
        this.accent = accent;

        tick.setStrokeCap(Paint.Cap.ROUND);
        label.setTextAlign(Paint.Align.CENTER);
        label.setTextSize(11 * context.getResources().getDisplayMetrics().scaledDensity);
        needle.setColor(accent);
        needle.setStrokeCap(Paint.Cap.ROUND);
        needle.setStrokeWidth(3 * density);

        scroller = new OverScroller(context);
        ViewConfiguration configuration = ViewConfiguration.get(context);
        touchSlop = configuration.getScaledTouchSlop();
        minFling = configuration.getScaledMinimumFlingVelocity();
        maxFling = configuration.getScaledMaximumFlingVelocity();
    }

    void setListener(Listener listener) {
        this.listener = listener;
    }

    /** The tick under the needle. */
    int tenths() {
        return MIN + Math.round(offset / spacing);
    }

    /** Puts `tenths` under the needle, sliding there or jumping. */
    void setTenths(int tenths, boolean animate) {
        float target = (clamp(tenths) - MIN) * spacing;
        scroller.forceFinished(true);
        if (animate && getWidth() > 0) {
            scroller.startScroll(Math.round(offset), 0, Math.round(target - offset), 0, 220);
            postInvalidateOnAnimation();
        } else {
            offset = target;
            report();
            invalidate();
        }
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int height = Math.round(58 * getResources().getDisplayMetrics().density);
        setMeasuredDimension(getDefaultSize(getSuggestedMinimumWidth(), widthSpec), resolveSize(height, heightSpec));
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        if (velocity == null) {
            velocity = VelocityTracker.obtain();
        }
        velocity.addMovement(event);

        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                scroller.forceFinished(true);
                lastX = downX = event.getX();
                dragging = false;
                // The dialog's own scrolling, if any, leaves the ruler alone.
                getParent().requestDisallowInterceptTouchEvent(true);
                return true;
            case MotionEvent.ACTION_MOVE: {
                float x = event.getX();
                if (!dragging && Math.abs(x - downX) > touchSlop) {
                    dragging = true;
                }
                if (dragging) {
                    // The ruler follows the finger, so sliding it left brings
                    // the faster ticks under the needle.
                    moveTo(offset - (x - lastX));
                }
                lastX = x;
                return true;
            }
            case MotionEvent.ACTION_UP: {
                velocity.computeCurrentVelocity(1000, maxFling);
                float speed = -velocity.getXVelocity();
                if (dragging && Math.abs(speed) > minFling) {
                    scroller.fling(Math.round(offset), 0, Math.round(speed), 0, 0, Math.round((MAX - MIN) * spacing), 0, 0);
                    postInvalidateOnAnimation();
                } else if (!dragging) {
                    // A tap: the tick tapped slides under the needle.
                    float tapped = offset + (event.getX() - getWidth() / 2f);
                    setTenths(MIN + Math.round(tapped / spacing), true);
                } else {
                    settle();
                }
                recycle();
                return true;
            }
            case MotionEvent.ACTION_CANCEL:
                settle();
                recycle();
                return true;
            default:
                return true;
        }
    }

    @Override
    public void computeScroll() {
        if (scroller.computeScrollOffset()) {
            moveTo(scroller.getCurrX());
            if (scroller.isFinished()) {
                settle();
            } else {
                postInvalidateOnAnimation();
            }
        }
    }

    /** Slides onto the nearest tick. */
    private void settle() {
        float target = (tenths() - MIN) * spacing;
        // The scroller moves in whole pixels, so within one of the tick is on it.
        if (Math.abs(target - offset) < 1f) {
            offset = target;
            invalidate();
            report();
            return;
        }
        scroller.startScroll(Math.round(offset), 0, Math.round(target - offset), 0, 160);
        postInvalidateOnAnimation();
    }

    private void moveTo(float position) {
        offset = Math.max(0f, Math.min((MAX - MIN) * spacing, position));
        report();
        invalidate();
    }

    private void report() {
        int tenths = tenths();
        if (tenths != reported) {
            reported = tenths;
            if (listener != null) {
                listener.onTenths(tenths);
            }
        }
    }

    private void recycle() {
        if (velocity != null) {
            velocity.recycle();
            velocity = null;
        }
        dragging = false;
    }

    private static int clamp(int tenths) {
        return Math.max(MIN, Math.min(MAX, tenths));
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float density = getResources().getDisplayMetrics().density;
        float centre = getWidth() / 2f;
        float top = 6 * density;

        for (int tenths = MIN; tenths <= MAX; tenths++) {
            float x = centre + (tenths - MIN) * spacing - offset;
            if (x < -spacing || x > getWidth() + spacing) {
                continue;
            }
            // Ticks fade toward the ends of the strip, so it reads as a dial
            // turning under the needle.
            float distance = Math.min(1f, Math.abs(x - centre) / centre);
            int alpha = Math.round(255 * (1f - distance * distance));

            boolean whole = tenths % 10 == 0;
            boolean half = tenths % 5 == 0;
            float length = (whole ? 24 : half ? 16 : 10) * density;
            tick.setStrokeWidth((whole ? 2 : 1) * density);
            tick.setColor(whole ? ink : half ? subInk : Color.rgb(107, 111, 123));
            tick.setAlpha(alpha);
            canvas.drawLine(x, top, x, top + length, tick);

            if (half || tenths == MIN) {
                label.setColor(whole ? ink : subInk);
                label.setAlpha(alpha);
                label.setTypeface(whole ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
                canvas.drawText(format(tenths), x, top + 40 * density, label);
            }
        }

        // The needle, with a small arrowhead over it.
        canvas.drawLine(centre, top, centre, top + 26 * density, needle);
        float head = 6 * density;
        arrow.reset();
        arrow.moveTo(centre - head, top - 4 * density);
        arrow.lineTo(centre + head, top - 4 * density);
        arrow.lineTo(centre, top + 3 * density);
        arrow.close();
        needle.setStyle(Paint.Style.FILL);
        canvas.drawPath(arrow, needle);
        needle.setStyle(Paint.Style.STROKE);
    }

    /** 0.1x, 1x, 1.5x. */
    static String format(int tenths) {
        return tenths % 10 == 0 ? (tenths / 10) + "x" : (tenths / 10) + "." + (tenths % 10) + "x";
    }
}
