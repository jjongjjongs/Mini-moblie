package com.jjongjjongs.minimobile;

import android.content.Context;

import java.io.File;
import java.util.List;

/**
 * Removes a title's saved data from the app's private directory.
 *
 * <p>A handset's storage outlives the run that wrote it, and so does a bad
 * write: 던전앤파이터 격투가 tests for its options file by opening it, and a
 * build that created one for that test left an empty file behind that the
 * title then read its options out of, drawing a white screen on every run
 * afterwards. Fixing the open stopped new ones appearing and did nothing for
 * the phones that already had one - there is nothing in such a file to tell it
 * apart from an options file the title meant to write, so nothing can single
 * it out at runtime. Taking the title's storage away and letting it start
 * again is what is left, and it is the player's call because it takes the
 * saves with it.
 *
 * <p>It removes exactly what {@link SaveExporter} would put in a zip, so the
 * export offered beside it is a complete backup of what this deletes.
 */
final class SaveEraser {
    private SaveEraser() {
    }

    /**
     * Deletes every file under the title's save directories, and the
     * directories themselves.
     *
     * @param archive the imported game file, read to find its ids
     * @return how many files were removed; zero if the title had saved nothing
     * @throws Exception if the archive's ids could not be read
     */
    static int erase(Context context, File archive) throws Exception {
        int removed = 0;
        for (File root : SaveExporter.roots(context, archive)) {
            removed += deleteTree(root);
        }

        return removed;
    }

    /** Deletes a directory's contents and then itself, counting the files. */
    private static int deleteTree(File root) {
        File[] entries = root.listFiles();
        int removed = 0;

        if (entries != null) {
            for (File entry : entries) {
                if (entry.isDirectory()) {
                    removed += deleteTree(entry);
                } else if (entry.delete()) {
                    removed++;
                }
            }
        }

        root.delete();

        return removed;
    }
}
