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
        /** A keypad: the on-screen keys shown or put away. */
        KEYPAD,
        /** A handset with a turning arc: which way up the player is. */
        ROTATE,
        /** A pointing finger: touches on the game screen. */
        TOUCH,
        /** A framed hill: how the screen is drawn. */
        QUALITY,
        /** Corner arrows: how big the screen is drawn. */
        SCALE,
        /** Three sliders: 조작 설정. */
        CONTROLS,
        /** Arrows four ways: keys moved and sized. */
        MOVE,
        /** An eye: keys shown or hidden. */
        EYE,
        /** A bolt: 연사. */
        BOLT,
        /** A floppy disk: layouts saved and backed up. */
        SAVE,
        /** A gamepad. */
        GAMEPAD,
        /** A framed hill, for 내 사진. */
        PHOTO,
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
            case KEYPAD:
                rect(3, 7, 21, 17);
                line(7, 11, 8, 11);
                line(11, 11, 12, 11);
                line(15, 11, 16, 11);
                line(8, 14, 16, 14);
                break;
            case ROTATE:
                rect(8, 3, 16, 21);
                path.arcTo(new RectF(2, 2, 22, 22), 200f, 40f, true);
                path.arcTo(new RectF(2, 2, 22, 22), 20f, 40f, true);
                break;
            case TOUCH:
                polyline(9, 12, 9, 5);
                path.arcTo(new RectF(9, 3, 13, 7), 180f, 180f, false);
                polyline(13, 5, 13, 11);
                polyline(13, 10, 17, 10, 17, 15);
                path.arcTo(new RectF(5, 9, 17, 21), 0f, 180f, false);
                polyline(5, 15, 5, 13, 9, 13);
                break;
            case QUALITY:
            case PHOTO:
                rect(4, 5, 20, 19);
                polyline(7, 16, 11, 12, 13, 14, 16, 10, 18, 13);
                break;
            case SCALE:
                polyline(10, 4, 4, 4, 4, 10);
                line(4, 4, 10, 10);
                polyline(14, 20, 20, 20, 20, 14);
                line(20, 20, 14, 14);
                break;
            case CONTROLS:
                line(4, 6, 20, 6);
                line(4, 12, 20, 12);
                line(4, 18, 20, 18);
                line(8, 4, 8, 8);
                line(16, 10, 16, 14);
                line(10, 16, 10, 20);
                break;
            case MOVE:
                line(12, 3, 12, 21);
                line(3, 12, 21, 12);
                polyline(9, 6, 12, 3, 15, 6);
                polyline(9, 18, 12, 21, 15, 18);
                polyline(6, 9, 3, 12, 6, 15);
                polyline(18, 9, 21, 12, 18, 15);
                break;
            case EYE:
                path.arcTo(new RectF(2, 5, 22, 25), 205f, 130f, true);
                path.arcTo(new RectF(2, -1, 22, 19), 25f, 130f, true);
                path.addCircle(12, 12, 3, Path.Direction.CW);
                break;
            case BOLT:
                polyline(13, 2, 4, 14, 11, 14, 10, 22, 19, 10, 12, 10, 13, 2);
                break;
            case SAVE:
                polyline(5, 4, 16, 4, 19, 7, 19, 20, 5, 20, 5, 4);
                polyline(8, 4, 8, 9, 15, 9, 15, 4);
                polyline(8, 20, 8, 14, 16, 14, 16, 20);
                break;
            case GAMEPAD:
                path.addRoundRect(new RectF(2, 7, 22, 18), 5.5f, 5.5f, Path.Direction.CW);
                line(7, 10.5f, 7, 14.5f);
                line(5, 12.5f, 9, 12.5f);
                line(15.5f, 11, 15.6f, 11);
                line(18, 13.5f, 18.1f, 13.5f);
                break;
        }
    }

    private void rect(float left, float top, float right, float bottom) {
        path.addRoundRect(new RectF(left, top, right, bottom), 2f, 2f, Path.Direction.CW);
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
