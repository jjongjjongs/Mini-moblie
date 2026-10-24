package com.jjongjjongs.minimobile;

import android.graphics.Canvas;
import android.graphics.ColorFilter;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.RectF;
import android.graphics.drawable.Drawable;

/**
 * A small outline icon for the library's menus, drawn as strokes on a 24-unit
 * grid so it looks the same on every handset - the emoji it replaces were a
 * different picture on every maker's font.
 */
final class LineIcon extends Drawable {

    enum Kind {
        /** An arrow up out of a tray: a save copied out. */
        UPLOAD,
        /** An arrow down into a tray: a save brought back. */
        DOWNLOAD,
        /** A folder with an arrow up: copied to the data folder. */
        FOLDER_OUT,
        /** A folder with an arrow down: read back from the data folder. */
        FOLDER_IN,
        /** A turning arrow: the game's data reset. */
        RESET,
        /** A bin: the game taken off the list. */
        TRASH,
        /** A cross: the menu closed. */
        CLOSE,
    }

    private static final float GRID = 24f;

    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path path = new Path();

    LineIcon(Kind kind, int color) {
        this(kind, color, 2f);
    }

    LineIcon(Kind kind, int color, float stroke) {
        paint.setColor(color);
        paint.setStyle(Paint.Style.STROKE);
        paint.setStrokeWidth(stroke);
        paint.setStrokeCap(Paint.Cap.ROUND);
        paint.setStrokeJoin(Paint.Join.ROUND);
        trace(kind);
    }

    private void trace(Kind kind) {
        switch (kind) {
            case UPLOAD:
                line(12, 15, 12, 4);
                polyline(7, 9, 12, 4, 17, 9);
                tray();
                break;
            case DOWNLOAD:
                line(12, 4, 12, 15);
                polyline(7, 10, 12, 15, 17, 10);
                tray();
                break;
            case FOLDER_OUT:
                folder();
                line(12, 16, 12, 11);
                polyline(9.5f, 13.5f, 12, 11, 14.5f, 13.5f);
                break;
            case FOLDER_IN:
                folder();
                line(12, 11, 12, 16);
                polyline(9.5f, 13.5f, 12, 16, 14.5f, 13.5f);
                break;
            case RESET:
                // Most of a circle from the left round under to the upper left,
                // with the arrowhead at its start.
                path.arcTo(new RectF(4, 4, 20, 20), 180f, -315.5f, true);
                polyline(4, 4, 4, 8, 8, 8);
                break;
            case TRASH:
                line(5, 7, 19, 7);
                polyline(10, 7, 10, 5, 14, 5, 14, 7);
                polyline(7, 7, 8, 19, 16, 19, 17, 7);
                break;
            case CLOSE:
                line(6, 6, 18, 18);
                line(18, 6, 6, 18);
                break;
        }
    }

    private void tray() {
        polyline(5, 15, 5, 19, 19, 19, 19, 15);
    }

    private void folder() {
        polyline(3, 7, 9, 7, 11, 9, 21, 9, 21, 19, 3, 19, 3, 7);
    }

    private void line(float x0, float y0, float x1, float y1) {
        polyline(x0, y0, x1, y1);
    }

    private void polyline(float... points) {
        path.moveTo(points[0], points[1]);
        for (int i = 2; i < points.length; i += 2) {
            path.lineTo(points[i], points[i + 1]);
        }
    }

    @Override
    public void draw(Canvas canvas) {
        Rect bounds = getBounds();
        int save = canvas.save();
        canvas.translate(bounds.left, bounds.top);
        canvas.scale(bounds.width() / GRID, bounds.height() / GRID);
        canvas.drawPath(path, paint);
        canvas.restoreToCount(save);
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

    @Override
    public int getIntrinsicWidth() {
        return -1;
    }

    @Override
    public int getIntrinsicHeight() {
        return -1;
    }
}
