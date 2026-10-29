package com.jjongjjongs.minimobile;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.net.Uri;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 키패드 디자인 → 내 사진: a picture of the player's own, laid on the keypad
 * one of four ways. The keys keep their places, sizes and shapes; the picture
 * only fills them or lies under them.
 *
 * <ul>
 *   <li>{@link #BACKDROP}: the picture under the whole keypad, the keys frosted
 *       glass over it.
 *   <li>{@link #STENCIL}: a dark tray, the picture showing only through the
 *       keys - one picture across all of them.
 *   <li>{@link #TINT}: the picture blurred under the keypad, the keys coloured
 *       from it.
 *   <li>{@link #DISC}: 다크 글래스 as it is, the picture on the direction
 *       disc alone.
 * </ul>
 *
 * <p>The picture is copied into the app's own files when it is chosen, so
 * it stays when the original is deleted from the gallery. One is kept; a
 * newly chosen one waits beside it until 적용 keeps it in its place.
 */
final class KeypadPhoto {
    static final int BACKDROP = 0;
    static final int STENCIL = 1;
    static final int TINT = 2;
    static final int DISC = 3;

    static final String[] MODES = {"배경에 깔고 유리 키", "키 모양으로 찍어내기", "사진에서 색 뽑기", "방향 원판에만"};

    private static final String PREFS = "mini_keypad_photo";
    private static final String FILE = "keypad_photo.jpg";
    private static final String PENDING = "keypad_photo_new.jpg";
    /** The long side a chosen picture is brought down to; the keypad is never wider than the phone. */
    private static final int LONGEST = 1600;

    int mode = BACKDROP;
    /** How much the picture is darkened, 0 to 80 percent. */
    int dim = 35;
    /** How soft the picture is drawn, 0 to 20 dp. */
    int blur = 2;
    /**
     * How clear the glass keys of {@link #BACKDROP} and the tinted keys of
     * {@link #TINT} are, 0 to 100 percent. Fully clear, a glass key is its
     * outline and label over the picture, as sharp as the tray's.
     */
    int glass = 45;
    /**
     * Which part of the picture shows when it is taller than the keypad, 0
     * (its top) to 100 (its bottom). Portrait and landscape keypads are
     * different shapes, so each keeps its own.
     */
    int portraitFocus = 50;
    int landscapeFocus = 50;

    Bitmap picture;
    /** Whether {@link #picture} is a newly chosen one, not yet kept. */
    boolean pending;

    private final Map<String, Bitmap> layers = new LinkedHashMap<String, Bitmap>(8, 0.75f, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<String, Bitmap> eldest) {
            return size() > 8;
        }
    };
    private int[] palette;

    /** The settings kept, and the picture kept with them if there is one. */
    static KeypadPhoto load(Context context) {
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        KeypadPhoto photo = new KeypadPhoto();
        photo.mode = clamp(prefs.getInt("mode", BACKDROP), 0, MODES.length - 1);
        photo.dim = clamp(prefs.getInt("dim", photo.dim), 0, 80);
        photo.blur = clamp(prefs.getInt("blur", photo.blur), 0, 20);
        photo.glass = clamp(prefs.getInt("glass", photo.glass), 0, 100);
        photo.portraitFocus = clamp(prefs.getInt("portraitFocus", photo.portraitFocus), 0, 100);
        photo.landscapeFocus = clamp(prefs.getInt("landscapeFocus", photo.landscapeFocus), 0, 100);
        File file = new File(context.getFilesDir(), FILE);
        if (file.isFile()) {
            photo.picture = BitmapFactory.decodeFile(file.getPath());
        }
        return photo;
    }

    /** Keeps these settings, and a newly chosen picture in place of the old one. */
    void save(Context context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putInt("mode", mode)
                .putInt("dim", dim)
                .putInt("blur", blur)
                .putInt("glass", glass)
                .putInt("portraitFocus", portraitFocus)
                .putInt("landscapeFocus", landscapeFocus)
                .apply();
        if (pending) {
            File chosen = new File(context.getFilesDir(), PENDING);
            File kept = new File(context.getFilesDir(), FILE);
            if (chosen.isFile() && (!kept.exists() || kept.delete()) && chosen.renameTo(kept)) {
                pending = false;
            }
        }
    }

    /** Drops a chosen picture that was never kept. */
    static void discardPending(Context context) {
        File chosen = new File(context.getFilesDir(), PENDING);
        if (chosen.exists()) {
            chosen.delete();
        }
    }

    /** The same settings and picture, to change without touching these. */
    KeypadPhoto copy() {
        KeypadPhoto copy = new KeypadPhoto();
        copy.mode = mode;
        copy.dim = dim;
        copy.blur = blur;
        copy.glass = glass;
        copy.portraitFocus = portraitFocus;
        copy.landscapeFocus = landscapeFocus;
        copy.picture = picture;
        copy.pending = pending;
        return copy;
    }

    /** Takes a newly chosen picture. */
    void setPicture(Bitmap bitmap) {
        picture = bitmap;
        pending = true;
        layers.clear();
        palette = null;
    }

    /**
     * Reads the picture at {@code uri}, upright and no bigger than the keypad
     * can use, and writes it beside the kept one until it is kept. Off the UI
     * thread: a camera picture takes a moment to decode.
     */
    static Bitmap importPicture(Context context, Uri uri) throws IOException {
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        try (InputStream in = context.getContentResolver().openInputStream(uri)) {
            BitmapFactory.decodeStream(in, null, bounds);
        }
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            throw new IOException("사진으로 읽을 수 없는 파일이에요");
        }
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inSampleSize = 1;
        while (Math.max(bounds.outWidth, bounds.outHeight) / (options.inSampleSize * 2) >= LONGEST) {
            options.inSampleSize *= 2;
        }
        Bitmap decoded;
        try (InputStream in = context.getContentResolver().openInputStream(uri)) {
            decoded = BitmapFactory.decodeStream(in, null, options);
        }
        if (decoded == null) {
            throw new IOException("사진으로 읽을 수 없는 파일이에요");
        }

        // A camera writes its pictures as the sensor saw them and says in
        // EXIF which way is up.
        int degrees = 0;
        try (InputStream in = context.getContentResolver().openInputStream(uri)) {
            if (in != null) {
                int orientation = new android.media.ExifInterface(in)
                        .getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, android.media.ExifInterface.ORIENTATION_NORMAL);
                degrees = orientation == android.media.ExifInterface.ORIENTATION_ROTATE_90 ? 90
                        : orientation == android.media.ExifInterface.ORIENTATION_ROTATE_180 ? 180
                        : orientation == android.media.ExifInterface.ORIENTATION_ROTATE_270 ? 270 : 0;
            }
        } catch (IOException | RuntimeException ignored) {
            // Not every picture has EXIF; one without is upright as it is.
        }

        float scale = Math.min(1f, (float) LONGEST / Math.max(decoded.getWidth(), decoded.getHeight()));
        if (degrees != 0 || scale < 1f) {
            Matrix matrix = new Matrix();
            matrix.postScale(scale, scale);
            matrix.postRotate(degrees);
            Bitmap turned = Bitmap.createBitmap(decoded, 0, 0, decoded.getWidth(), decoded.getHeight(), matrix, true);
            if (turned != decoded) {
                decoded.recycle();
            }
            decoded = turned;
        }

        try (OutputStream out = new FileOutputStream(new File(context.getFilesDir(), PENDING))) {
            decoded.compress(Bitmap.CompressFormat.JPEG, 90, out);
        }
        return decoded;
    }

    /**
     * The picture filling {@code width} by {@code height} - cropped to that
     * shape, with {@code focus} saying which part of a taller picture shows -
     * softened by {@code blur} pixels and darkened by {@code dim} (0 to 1).
     * Kept for the next frame, which nearly always asks for the same.
     */
    Bitmap layer(int width, int height, float blur, float dim, int focus) {
        if (picture == null || width <= 0 || height <= 0) {
            return null;
        }
        String key = width + "x" + height + "/" + Math.round(blur) + "/" + Math.round(dim * 100) + "/" + focus;
        Bitmap cached = layers.get(key);
        if (cached != null) {
            return cached;
        }

        Bitmap out = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(out);
        Paint paint = new Paint(Paint.FILTER_BITMAP_FLAG);
        float scale = Math.max((float) width / picture.getWidth(), (float) height / picture.getHeight());
        float drawnWidth = picture.getWidth() * scale;
        float drawnHeight = picture.getHeight() * scale;
        Matrix matrix = new Matrix();
        matrix.postScale(scale, scale);
        matrix.postTranslate((width - drawnWidth) / 2f, (height - drawnHeight) * focus / 100f);
        canvas.drawBitmap(picture, matrix, paint);

        if (blur >= 1f) {
            out = soften(out, blur);
            canvas = new Canvas(out);
        }
        if (dim > 0f) {
            canvas.drawColor(Color.argb(Math.round(Math.min(1f, dim) * 255), 0, 0, 0));
        }
        layers.put(key, out);
        return out;
    }

    /**
     * A blur by halving and growing back: each halving averages two by two,
     * so a few of them make a box blur about {@code radius} wide, and the
     * filtered stretch back up smooths the blocks away.
     */
    private static Bitmap soften(Bitmap source, float radius) {
        int width = source.getWidth();
        int height = source.getHeight();
        Bitmap small = source;
        float reach = 1f;
        while (reach * 2f <= radius && small.getWidth() > 4 && small.getHeight() > 4) {
            Bitmap half = Bitmap.createScaledBitmap(small, Math.max(1, small.getWidth() / 2), Math.max(1, small.getHeight() / 2), true);
            if (small != source) {
                small.recycle();
            }
            small = half;
            reach *= 2f;
        }
        if (small == source) {
            small = Bitmap.createScaledBitmap(source, Math.max(1, Math.round(width / 1.6f)), Math.max(1, Math.round(height / 1.6f)), true);
        }
        Bitmap grown = Bitmap.createScaledBitmap(small, width, height, true);
        small.recycle();
        source.recycle();
        return grown;
    }

    /**
     * Two colours from the picture: its average, and its most vivid one that
     * is not dark - what {@link #TINT} paints the keys and their accents with.
     */
    int[] palette() {
        if (palette != null) {
            return palette;
        }
        if (picture == null) {
            return palette = new int[]{Color.rgb(40, 44, 56), Color.rgb(84, 199, 214)};
        }
        Bitmap small = Bitmap.createScaledBitmap(picture, 32, 20, true);
        long red = 0;
        long green = 0;
        long blue = 0;
        float best = -1f;
        int vivid = Color.rgb(84, 199, 214);
        float[] hsv = new float[3];
        for (int y = 0; y < small.getHeight(); y++) {
            for (int x = 0; x < small.getWidth(); x++) {
                int pixel = small.getPixel(x, y);
                red += Color.red(pixel);
                green += Color.green(pixel);
                blue += Color.blue(pixel);
                Color.colorToHSV(pixel, hsv);
                float score = hsv[1] * hsv[2] * (hsv[2] > 0.35f ? 1f : 0.2f);
                if (score > best) {
                    best = score;
                    vivid = pixel | 0xff000000;
                }
            }
        }
        int count = small.getWidth() * small.getHeight();
        small.recycle();
        return palette = new int[]{Color.rgb((int) (red / count), (int) (green / count), (int) (blue / count)), vivid};
    }

    /** {@code from} moved {@code amount} (0 to 1) of the way to {@code to}. */
    static int mix(int from, int to, float amount) {
        return Color.rgb(Math.round(Color.red(from) + (Color.red(to) - Color.red(from)) * amount),
                Math.round(Color.green(from) + (Color.green(to) - Color.green(from)) * amount),
                Math.round(Color.blue(from) + (Color.blue(to) - Color.blue(from)) * amount));
    }

    static int withAlpha(int color, float alpha) {
        return (Math.round(Math.max(0f, Math.min(1f, alpha)) * 255) << 24) | (color & 0xffffff);
    }

    /** How light a colour reads, 0 to 1. */
    static float luminance(int color) {
        return (0.2126f * Color.red(color) + 0.7152f * Color.green(color) + 0.0722f * Color.blue(color)) / 255f;
    }

    private static int clamp(int value, int low, int high) {
        return Math.max(low, Math.min(high, value));
    }
}
