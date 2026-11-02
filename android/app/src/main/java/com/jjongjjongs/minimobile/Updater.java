package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.view.ViewGroup;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Locale;

/**
 * Updates the app from its own GitHub release, so a patch is installed from
 * inside the app instead of downloaded by hand each time.
 *
 * <p>CI replaces the {@code mobile-dev} release's {@code MiniMobile.apk} and
 * {@code version.txt} on every push, {@code version.txt} being the apk's
 * version code (its CI run number). The app reads that number, and when it is
 * higher than the installed one offers to download the apk and hand it to the
 * system installer. The new apk is signed with the same key as the installed
 * one, so it installs over it and the saves stay.
 *
 * <p>This is for the build distributed outside Google Play; a Play build must
 * not update itself this way and would leave the check turned off.
 */
public final class Updater {
    /** The release the CI build publishes to, under the project's public name. */
    private static final String BASE = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/";
    private static final String VERSION_URL = BASE + "version.txt";
    private static final String APK_URL = BASE + "MiniMobile.apk";

    private static final String PREFS = "mini_update";
    private static final String KEY_LAST_CHECK = "last_check";
    /** Auto-check at most this often, so a launch is not a network round-trip. */
    private static final long CHECK_EVERY_MS = 24L * 60 * 60 * 1000;

    private Updater() {
    }

    /** The installed app's version code, or -1 if it cannot be read. */
    static int installedVersion(Context context) {
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(context.getPackageName(), 0);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                return (int) info.getLongVersionCode();
            }
            return info.versionCode;
        } catch (Exception e) {
            return -1;
        }
    }

    /** Where the downloaded apk is kept; {@link ApkProvider} serves this one file. */
    static File apkFile(Context context) {
        return new File(new File(context.getCacheDir(), "update"), "MiniMobile.apk");
    }

    /**
     * Checks once a day on launch, quietly: a newer build offers itself, and
     * anything else - up to date, or no network - says nothing.
     */
    static void autoCheck(Activity activity) {
        SharedPreferences prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        long now = System.currentTimeMillis();
        if (now - prefs.getLong(KEY_LAST_CHECK, 0) < CHECK_EVERY_MS) {
            return;
        }
        prefs.edit().putLong(KEY_LAST_CHECK, now).apply();
        check(activity, false);
    }

    /**
     * Checks for a newer build. When {@code manual}, every outcome is shown -
     * up to date, a failure, or a new build; an auto check shows only a new one.
     */
    static void check(final Activity activity, final boolean manual) {
        final Handler ui = new Handler(Looper.getMainLooper());
        if (manual) {
            Toast.makeText(activity, "업데이트 확인 중…", Toast.LENGTH_SHORT).show();
        }
        new Thread(() -> {
            final int latest;
            try {
                latest = fetchLatest();
            } catch (Exception e) {
                if (manual) {
                    ui.post(() -> Toast.makeText(activity, "업데이트를 확인하지 못했어요. 인터넷을 확인해 주세요.", Toast.LENGTH_SHORT).show());
                }
                return;
            }
            final int installed = installedVersion(activity);
            ui.post(() -> {
                if (activity.isFinishing()) {
                    return;
                }
                if (latest <= installed) {
                    if (manual) {
                        Toast.makeText(activity, "이미 최신 버전이에요 (버전 " + installed + ").", Toast.LENGTH_SHORT).show();
                    }
                    return;
                }
                promptUpdate(activity, latest);
            });
        }, "update-check").start();
    }

    private static int fetchLatest() throws IOException {
        HttpURLConnection connection = open(VERSION_URL);
        try {
            if (connection.getResponseCode() != HttpURLConnection.HTTP_OK) {
                throw new IOException("HTTP " + connection.getResponseCode());
            }
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(connection.getInputStream()))) {
                String line = reader.readLine();
                return Integer.parseInt(line.trim());
            }
        } catch (NumberFormatException e) {
            throw new IOException("버전 번호를 읽을 수 없습니다.", e);
        } finally {
            connection.disconnect();
        }
    }

    private static void promptUpdate(final Activity activity, final int latest) {
        new AlertDialog.Builder(activity)
                .setTitle("업데이트 있음")
                .setMessage("새 버전(버전 " + latest + ")이 있어요.\n지금 받아서 설치할까요?\n\n설치해도 세이브는 지워지지 않아요.")
                .setPositiveButton("업데이트", (dialog, which) -> startUpdate(activity, latest))
                .setNegativeButton("나중에", null)
                .show();
    }

    private static void startUpdate(final Activity activity, final int latest) {
        // From Android 8 the app needs the person's go-ahead to install apks it
        // downloads. Before that it was one system-wide setting, and launching
        // the installer is enough.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !activity.getPackageManager().canRequestPackageInstalls()) {
            new AlertDialog.Builder(activity)
                    .setTitle("설치 권한이 필요해요")
                    .setMessage("앱에서 업데이트를 설치하려면 이 앱에 ‘이 출처 허용’을 한 번 켜 줘야 해요.\n설정 화면을 열게요. 허용한 뒤 다시 ‘업데이트 확인’을 눌러 주세요.")
                    .setPositiveButton("설정 열기", (dialog, which) -> {
                        try {
                            activity.startActivity(new Intent(
                                    Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                                    Uri.parse("package:" + activity.getPackageName())));
                        } catch (Exception e) {
                            Toast.makeText(activity, "설정을 열 수 없어요. 휴대폰 설정에서 ‘출처를 알 수 없는 앱 설치’를 켜 주세요.", Toast.LENGTH_LONG).show();
                        }
                    })
                    .setNegativeButton("취소", null)
                    .show();
            return;
        }
        download(activity, latest);
    }

    private static void download(final Activity activity, final int latest) {
        final ProgressBar bar = new ProgressBar(activity, null, android.R.attr.progressBarStyleHorizontal);
        bar.setMax(100);
        final TextView label = new TextView(activity);
        label.setText("내려받는 중… 0%");
        label.setPadding(dp(activity, 24), dp(activity, 18), dp(activity, 24), dp(activity, 4));

        LinearLayout box = new LinearLayout(activity);
        box.setOrientation(LinearLayout.VERTICAL);
        box.addView(label);
        LinearLayout.LayoutParams barParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        barParams.leftMargin = dp(activity, 24);
        barParams.rightMargin = dp(activity, 24);
        barParams.bottomMargin = dp(activity, 16);
        box.addView(bar, barParams);

        final boolean[] cancelled = {false};
        final AlertDialog dialog = new AlertDialog.Builder(activity)
                .setTitle("업데이트 받는 중")
                .setView(box)
                .setCancelable(false)
                .setNegativeButton("취소", (d, w) -> cancelled[0] = true)
                .create();
        dialog.show();

        final Handler ui = new Handler(Looper.getMainLooper());
        new Thread(() -> {
            File out = apkFile(activity);
            HttpURLConnection connection = null;
            try {
                File dir = out.getParentFile();
                if (dir != null) {
                    dir.mkdirs();
                }
                connection = open(APK_URL);
                if (connection.getResponseCode() != HttpURLConnection.HTTP_OK) {
                    throw new IOException("HTTP " + connection.getResponseCode());
                }
                int total = connection.getContentLength();
                try (InputStream in = connection.getInputStream();
                     FileOutputStream fileOut = new FileOutputStream(out)) {
                    byte[] buffer = new byte[65536];
                    long got = 0;
                    int read;
                    int lastPercent = -1;
                    while ((read = in.read(buffer)) >= 0) {
                        if (cancelled[0]) {
                            fileOut.close();
                            out.delete();
                            return;
                        }
                        fileOut.write(buffer, 0, read);
                        got += read;
                        if (total > 0) {
                            int percent = (int) (got * 100 / total);
                            if (percent != lastPercent) {
                                lastPercent = percent;
                                final int shownPercent = percent;
                                final long shownGot = got;
                                final int shownTotal = total;
                                ui.post(() -> {
                                    bar.setProgress(shownPercent);
                                    label.setText("내려받는 중… " + shownPercent + "%  (" + mb(shownGot) + " / " + mb(shownTotal) + ")");
                                });
                            }
                        }
                    }
                }
                ui.post(() -> {
                    dialog.dismiss();
                    if (!cancelled[0] && !activity.isFinishing()) {
                        install(activity, out);
                    }
                });
            } catch (Exception e) {
                out.delete();
                ui.post(() -> {
                    dialog.dismiss();
                    if (!cancelled[0] && !activity.isFinishing()) {
                        Toast.makeText(activity, "내려받기에 실패했어요. 다시 시도해 주세요.", Toast.LENGTH_LONG).show();
                    }
                });
            } finally {
                if (connection != null) {
                    connection.disconnect();
                }
            }
        }, "update-download").start();
    }

    private static void install(Activity activity, File apk) {
        try {
            Uri uri = ApkProvider.uriFor(activity, apk);
            Intent intent = new Intent(Intent.ACTION_VIEW);
            intent.setDataAndType(uri, "application/vnd.android.package-archive");
            intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
            activity.startActivity(intent);
        } catch (Exception e) {
            Toast.makeText(activity, "설치 화면을 열 수 없어요: " + e.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    private static HttpURLConnection open(String url) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        // GitHub serves a release asset by redirecting to its CDN, always over
        // https, so the default follow handles it.
        connection.setInstanceFollowRedirects(true);
        connection.setConnectTimeout(15000);
        connection.setReadTimeout(30000);
        connection.setRequestProperty("User-Agent", "MiniMobile");
        return connection;
    }

    private static String mb(long bytes) {
        return String.format(Locale.US, "%.1fMB", bytes / 1048576.0);
    }

    private static int dp(Context context, int value) {
        return Math.round(value * context.getResources().getDisplayMetrics().density);
    }
}
