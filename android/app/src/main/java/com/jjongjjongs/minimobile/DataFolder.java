package com.jjongjjongs.minimobile;

import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.MediaStore;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * A per-game data folder the person can reach in their file manager:
 * {@code Download/Mini Mobile/<game>/}. The emulator's saves live in the app's
 * private directory, which nothing else can open, so this mirrors them out as
 * individual files ({@link #export}) and reads them back in ({@link #restore}).
 *
 * <p>Only save data is mirrored - the record stores under {@code db/<id>} and
 * the files a title wrote under {@code fs/<id>}, the same set {@link SaveExporter}
 * zips - laid out under the game's folder with that {@code db|fs/<id>} split kept
 * so a restore can route each file back by its path alone, exactly as
 * {@link SaveImporter} does for a zip.
 *
 * <p>It is not a live mount: Android's scoped storage lets the app write and
 * re-read its own folder, but the running game only ever touches its private
 * copy, so the folder and the game are kept in step by export/restore.
 */
final class DataFolder {
    private static final int CHUNK = 32768;
    static final String ROOT = "Mini Mobile";

    private static final String README_NAME = "읽어주세요.txt";
    private static final String README_TEXT =
            "이 폴더는 Mini Mobile 게임의 저장 데이터입니다.\n\n"
                    + "- db/ : 게임의 기록 저장소, fs/ : 게임이 만든 파일\n"
                    + "- 백업하려면 이 폴더를 다른 곳에 복사하세요.\n"
                    + "- 여기 파일을 고치거나 되돌린 뒤에는, 게임 메뉴의\n"
                    + "  '데이터 폴더에서 불러오기'를 눌러야 게임에 적용됩니다.\n"
                    + "- '데이터 폴더로 내보내기'를 다시 누르면 이 폴더는\n"
                    + "  현재 저장 데이터로 새로 덮어써집니다.\n";

    /** What an export or restore moved. */
    static final class Result {
        final int files;
        final String path;

        private Result(int files, String path) {
            this.files = files;
            this.path = path;
        }
    }

    private DataFolder() {
    }

    /** The folder name a game's data sits under, e.g. {@code Mini Mobile/지혜의검}. */
    static String subDir(String title) {
        return ROOT + "/" + Downloads.safeName(title);
    }

    /** A person-facing path for a toast. */
    static String displayPath(String title) {
        return "다운로드/" + subDir(title);
    }

    /**
     * Mirrors a game's current save data out to {@code Download/Mini Mobile/<game>/},
     * replacing whatever was there so the folder is a fresh snapshot.
     *
     * @return what was written, or {@code null} if the title has saved nothing
     */
    static Result export(Context context, File archive, String title) throws Exception {
        List<File> roots = SaveExporter.roots(context, archive);
        if (roots.isEmpty()) {
            return null;
        }

        String base = subDir(title);
        clear(context, base);

        int files = 0;
        for (File root : roots) {
            // "db/<id>" or "fs/<id>", the same prefix the zip export uses.
            String prefix = root.getParentFile().getName() + "/" + root.getName();
            files += writeTree(context, base, root, prefix);
        }

        writeFile(context, base, "", README_NAME, "text/plain", README_TEXT.getBytes("UTF-8"));

        return new Result(files, displayPath(title));
    }

    /**
     * Reads {@code Download/Mini Mobile/<game>/} back into the app's private
     * {@code runtime} directory, overwriting what is there. Only {@code db/} and
     * {@code fs/} entries are routed; the README and anything else is ignored.
     *
     * @return what was restored, or {@code null} if the folder holds no save data
     */
    static Result restore(Context context, String title) throws Exception {
        File runtime = new File(context.getFilesDir(), "runtime");
        String runtimePath = runtime.getCanonicalPath() + File.separator;
        String base = subDir(title);

        int files = 0;
        for (Entry entry : list(context, base)) {
            String rel = entry.relativePath; // e.g. "db/<id>/save.db"
            if (!(rel.startsWith("db/") || rel.startsWith("fs/"))) {
                continue;
            }

            File target = new File(runtime, rel);
            // Untrusted names: refuse anything that would escape runtime.
            if (!target.getCanonicalPath().startsWith(runtimePath)) {
                throw new IllegalStateException("폴더에 잘못된 경로가 들어 있습니다.");
            }

            File parent = target.getParentFile();
            if (parent != null && !parent.isDirectory() && !parent.mkdirs()) {
                throw new IllegalStateException("폴더를 만들 수 없습니다: " + parent.getName());
            }

            byte[] bytes = entry.read(context);
            try (FileOutputStream output = new FileOutputStream(target)) {
                output.write(bytes);
            }
            files++;
        }

        if (files == 0) {
            return null;
        }

        return new Result(files, displayPath(title));
    }

    // --- writing ---------------------------------------------------------

    private static int writeTree(Context context, String base, File root, String prefix) throws Exception {
        File[] entries = root.listFiles();
        if (entries == null) {
            return 0;
        }

        int written = 0;
        for (File entry : entries) {
            if (entry.isDirectory()) {
                written += writeTree(context, base, entry, prefix + "/" + entry.getName());
                continue;
            }

            byte[] bytes;
            try (InputStream input = new FileInputStream(entry); ByteArrayOutputStream buffer = new ByteArrayOutputStream()) {
                byte[] chunk = new byte[CHUNK];
                int read;
                while ((read = input.read(chunk)) >= 0) {
                    buffer.write(chunk, 0, read);
                }
                bytes = buffer.toByteArray();
            }

            // `prefix` is the directory these files sit in ("db/<id>[/...]");
            // the file keeps its own name inside it.
            writeFile(context, base, prefix, entry.getName(), "application/octet-stream", bytes);
            written++;
        }

        return written;
    }

    /** Writes one file at {@code Download/<base>/<subDir>/<name>}. */
    private static void writeFile(Context context, String base, String subDir, String name, String mime, byte[] bytes) throws Exception {
        String relDir = subDir.isEmpty() ? base : base + "/" + subDir;

        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            File dir = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), relDir);
            if (!dir.isDirectory() && !dir.mkdirs()) {
                throw new IllegalStateException("폴더를 만들 수 없습니다.");
            }
            try (FileOutputStream output = new FileOutputStream(new File(dir, name))) {
                output.write(bytes);
            }
            return;
        }

        ContentResolver resolver = context.getContentResolver();
        ContentValues values = new ContentValues();
        values.put(MediaStore.Downloads.DISPLAY_NAME, name);
        values.put(MediaStore.Downloads.MIME_TYPE, mime);
        values.put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/" + relDir + "/");

        Uri target = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values);
        if (target == null) {
            throw new IllegalStateException("폴더에 쓸 수 없습니다.");
        }
        try (OutputStream output = resolver.openOutputStream(target)) {
            if (output == null) {
                throw new IllegalStateException("폴더에 쓸 수 없습니다.");
            }
            output.write(bytes);
        } catch (Exception e) {
            resolver.delete(target, null, null);
            throw e;
        }
    }

    /** Removes everything already under {@code Download/<base>/}. */
    private static void clear(Context context, String base) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            deleteRecursively(new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), base));
            return;
        }

        String path = Environment.DIRECTORY_DOWNLOADS + "/" + base + "/";
        try {
            context.getContentResolver().delete(
                    MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                    MediaStore.Downloads.RELATIVE_PATH + " LIKE ?",
                    new String[]{path + "%"});
        } catch (Exception ignored) {
            // A folder that is not there yet is nothing to clear.
        }
    }

    private static void deleteRecursively(File file) {
        if (file == null || !file.exists()) {
            return;
        }
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) {
                deleteRecursively(child);
            }
        }
        file.delete();
    }

    // --- reading ---------------------------------------------------------

    /** One file found under the game folder, with its path relative to it. */
    private static final class Entry {
        final String relativePath;
        final Uri uri;   // API 29+
        final File file; // pre-29

        Entry(String relativePath, Uri uri, File file) {
            this.relativePath = relativePath;
            this.uri = uri;
            this.file = file;
        }

        byte[] read(Context context) throws Exception {
            try (InputStream input = uri != null ? context.getContentResolver().openInputStream(uri) : new FileInputStream(file);
                    ByteArrayOutputStream buffer = new ByteArrayOutputStream()) {
                if (input == null) {
                    throw new IllegalStateException("파일을 열 수 없습니다: " + relativePath);
                }
                byte[] chunk = new byte[CHUNK];
                int read;
                while ((read = input.read(chunk)) >= 0) {
                    buffer.write(chunk, 0, read);
                }
                return buffer.toByteArray();
            }
        }
    }

    private static List<Entry> list(Context context, String base) {
        List<Entry> entries = new ArrayList<>();

        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            File root = new File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), base);
            listFiles(root, "", entries);
            return entries;
        }

        String basePath = Environment.DIRECTORY_DOWNLOADS + "/" + base + "/";
        String[] projection = {MediaStore.Downloads._ID, MediaStore.Downloads.DISPLAY_NAME, MediaStore.Downloads.RELATIVE_PATH};
        try (Cursor cursor = context.getContentResolver().query(
                MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                projection,
                MediaStore.Downloads.RELATIVE_PATH + " LIKE ?",
                new String[]{basePath + "%"},
                null)) {
            if (cursor == null) {
                return entries;
            }
            int idColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads._ID);
            int nameColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads.DISPLAY_NAME);
            int pathColumn = cursor.getColumnIndexOrThrow(MediaStore.Downloads.RELATIVE_PATH);
            while (cursor.moveToNext()) {
                String relPath = cursor.getString(pathColumn); // ".../Mini Mobile/<game>/db/<id>/"
                String name = cursor.getString(nameColumn);
                if (relPath == null || name == null || !relPath.startsWith(basePath)) {
                    continue;
                }
                String subDir = relPath.substring(basePath.length()); // "db/<id>/" or ""
                String relative = subDir + name;
                Uri uri = android.content.ContentUris.withAppendedId(
                        MediaStore.Downloads.EXTERNAL_CONTENT_URI, cursor.getLong(idColumn));
                entries.add(new Entry(relative, uri, null));
            }
        }

        return entries;
    }

    private static void listFiles(File dir, String prefix, List<Entry> out) {
        File[] children = dir.listFiles();
        if (children == null) {
            return;
        }
        for (File child : children) {
            if (child.isDirectory()) {
                listFiles(child, prefix + child.getName() + "/", out);
            } else {
                out.add(new Entry(prefix + child.getName(), null, child));
            }
        }
    }
}
