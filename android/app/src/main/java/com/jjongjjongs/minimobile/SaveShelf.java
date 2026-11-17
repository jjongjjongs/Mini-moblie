package com.jjongjjongs.minimobile;

import android.content.Context;
import android.net.Uri;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;

/**
 * The exported save zips, as the 세이브 불러오기 list shows them.
 *
 * <p>An export goes to {@code Download/Mini Mobile/세이브/}, where the player
 * can reach it, and a copy stays in the app's own storage. The copy is what
 * keeps the list whole: from Android 11 the app sees in Downloads only the
 * files this install wrote, so after the app is put on again the ones from
 * before vanish from a listing of Downloads although they are still there. The
 * list is both, one entry a name.
 */
final class SaveShelf {
    /** Under the app's files directory: the copies of every export. */
    private static final String DIR = "save_exports";

    /** What an export taken before an import is marked with. */
    static final String BEFORE_IMPORT = " (가져오기 전)";

    /** Bound on one read, so a bad file cannot fill memory. */
    private static final int CHUNK = 32768;

    /** One save zip. */
    static final class Entry {
        final String name;
        /** The app's own copy, or {@code null}. */
        final File file;
        /** The one in Downloads, or {@code null} when the app cannot see one. */
        final Uri download;
        final long modified;
        final long bytes;
        final int files;
        /** Whether it holds the game's own saves rather than another's. */
        final boolean ours;

        Entry(String name, File file, Uri download, long modified, long bytes, int files, boolean ours) {
            this.name = name;
            this.file = file;
            this.download = download;
            this.modified = modified;
            this.bytes = bytes;
            this.files = files;
            this.ours = ours;
        }

        /** Whether it is the copy taken by itself just before an import. */
        boolean beforeImport() {
            return name.endsWith(BEFORE_IMPORT + ".zip");
        }

        /**
         * The title the zip was named for: what comes before " 세이브", or the
         * whole name without {@code .zip} for one named some other way.
         */
        String title() {
            String bare = name.toLowerCase(Locale.ROOT).endsWith(".zip") ? name.substring(0, name.length() - 4) : name;
            int at = bare.lastIndexOf(" 세이브");
            return at > 0 ? bare.substring(0, at) : bare;
        }
    }

    private SaveShelf() {
    }

    static File dir(Context context) {
        File dir = new File(context.getFilesDir(), DIR);
        if (!dir.isDirectory()) {
            dir.mkdirs();
        }
        return dir;
    }

    /**
     * Writes an export under a name of its own - {@code <title> 세이브
     * <yyyy-MM-dd HH.mm><note>.zip}, with " (2)" and on when that minute has
     * one already - to the app's copies and to Downloads, and returns the name.
     */
    static String store(Context context, String title, String note, byte[] zip) throws Exception {
        String stamp = new SimpleDateFormat("yyyy-MM-dd HH.mm", Locale.ROOT).format(new Date());
        String base = Downloads.safeName(title) + " 세이브 " + stamp + note;

        Set<String> taken = new HashSet<>();
        String[] own = dir(context).list();
        if (own != null) {
            for (String name : own) {
                taken.add(name);
            }
        }
        for (Downloads.SaveFile save : Downloads.listSaves(context)) {
            taken.add(save.name);
        }
        String name = base + ".zip";
        for (int n = 2; taken.contains(name); n++) {
            name = base + " (" + n + ").zip";
        }

        File copy = new File(dir(context), name);
        try (FileOutputStream output = new FileOutputStream(copy)) {
            output.write(zip);
        }
        try {
            Downloads.writeInto(context, Downloads.SAVES_DIR, name, "application/zip", zip);
        } catch (Exception e) {
            copy.delete();
            throw e;
        }
        return name;
    }

    /**
     * Every save zip there is, the game's own first and then the rest, each
     * newest first. {@code ids} are the game's, from {@link SaveExporter#ids};
     * zips holding no saves are left out.
     */
    static List<Entry> list(Context context, String[] ids) {
        Map<String, File> copies = new LinkedHashMap<>();
        File[] own = dir(context).listFiles();
        if (own != null) {
            for (File file : own) {
                if (file.isFile() && file.getName().toLowerCase(Locale.ROOT).endsWith(".zip")) {
                    copies.put(file.getName(), file);
                }
            }
        }
        Map<String, Downloads.SaveFile> downloads = new LinkedHashMap<>();
        for (Downloads.SaveFile save : Downloads.listSaves(context)) {
            downloads.put(save.name, save);
        }

        Set<String> names = new HashSet<>(copies.keySet());
        names.addAll(downloads.keySet());

        Set<String> ourRoots = new HashSet<>();
        if (ids != null) {
            ourRoots.add("db/" + ids[0]);
            ourRoots.add("fs/" + ids[1]);
            ourRoots.add("fs/" + ids[0]);
        }

        List<Entry> entries = new ArrayList<>();
        for (String name : names) {
            File file = copies.get(name);
            Downloads.SaveFile download = downloads.get(name);
            long modified = file != null ? file.lastModified() : download.modified;

            int[] count = new int[1];
            long[] bytes = new long[1];
            Set<String> roots = new HashSet<>();
            try (InputStream input = open(context, file, download == null ? null : download.uri)) {
                read(input, roots, count, bytes);
            } catch (Exception e) {
                continue;
            }
            if (roots.isEmpty()) {
                continue;
            }
            boolean ours = false;
            for (String root : roots) {
                ours |= ourRoots.contains(root);
            }
            entries.add(new Entry(name, file, download == null ? null : download.uri, modified, bytes[0], count[0], ours));
        }

        entries.sort((a, b) -> {
            if (a.ours != b.ours) {
                return a.ours ? -1 : 1;
            }
            return Long.compare(b.modified, a.modified);
        });
        return entries;
    }

    static InputStream open(Context context, Entry entry) throws Exception {
        return open(context, entry.file, entry.download);
    }

    private static InputStream open(Context context, File file, Uri download) throws Exception {
        InputStream input = file != null ? new FileInputStream(file) : context.getContentResolver().openInputStream(download);
        if (input == null) {
            throw new IllegalStateException("파일을 열 수 없습니다.");
        }
        return input;
    }

    /** The {@code db/<id>}, {@code fs/<id>} roots a zip holds, its files and their size. */
    private static void read(InputStream input, Set<String> roots, int[] count, long[] bytes) throws Exception {
        byte[] chunk = new byte[CHUNK];
        try (ZipInputStream zip = new ZipInputStream(input)) {
            ZipEntry entry;
            while ((entry = zip.getNextEntry()) != null) {
                if (entry.isDirectory()) {
                    continue;
                }
                String[] parts = entry.getName().split("/", 3);
                if (parts.length < 3 || !(parts[0].equals("db") || parts[0].equals("fs")) || parts[1].isEmpty() || parts[2].isEmpty()) {
                    continue;
                }
                roots.add(parts[0] + "/" + parts[1]);
                count[0]++;
                long size = 0;
                int read;
                while ((read = zip.read(chunk)) >= 0) {
                    size += read;
                }
                bytes[0] += size;
            }
        }
    }

    /**
     * Removes the zip: the app's copy, and the one in Downloads where the app
     * may. False when the one in Downloads had to stay - from Android 10 a
     * file another install wrote is not the app's to delete.
     */
    static boolean delete(Context context, Entry entry) {
        if (entry.file != null) {
            entry.file.delete();
        }
        if (entry.download == null) {
            return true;
        }
        try {
            if ("file".equals(entry.download.getScheme())) {
                return new File(entry.download.getPath()).delete();
            }
            return context.getContentResolver().delete(entry.download, null, null) > 0;
        } catch (SecurityException e) {
            return false;
        }
    }

    /**
     * A {@code content://} address another app can read the zip at, for the
     * share sheet. The app's copy is what is handed out, made first when
     * there is only the one in Downloads.
     */
    static Uri shareUri(Context context, Entry entry) throws Exception {
        File file = entry.file;
        if (file == null) {
            file = new File(dir(context), entry.name);
            try (InputStream input = open(context, null, entry.download); FileOutputStream output = new FileOutputStream(file)) {
                byte[] chunk = new byte[CHUNK];
                int read;
                while ((read = input.read(chunk)) >= 0) {
                    output.write(chunk, 0, read);
                }
            }
        }
        return SaveShareProvider.uriFor(context, file);
    }
}
