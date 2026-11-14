package com.jjongjjongs.minimobile;

import android.app.AlertDialog;
import android.content.Context;
import android.content.DialogInterface;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.ColorFilter;
import android.graphics.Paint;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.drawable.Drawable;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;

/**
 * The row of buttons along a dialog's bottom, set off from what is above it
 * by a hairline across the dialog and lifted off its bottom edge: as much
 * room under the buttons as there is beside them, rather than a few pixels.
 * Every dialog of the app has it - the controls' own (see
 * {@link ControlStyle#decorate}) and every other one, made through
 * {@link Builder}.
 */
final class DialogFooter {

    /** A hairline that reads on a light dialog and on a dark one. */
    static final int NEUTRAL_LINE = Color.argb(56, 128, 128, 128);

    private DialogFooter() {
    }

    /**
     * Gives a dialog on screen its footer; one with no buttons is left as it
     * is. The bar's padding grows the bar, where a margin on the buttons would
     * be clipped by it.
     */
    static void apply(AlertDialog dialog, int lineColor) {
        Button shown = null;
        for (int which : new int[]{DialogInterface.BUTTON_POSITIVE, DialogInterface.BUTTON_NEGATIVE, DialogInterface.BUTTON_NEUTRAL}) {
            Button button = dialog.getButton(which);
            if (button != null && button.getVisibility() == View.VISIBLE && button.getText() != null && button.getText().length() > 0) {
                shown = button;
                break;
            }
        }
        if (shown == null || !(shown.getParent() instanceof ViewGroup)) {
            return;
        }
        float density = dialog.getContext().getResources().getDisplayMetrics().density;
        ViewGroup bar = (ViewGroup) shown.getParent();
        bar.setPadding(Math.round(12 * density), Math.round(14 * density), Math.round(12 * density), Math.round(20 * density));
        View panel = bar.getParent() instanceof View ? (View) bar.getParent() : bar;
        panel.setBackground(new TopLine(lineColor, Math.max(1, Math.round(density))));
    }

    /** A line across the top of whatever it is the background of. */
    private static final class TopLine extends Drawable {
        private final Paint paint = new Paint();
        private final int thickness;

        TopLine(int color, int thickness) {
            paint.setColor(color);
            this.thickness = thickness;
        }

        @Override
        public void draw(Canvas canvas) {
            Rect bounds = getBounds();
            canvas.drawRect(bounds.left, bounds.top, bounds.right, bounds.top + thickness, paint);
        }

        @Override
        public void setAlpha(int alpha) {
            paint.setAlpha(alpha);
        }

        @Override
        public void setColorFilter(ColorFilter colorFilter) {
            paint.setColorFilter(colorFilter);
        }

        @Override
        public int getOpacity() {
            return PixelFormat.TRANSLUCENT;
        }
    }

    /** An {@link AlertDialog.Builder} whose dialogs get the footer as they show. */
    static final class Builder extends AlertDialog.Builder {
        Builder(Context context) {
            super(context);
        }

        @Override
        public AlertDialog create() {
            final AlertDialog dialog = super.create();
            dialog.setOnShowListener(new DialogInterface.OnShowListener() {
                @Override
                public void onShow(DialogInterface shownDialog) {
                    apply(dialog, NEUTRAL_LINE);
                }
            });
            return dialog;
        }
    }
}
