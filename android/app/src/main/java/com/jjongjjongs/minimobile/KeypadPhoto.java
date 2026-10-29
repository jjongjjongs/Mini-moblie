package com.jjongjjongs.minimobile;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.PorterDuff;
import android.graphics.Rect;
import android.graphics.RectF;
import android.net.Uri;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 키패드 디자인 → 내 사진: pictures of the player's own, laid on the keypad
 * one of four ways. The keys keep their places, sizes and shapes; the
 * pictures only fill them or lie under them.
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
 * <p>Upright and on its side the keypad is a different shape - on its side
 * it is two bands either side of the game screen - so each keeps a
 * {@link Face} of its own: its pictures, the way they are laid on and how.
 * A face holds one picture across the keypad, or one for each half: the
 * left (the directions) and the right (the numbers). On its side one picture
 * is stretched across both bands as though the screen were not between them.
 *
 * <p>Pictures are copied into the app's own files when they are chosen, so
 * they stay when the originals are deleted from the gallery. A newly chosen
 * one waits beside the kept one until 적용 keeps it in its place.
 */
final class KeypadPhoto {
    static final int BACKDROP = 0;
    static final int STENCIL = 1;
    static final int TINT = 2;
    static final int DISC = 3;

    static final String[] MODES = {"배경에 깔고 유리 키", "키 모양으로 찍어내기", "사진에서 색 뽑기", "방향 원판에만"};

    /** Which picture of a face: the one across the keypad, or one half's. */
    static final int WHOLE = 0;
    static final int LEFT = 1;
    static final int RIGHT = 2;

    private static final String PREFS = "mini_keypad_photo";
    /** The long side a chosen picture is brought down to; the keypad is never wider than the phone. */
    private static final int LONGEST = 1280;

    /** One picture and how it sits in its part of the keypad. */
    static final class Slot {
        Bitmap picture;
        /** Quarter turns clockwise, 0 to 3. */
        int turns;
        /**
         * Which part of a picture bigger than its place shows, 0 to 100 each
         * way: 0 its left or top edge, 100 its right or bottom.
         */
        int focusX = 50;
        int focusY = 50;
        /** How far in it is drawn, 100 (just filling its place) to 300 percent. */
        int zoom = 100;
        /** Whether {@link #picture} is newly chosen, not yet kept. */
        boolean pending;
        /** Whether a kept picture was taken out, to be deleted when kept. */
        boolean removed;

        private Bitmap turned;
        private int turnedFor = -1;

        Slot copy() {
            Slot copy = new Slot();
            copy.picture = picture;
            copy.turns = turns;
            copy.focusX = focusX;
            copy.focusY = focusY;
            copy.zoom = zoom;
            copy.pending = pending;
            copy.removed = removed;
            copy.turned = turned;
            copy.turnedFor = turnedFor;
            return copy;
        }

        /** The picture as turned. */
        Bitmap upright() {
            if (picture == null || turns % 4 == 0) {
                return picture;
            }
            if (turned == null || turnedFor != turns) {
                Matrix matrix = new Matrix();
                matrix.postRotate(90 * turns);
                turned = Bitmap.createBitmap(picture, 0, 0, picture.getWidth(), picture.getHeight(), matrix, true);
                turnedFor = turns;
            }
            return turned;
        }

        void setPicture(Bitmap bitmap) {
            picture = bitmap;
            turned = null;
            turnedFor = -1;
            turns = 0;
            focusX = 50;
            focusY = 50;
            zoom = 100;
            pending = true;
            removed = false;
        }

        void clear() {
            picture = null;
            turned = null;
            pending = false;
            removed = true;
        }

        /**
         * The picture filling {@code width} by {@code height}: turned, scaled
         * to cover it and {@link #zoom} further, and placed by the focus.
         */
        Bitmap place(int width, int height) {
            Bitmap source = upright();
            Bitmap out = Bitmap.createBitmap(Math.max(1, width), Math.max(1, height), Bitmap.Config.ARGB_8888);
            if (source == null) {
                return out;
            }
            float scale = Math.max((float) width / source.getWidth(), (float) height / source.getHeight()) * zoom / 100f;
            float drawnWidth = source.getWidth() * scale;
            float drawnHeight = source.getHeight() * scale;
            Matrix matrix = new Matrix();
            matrix.postScale(scale, scale);
            matrix.postTranslate((width - drawnWidth) * focusX / 100f, (height - drawnHeight) * focusY / 100f);
            new Canvas(out).drawBitmap(source, matrix, new Paint(Paint.FILTER_BITMAP_FLAG));
            return out;
        }
    }

    /** Everything 내 사진 draws one way up with. */
    static final class Face {
        int mode = BACKDROP;
        /** How much the pictures are darkened, 0 to 80 percent. */
        int dim = 35;
        /** How soft they are drawn, 0 to 20 dp. */
        int blur = 2;
        /**
         * How clear the glass keys of {@link #BACKDROP} and the tinted keys of
         * {@link #TINT} are, 0 to 100 percent. Fully clear, a glass key is its
         * outline and label over the picture, as sharp as the tray's.
         */
        int glass = 45;
        /** A picture for each half rather than one across the keypad. */
        boolean split;
        Slot[] slots = {new Slot(), new Slot(), new Slot()};

        /** Bumped whenever a picture or its placing changes, to tell cached layers apart. */
        private int stamp;
        private final Map<String, Bitmap> layers = new LinkedHashMap<String, Bitmap>(8, 0.75f, true) {
            @Override
            protected boolean removeEldestEntry(Map.Entry<String, Bitmap> eldest) {
                return size() > 8;
            }
        };
        private Bitmap paletteFrom;
        private int[] palette;

        Face copy() {
            Face copy = new Face();
            copy.mode = mode;
            copy.dim = dim;
            copy.blur = blur;
            copy.glass = glass;
            copy.split = split;
            for (int index = 0; index < slots.length; index++) {
                copy.slots[index] = slots[index].copy();
            }
            return copy;
        }

        /** Says a picture or its placing changed. */
        void changed() {
            stamp++;
            layers.clear();
        }

        boolean hasPicture() {
            return split ? slots[LEFT].picture != null || slots[RIGHT].picture != null : slots[WHOLE].picture != null;
        }

        /** The picture the direction disc takes in {@link #DISC}: the left half's, which it sits in. */
        Slot discSlot() {
            return split && slots[LEFT].picture != null ? slots[LEFT] : split ? slots[RIGHT] : slots[WHOLE];
        }

        /**
         * The pictures laid into {@code regions} of a {@code width} by
         * {@code height} keypad, softened by {@code blur} pixels and darkened
         * by {@code dim} (0 to 1); clear outside them. One region is the whole
         * keypad; two are its halves, or the bands either side of the screen.
         * Kept for the next frame, which nearly always asks for the same.
         */
        Bitmap composite(int width, int height, RectF[] regions, float blur, float dim) {
            if (width <= 0 || height <= 0 || !hasPicture()) {
                return null;
            }
            StringBuilder key = new StringBuilder().append(stamp).append('/').append(width).append('x').append(height)
                    .append('/').append(Math.round(blur)).append('/').append(Math.round(dim * 100));
            for (RectF region : regions) {
                key.append('/').append(Math.round(region.left)).append(',').append(Math.round(region.right));
            }
            Bitmap cached = layers.get(key.toString());
            if (cached != null) {
                return cached;
            }

            Bitmap out = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
            Canvas canvas = new Canvas(out);
            Paint paint = new Paint(Paint.FILTER_BITMAP_FLAG);
            if (!split) {
                if (regions.length == 1) {
                    canvas.drawBitmap(slots[WHOLE].place(width, height), 0, 0, paint);
                } else {
                    // One picture across both bands, the screen between them
                    // left out of it.
                    int left = Math.max(1, Math.round(regions[0].width()));
                    int right = Math.max(1, Math.round(regions[1].width()));
                    Bitmap across = slots[WHOLE].place(left + right, height);
                    canvas.drawBitmap(across, new Rect(0, 0, left, height), regions[0], paint);
                    canvas.drawBitmap(across, new Rect(left, 0, left + right, height), regions[1], paint);
                }
            } else {
                for (int half = 0; half < 2 && half < regions.length; half++) {
                    Slot slot = slots[LEFT + half];
                    if (slot.picture != null) {
                        RectF region = regions[half];
                        canvas.drawBitmap(slot.place(Math.max(1, Math.round(region.width())), Math.max(1, Math.round(region.height()))),
                                null, region, paint);
                    }
                }
            }
            if (blur >= 1f) {
                out = soften(out, blur);
            }
            if (dim > 0f) {
                // Only over the pictures: where there is none the tray shows.
                new Canvas(out).drawColor(Color.argb(Math.round(Math.min(1f, dim) * 255), 0, 0, 0), PorterDuff.Mode.SRC_ATOP);
            }
            layers.put(key.toString(), out);
            return out;
        }

        /** The direction disc's own picture, {@code size} across, for {@link #DISC}. */
        Bitmap disc(int size, float blur, float dim) {
            Slot slot = discSlot();
            if (size <= 0 || slot.picture == null) {
                return null;
            }
            String key = stamp + "/disc/" + size + "/" + Math.round(blur) + "/" + Math.round(dim * 100);
            Bitmap cached = layers.get(key);
            if (cached != null) {
                return cached;
            }
            Bitmap out = slot.place(size, size);
            if (blur >= 1f) {
                out = soften(out, blur);
            }
            if (dim > 0f) {
                new Canvas(out).drawColor(Color.argb(Math.round(Math.min(1f, dim) * 255), 0, 0, 0));
            }
            layers.put(key, out);
            return out;
        }

        /**
         * Two colours from {@code picture} - the face's own pictures as laid
         * on: their average, and their most vivid colour that is not dark -
         * what {@link #TINT} paints the keys and their accents with.
         */
        int[] palette(Bitmap picture) {
            if (picture == null) {
                return new int[]{Color.rgb(40, 44, 56), Color.rgb(84, 199, 214)};
            }
            if (palette != null && paletteFrom == picture) {
                return palette;
            }
            Bitmap small = Bitmap.createScaledBitmap(picture, 32, 20, true);
            long red = 0;
            long green = 0;
            long blue = 0;
            int count = 0;
            float best = -1f;
            int vivid = Color.rgb(84, 199, 214);
            float[] hsv = new float[3];
            for (int y = 0; y < small.getHeight(); y++) {
                for (int x = 0; x < small.getWidth(); x++) {
                    int pixel = small.getPixel(x, y);
                    // The clear parts are no part of any picture.
                    if (Color.alpha(pixel) < 128) {
                        continue;
                    }
                    red += Color.red(pixel);
                    green += Color.green(pixel);
                    blue += Color.blue(pixel);
                    count++;
                    Color.colorToHSV(pixel, hsv);
                    float score = hsv[1] * hsv[2] * (hsv[2] > 0.35f ? 1f : 0.2f);
                    if (score > best) {
                        best = score;
                        vivid = pixel | 0xff000000;
                    }
                }
            }
            small.recycle();
            count = Math.max(1, count);
            palette = new int[]{Color.rgb((int) (red / count), (int) (green / count), (int) (blue / count)), vivid};
            paletteFrom = picture;
            return palette;
        }
    }

    Face portrait = new Face();
    Face landscape = new Face();

    Face face(boolean onItsSide) {
        return onItsSide ? landscape : portrait;
    }

    /** Both faces, to change without touching these. */
    KeypadPhoto copy() {
        KeypadPhoto copy = new KeypadPhoto();
        copy.portrait = portrait.copy();
        copy.landscape = landscape.copy();
        return copy;
    }

    private static String prefix(boolean onItsSide) {
        return onItsSide ? "l." : "p.";
    }

    private static File file(Context context, boolean onItsSide, int slot, boolean pending) {
        return new File(context.getFilesDir(), "keypad_photo_" + (onItsSide ? "l" : "p") + slot + (pending ? "_new" : "") + ".jpg");
    }

    /** The faces kept, and their pictures. */
    static KeypadPhoto load(Context context) {
        SharedPreferences prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        migrate(context, prefs);
        KeypadPhoto photo = new KeypadPhoto();
        for (int side = 0; side < 2; side++) {
            boolean onItsSide = side == 1;
            Face face = photo.face(onItsSide);
            String p = prefix(onItsSide);
            face.mode = clamp(prefs.getInt(p + "mode", BACKDROP), 0, MODES.length - 1);
            face.dim = clamp(prefs.getInt(p + "dim", face.dim), 0, 80);
            face.blur = clamp(prefs.getInt(p + "blur", face.blur), 0, 20);
            face.glass = clamp(prefs.getInt(p + "glass", face.glass), 0, 100);
            face.split = prefs.getBoolean(p + "split", false);
            for (int index = 0; index < face.slots.length; index++) {
                Slot slot = face.slots[index];
                String s = p + index + ".";
                slot.turns = clamp(prefs.getInt(s + "turns", 0), 0, 3);
                slot.focusX = clamp(prefs.getInt(s + "x", 50), 0, 100);
                slot.focusY = clamp(prefs.getInt(s + "y", 50), 0, 100);
                slot.zoom = clamp(prefs.getInt(s + "zoom", 100), 100, 300);
                File kept = file(context, onItsSide, index, false);
                if (kept.isFile()) {
                    slot.picture = BitmapFactory.decodeFile(kept.getPath());
                }
            }
        }
        return photo;
    }

    /**
     * The first 내 사진 kept one picture and one set of settings for both ways
     * up; both faces start from them.
     */
    private static void migrate(Context context, SharedPreferences prefs) {
        if (prefs.contains("p.mode") || !prefs.contains("mode")) {
            return;
        }
        SharedPreferences.Editor editor = prefs.edit();
        for (int side = 0; side < 2; side++) {
            String p = prefix(side == 1);
            editor.putInt(p + "mode", prefs.getInt("mode", BACKDROP));
            editor.putInt(p + "dim", prefs.getInt("dim", 35));
            editor.putInt(p + "blur", prefs.getInt("blur", 2));
            editor.putInt(p + "glass", prefs.getInt("glass", 45));
            editor.putInt(p + "0.y", prefs.getInt(side == 1 ? "landscapeFocus" : "portraitFocus", 50));
        }
        editor.apply();
        File old = new File(context.getFilesDir(), "keypad_photo.jpg");
        if (old.isFile()) {
            copyFile(old, file(context, false, WHOLE, false));
            copyFile(old, file(context, true, WHOLE, false));
            old.delete();
        }
    }

    private static void copyFile(File from, File to) {
        try (InputStream in = new FileInputStream(from); OutputStream out = new FileOutputStream(to)) {
            byte[] buffer = new byte[64 * 1024];
            int read;
            while ((read = in.read(buffer)) > 0) {
                out.write(buffer, 0, read);
            }
        } catch (IOException ignored) {
            // Without the copy that way up simply starts with no picture.
        }
    }

    /** Keeps both faces, newly chosen pictures in place of the old ones and taken-out ones gone. */
    void save(Context context) {
        SharedPreferences.Editor editor = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit();
        for (int side = 0; side < 2; side++) {
            boolean onItsSide = side == 1;
            Face face = face(onItsSide);
            String p = prefix(onItsSide);
            editor.putInt(p + "mode", face.mode).putInt(p + "dim", face.dim).putInt(p + "blur", face.blur)
                    .putInt(p + "glass", face.glass).putBoolean(p + "split", face.split);
            for (int index = 0; index < face.slots.length; index++) {
                Slot slot = face.slots[index];
                String s = p + index + ".";
                editor.putInt(s + "turns", slot.turns).putInt(s + "x", slot.focusX).putInt(s + "y", slot.focusY)
                        .putInt(s + "zoom", slot.zoom);
                File kept = file(context, onItsSide, index, false);
                if (slot.pending) {
                    File chosen = file(context, onItsSide, index, true);
                    if (chosen.isFile() && (!kept.exists() || kept.delete()) && chosen.renameTo(kept)) {
                        slot.pending = false;
                    }
                } else if (slot.removed && slot.picture == null && kept.exists()) {
                    kept.delete();
                    slot.removed = false;
                }
            }
        }
        editor.apply();
    }

    /** Drops every chosen picture that was never kept. */
    static void discardPending(Context context) {
        for (int side = 0; side < 2; side++) {
            for (int index = 0; index < 3; index++) {
                File chosen = file(context, side == 1, index, true);
                if (chosen.exists()) {
                    chosen.delete();
                }
            }
        }
    }

    /**
     * Reads the picture at {@code uri}, upright and no bigger than the keypad
     * can use, and writes it beside the kept one for that face and slot until
     * it is kept. Off the UI thread: a camera picture takes a moment to decode.
     */
    static Bitmap importPicture(Context context, Uri uri, boolean onItsSide, int slot) throws IOException {
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

        try (OutputStream out = new FileOutputStream(file(context, onItsSide, slot, true))) {
            decoded.compress(Bitmap.CompressFormat.JPEG, 90, out);
        }
        return decoded;
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
