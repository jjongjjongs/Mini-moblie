package com.jjongjjongs.minimobile;

import android.content.ContentResolver;
import android.content.ContentUris;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.provider.MediaStore;

import java.io.File;
import java.io.FileOutputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Puts a file in the Downloads folder, which is the one place the app can
 * write that the person using it can also reach.
 *
 * <p>From Android 10 that is a MediaStore insert and needs no permission.
 * Before it, Downloads is a plain directory and writing to it does - see
 * {@code MainActivity.withDownloadPermission}.
 */
final class Downloads {
    /** The folder exported save zips are kept in, under Downloads. */
    static final String SAVES_DIR = "Mini Mobile/세이브";

    private Downloads() {
    }

    /** Whether {@link #write} will need the storage permission first. */
    static boolean needsPermission() {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.Q;
    }

    /**
     * Like {@link #write}, but into {@code Download/<subDir>/}, replacing any
     * file of the same name that is already there (so re-exporting a save
     * overwrites rather than piling up "(1)" copies).
     */
    static void writeInto(Context context, String subDir, String name, String mimeType, byte[] contents) throws Exception {
        if (needsPermission()) {
            File dir = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), subDir);
            if (!dir.isDirectory() && !dir.mkdirs()) {
                throw new IllegalStateException("폴더를 만들 수 없습니다.");
            }
            try (FileOutputStream output = new FileOutputStream(new File(dir, name))) {
                output.write(contents);
            }
            return;
        }

        ContentResolver resolver = context.getContentResolver();
        String relPath = Environment.DIRECTORY_DOWNLOADS + "/" + subDir + "/";

        resolver.delete(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                MediaStore.Downloads.RELATIVE_PATH + "=? AND " + MediaStore.Downloads.DISPLAY_NAME + "=?",
                new String[]{relPath, name});

        ContentValues values = new ContentValues();
        values.put(MediaStore.Downloads.DISPLAY_NAME, name);
        values.put(MediaStore.Downloads.MIME_TYPE, mimeType);
        values.put(MediaStore.Downloads.RELATIVE_PATH, relPath);

        Uri target = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values);
        if (target == null) {
            throw new IllegalStateException("폴더에 쓸 수 없습니다.");
        }
        try (OutputStream output = resolver.openOutputStream(target)) {
            if (output == null) {
                throw new IllegalStateException("폴더에 쓸 수 없습니다.");
            }
            output.write(contents);
        } catch (Exception e) {
            resolver.delete(target, null, null);
            throw e;
        }
    }

    /** One exported save zip found in the saves folder. */
    static final class SaveFile {
        final String name;
        final Uri uri;
        /** When it was last written, in milliseconds; 0 when not known. */
        final long modified;

        SaveFile(String name, Uri uri, long modified) {
            this.name = name;
            this.uri = uri;
            this.modified = modified;
        }
    }

    /**
     * {@code Download/Mini Mobile/세이브/} as the system file picker names it,
     * for {@code EXTRA_INITIAL_URI}: the folder an exported save is in, and so
     * where a save being imported most likely is.
     *
     * <p>The picker takes it only as a hint. A folder that is not there yet -
     * nothing has been exported - or a picker that is not the system's own
     * opens where it would have anyway.
     */
    static Uri savesFolderUri() {
        return DocumentsContract.buildDocumentUri(
                "com.android.externalstorage.documents",
                "primary:" + Environment.DIRECTORY_DOWNLOADS + "/" + SAVES_DIR);
    }

    /** The save zips in {@code Download/Mini Mobile/세이브/}, by name. */
    static List<SaveFile> listSaves(Context context) {
        List<SaveFile> saves = new ArrayList<>();

        if (needsPermission()) {
            File dir = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), SAVES_DIR);
            File[] files = dir.listFiles();
            if (files != null) {
                for (File file : files) {
                    if (file.isFile() && file.getName().toLowerCase(Locale.ROOT).endsWith(".zip")) {
                        saves.add(new SaveFile(file.getName(), Uri.fromFile(file), file.lastModified()));
                    }
                }
            }
            saves.sort((a, b) -> a.name.compareToIgnoreCase(b.name));
            return saves;
        }

        String relPath = Environment.DIRECTORY_DOWNLOADS + "/" + SAVES_DIR + "/";
        String[] projection = {MediaStore.Downloads._ID, MediaStore.Downloads.DISPLAY_NAME, MediaStore.Downloads.DATE_MODIFIED};
        try (Cursor cursor = context.getContentResolver().query(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                projection,
                MediaStore.Downloads.RELATIVE_PATH + "=?",
                new String[]{relPath},
                MediaStore.Downloads.DISPLAY_NAME + " ASC")) {
            if (cursor != null) {
                int idColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads._ID);
                int nameColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads.DISPLAY_NAME);
                int modifiedColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads.DATE_MODIFIED);
                while (cursor.moveToNext()) {
                    String name = cursor.getString(nameColumn);
                    if (name == null || !name.toLowerCase(Locale.ROOT).endsWith(".zip")) {
                        continue;
                    }
                    Uri uri = ContentUris.withAppendedId(MediaStore.Downloads.EXTERNAL_CONTENT_URI, cursor.getLong(idColumn));
                    // MediaStore keeps the date in seconds.
                    saves.add(new SaveFile(name, uri, cursor.getLong(modifiedColumn) * 1000L));
                }
            }
        }

        return saves;
    }

    static void write(Context context, String name, String mimeType, byte[] contents) throws Exception {
        if (needsPermission()) {
            File downloads = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS);
            if (!downloads.exists() && !downloads.mkdirs()) {
                throw new IllegalStateException("다운로드 폴더를 열 수 없습니다.");
            }

            try (FileOutputStream output = new FileOutputStream(new File(downloads, name))) {
                output.write(contents);
            }
            return;
        }

        ContentValues values = new ContentValues();
        values.put(MediaStore.Downloads.DISPLAY_NAME, name);
        values.put(MediaStore.Downloads.MIME_TYPE, mimeType);

        ContentResolver resolver = context.getContentResolver();
        Uri target = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values);
        if (target == null) {
            throw new IllegalStateException("다운로드 폴더에 쓸 수 없습니다.");
        }

        try (OutputStream output = resolver.openOutputStream(target)) {
            if (output == null) {
                throw new IllegalStateException("다운로드 폴더에 쓸 수 없습니다.");
            }
            output.write(contents);
        } catch (Exception e) {
            // A half-written entry would show up in Downloads as a broken file.
            resolver.delete(target, null, null);
            throw e;
        }
    }

    /** Trims a title down to something a filesystem will take. */
    static String safeName(String title) {
        String trimmed = title.replaceAll("[^A-Za-z0-9가-힣._ -]", "_").trim();

        if (trimmed.isEmpty()) {
            return "game";
        }

        return trimmed.length() > 60 ? trimmed.substring(0, 60).trim() : trimmed;
    }
}
