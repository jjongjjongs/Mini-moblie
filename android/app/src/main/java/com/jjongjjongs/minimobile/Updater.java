package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.Dialog;
import android.app.DownloadManager;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Locale;

/**
 * Updates the app from its own GitHub release: the newest build is downloaded
 * and Android's installer opens on it by itself as soon as the download ends.
 *
 * <p>CI replaces the {@code mobile-dev} release's {@code MiniMobile.apk} and
 * {@code version.txt} on every push, {@code version.txt} being the apk's
 * version code (its CI run number). When it is higher than the installed one
 * the app offers the update. The new apk is signed with the same key, so it
 * installs over this one and the saves stay.
 *
 * <p>The download goes through the system's DownloadManager, the way Mini
 * Emulator updates itself: its own notification, it carries on in the
 * background, and when it finishes the app hears {@code DOWNLOAD_COMPLETE}
 * and hands the installer the download provider's own {@code content://}
 * URI. An apk fetched by the app itself and served from its own provider is
 * what Samsung's Auto Blocker flagged as a suspected malicious app; the same
 * apk handed over as a finished system download installs like any download.
 *
 * <p>This is for the build distributed outside Google Play; a Play build must
 * not update itself this way and would leave the check turned off.
 */
public final class Updater {
    /** The release the CI build publishes to, under the project's public name. */
    private static final String BASE = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/";
    private static final String VERSION_URL = BASE + "version.txt";
    private static final String APK_URL = BASE + "MiniMobile.apk";
    /** The release page, opened if the download cannot be started. */
    private static final String RELEASE_PAGE = "https://github.com/jjongjjongs/Mini-moblie/releases/tag/mobile-dev";
    private static final String APK_TYPE = "application/vnd.android.package-archive";

    private static final String PREFS = "mini_update";
    private static final String KEY_LAST_CHECK = "last_check";
    /** The newest version the last successful check saw, so the bar can show a dot. */
    private static final String KEY_LATEST = "latest";
    /** The DownloadManager id of the update being fetched, and its version. */
    private static final String KEY_DOWNLOAD = "download_id";
    private static final String KEY_DOWNLOAD_VERSION = "download_version";
    /** Auto-check at most this often, so a launch is not a network round-trip. */
    private static final long CHECK_EVERY_MS = 24L * 60 * 60 * 1000;

    /** The install waits for the "이 출처 허용" setting, opened from here. */
    private static boolean installAfterPermission;
    private static BroadcastReceiver downloadReceiver;

    private Updater() {
    }

    private static SharedPreferences prefs(Context context) {
        return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** Whether the last check found a build newer than the installed one. */
    static boolean updateAvailable(Context context) {
        return prefs(context).getInt(KEY_LATEST, -1) > installedVersion(context);
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

    // ------------------------------------------------------------------
    // Lifecycle: the activity registers for the download's end, and on resume
    // picks up an install that waited for the permission or for the app.
    // ------------------------------------------------------------------

    /** From onCreate: hear the download manager say the update is in. */
    static void register(final Activity activity) {
        if (downloadReceiver != null) {
            return;
        }
        downloadReceiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                long id = intent.getLongExtra(DownloadManager.EXTRA_DOWNLOAD_ID, -1);
                if (id != -1 && id == downloadId(activity)) {
                    onDownloaded(activity);
                }
            }
        };
        IntentFilter filter = new IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE);
        if (Build.VERSION.SDK_INT >= 33) {
            // Sent by the download manager, another process: exported.
            activity.registerReceiver(downloadReceiver, filter, Context.RECEIVER_EXPORTED);
        } else {
            activity.registerReceiver(downloadReceiver, filter);
        }
    }

    /** From onDestroy. */
    static void unregister(Activity activity) {
        if (downloadReceiver == null) {
            return;
        }
        try {
            activity.unregisterReceiver(downloadReceiver);
        } catch (IllegalArgumentException ignored) {
            // Not registered.
        }
        downloadReceiver = null;
    }

    /**
     * From onResume: back from "이 출처 허용", install now; and a download that
     * finished while the app was away opens the installer on return.
     */
    static void resume(Activity activity) {
        if (installAfterPermission && canInstall(activity)) {
            installAfterPermission = false;
            install(activity);
            return;
        }
        long id = downloadId(activity);
        if (id == -1) {
            return;
        }
        long[] state = progress(activity, id);
        boolean newer = prefs(activity).getInt(KEY_DOWNLOAD_VERSION, -1) > installedVersion(activity);
        if (state[0] == DownloadManager.STATUS_SUCCESSFUL && newer) {
            onDownloaded(activity);
        } else if (state[0] == 0 || state[0] == DownloadManager.STATUS_FAILED || !newer) {
            // Gone, failed, or already installed.
            clearDownload(activity);
        }
    }

    // ------------------------------------------------------------------
    // Checking
    // ------------------------------------------------------------------

    /**
     * Checks once a day on launch, quietly: a newer build offers itself, and
     * anything else - up to date, or no network - says nothing.
     */
    static void autoCheck(Activity activity, Runnable onResult) {
        SharedPreferences prefs = prefs(activity);
        long now = System.currentTimeMillis();
        if (now - prefs.getLong(KEY_LAST_CHECK, 0) < CHECK_EVERY_MS) {
            return;
        }
        prefs.edit().putLong(KEY_LAST_CHECK, now).apply();
        check(activity, false, onResult);
    }

    /**
     * Checks for a newer build. When {@code manual}, every outcome is shown -
     * up to date, a failure, or a new build; an auto check shows only a new one.
     * {@code onResult} runs on the UI thread once the check settles, so the bar
     * can redraw its dot from {@link #updateAvailable}; it may be null.
     */
    static void check(final Activity activity, final boolean manual, final Runnable onResult) {
        final Handler ui = new Handler(Looper.getMainLooper());
        // A download already under way: show it rather than start another.
        long running = downloadId(activity);
        if (manual && running != -1) {
            long[] state = progress(activity, running);
            if (state[0] == DownloadManager.STATUS_RUNNING || state[0] == DownloadManager.STATUS_PENDING
                    || state[0] == DownloadManager.STATUS_PAUSED) {
                showProgress(activity, running);
                return;
            }
            if (state[0] == DownloadManager.STATUS_SUCCESSFUL
                    && prefs(activity).getInt(KEY_DOWNLOAD_VERSION, -1) > installedVersion(activity)) {
                install(activity);
                return;
            }
        }
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
            // Remembered so the bar's dot survives to the next launch, and clears
            // once the installed build has caught up.
            prefs(activity).edit().putInt(KEY_LATEST, latest).apply();
            final int installed = installedVersion(activity);
            ui.post(() -> {
                if (onResult != null) {
                    onResult.run();
                }
                if (activity.isFinishing()) {
                    return;
                }
                if (latest <= installed) {
                    if (manual) {
                        Toast.makeText(activity, "이미 최신 버전이에요 (버전 " + installed + ").", Toast.LENGTH_SHORT).show();
                    }
                    return;
                }
                promptUpdate(activity, installed, latest);
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

    /** The new-version card, in the app's green look: update now, or later. */
    private static void promptUpdate(final Activity activity, final int installed, final int latest) {
        Pal pal = Pal.of(activity);
        Dialog dialog = cardDialog(activity);
        LinearLayout card = card(activity, pal);

        LinearLayout body = cardBody(activity);
        body.addView(iconTile(activity, pal, LineIcon.Kind.UPDATE));
        body.addView(title(activity, pal, "업데이트 있음"));

        LinearLayout versions = new LinearLayout(activity);
        versions.setOrientation(LinearLayout.HORIZONTAL);
        versions.setGravity(Gravity.CENTER_VERTICAL);
        versions.setPadding(0, dp(activity, 12), 0, 0);
        if (installed > 0) {
            versions.addView(chip(activity, pal, String.valueOf(installed), false));
            versions.addView(arrow(activity, pal));
        }
        versions.addView(chip(activity, pal, String.valueOf(latest), true));
        body.addView(versions);

        body.addView(paragraph(activity, pal, "받기가 끝나면 설치 화면이 바로 떠요. 게임과 세이브는 그대로예요."));
        body.addView(meta(activity, pal, "받는 동안 다른 걸 해도 괜찮아요"));
        card.addView(body);
        card.addView(divider(activity, pal));

        LinearLayout acts = actionRow(activity);
        acts.addView(button(activity, "나중에", 0, pal.ghost, dialog::dismiss));
        acts.addView(button(activity, "지금 업데이트", pal.green, pal.onGreen, () -> {
            dialog.dismiss();
            startDownload(activity, latest);
        }));
        card.addView(acts);

        showCard(activity, dialog, card, true);
    }

    // ------------------------------------------------------------------
    // Downloading, through the system's DownloadManager
    // ------------------------------------------------------------------

    private static long downloadId(Context context) {
        return prefs(context).getLong(KEY_DOWNLOAD, -1);
    }

    private static void clearDownload(Context context) {
        prefs(context).edit().remove(KEY_DOWNLOAD).remove(KEY_DOWNLOAD_VERSION).apply();
    }

    private static void startDownload(Activity activity, int latest) {
        try {
            DownloadManager manager = (DownloadManager) activity.getSystemService(Context.DOWNLOAD_SERVICE);
            File dir = activity.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS);
            if (manager == null || dir == null) {
                throw new IllegalStateException("다운로드 관리자를 쓸 수 없습니다.");
            }
            // An earlier update's apk, or a half-finished one: start clean.
            long previous = downloadId(activity);
            if (previous != -1) {
                manager.remove(previous);
            }
            String name = "MiniMobile-" + latest + ".apk";
            File target = new File(dir, name);
            if (target.exists()) {
                target.delete();
            }
            DownloadManager.Request request = new DownloadManager.Request(Uri.parse(APK_URL));
            request.setTitle("Mini Mobile 업데이트 (버전 " + latest + ")");
            request.setDescription("업데이트 받는 중");
            request.setMimeType(APK_TYPE);
            request.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);
            request.setDestinationInExternalFilesDir(activity, Environment.DIRECTORY_DOWNLOADS, name);
            long id = manager.enqueue(request);
            prefs(activity).edit().putLong(KEY_DOWNLOAD, id).putInt(KEY_DOWNLOAD_VERSION, latest).apply();
            showProgress(activity, id);
        } catch (Exception e) {
            // No download manager: the browser downloads it instead.
            if (!openLink(activity, APK_URL) && !openLink(activity, RELEASE_PAGE)) {
                Toast.makeText(activity, "받을 수 없어요. 직접 받아 주세요:\n" + RELEASE_PAGE, Toast.LENGTH_LONG).show();
            }
        }
    }

    /** {status, bytes so far, total bytes}; status 0 when the download is gone. */
    private static long[] progress(Context context, long id) {
        DownloadManager manager = (DownloadManager) context.getSystemService(Context.DOWNLOAD_SERVICE);
        if (manager == null) {
            return new long[]{0, 0, 0};
        }
        try (Cursor cursor = manager.query(new DownloadManager.Query().setFilterById(id))) {
            if (cursor == null || !cursor.moveToFirst()) {
                return new long[]{0, 0, 0};
            }
            return new long[]{
                    cursor.getInt(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS)),
                    cursor.getLong(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR)),
                    cursor.getLong(cursor.getColumnIndexOrThrow(DownloadManager.COLUMN_TOTAL_SIZE_BYTES))};
        } catch (Exception e) {
            return new long[]{0, 0, 0};
        }
    }

    /**
     * The download card: a big percent over a green bar, read from the
     * download manager. 숨기기 leaves it running in the notification; the
     * installer opens by itself when it ends either way.
     */
    private static void showProgress(final Activity activity, final long id) {
        Pal pal = Pal.of(activity);
        final Dialog dialog = cardDialog(activity);
        LinearLayout card = card(activity, pal);

        LinearLayout body = cardBody(activity);
        body.addView(title(activity, pal, "업데이트 받는 중"));

        LinearLayout pct = new LinearLayout(activity);
        pct.setOrientation(LinearLayout.HORIZONTAL);
        pct.setGravity(Gravity.BOTTOM);
        pct.setPadding(0, dp(activity, 12), 0, 0);
        final TextView percent = new TextView(activity);
        percent.setText("0%");
        percent.setTextSize(30f);
        percent.setTypeface(null, Typeface.BOLD);
        percent.setTextColor(pal.ink);
        pct.addView(percent, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        final TextView sizes = new TextView(activity);
        sizes.setTextSize(12.5f);
        sizes.setTextColor(pal.muted);
        sizes.setPadding(0, 0, 0, dp(activity, 5));
        pct.addView(sizes);
        body.addView(pct);

        // The bar: a filled part and the rest, their weights the percent.
        final LinearLayout track = new LinearLayout(activity);
        track.setOrientation(LinearLayout.HORIZONTAL);
        track.setWeightSum(100f);
        track.setBackground(roundedRect(activity, pal.track, pal.track, 0, 999));
        final View fill = new View(activity);
        fill.setBackground(roundedRect(activity, pal.green, pal.green, 0, 999));
        track.addView(fill, new LinearLayout.LayoutParams(0, dp(activity, 8), 0f));
        final View rest = new View(activity);
        track.addView(rest, new LinearLayout.LayoutParams(0, dp(activity, 8), 100f));
        LinearLayout.LayoutParams trackParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(activity, 8));
        trackParams.topMargin = dp(activity, 16);
        trackParams.bottomMargin = dp(activity, 8);
        body.addView(track, trackParams);

        body.addView(paragraph(activity, pal, "다 받으면 설치 화면이 바로 떠요. 숨겨도 알림에서 계속 받아요."));
        card.addView(body);
        card.addView(divider(activity, pal));

        LinearLayout acts = actionRow(activity);
        acts.addView(button(activity, "취소", 0, pal.danger, () -> {
            DownloadManager manager = (DownloadManager) activity.getSystemService(Context.DOWNLOAD_SERVICE);
            if (manager != null) {
                manager.remove(id);
            }
            clearDownload(activity);
            dialog.dismiss();
        }));
        acts.addView(button(activity, "숨기기", pal.green, pal.onGreen, dialog::dismiss));
        card.addView(acts);

        showCard(activity, dialog, card, true);

        final Handler handler = new Handler(Looper.getMainLooper());
        final Runnable[] poll = new Runnable[1];
        poll[0] = () -> {
            if (!dialog.isShowing()) {
                return;
            }
            long[] state = progress(activity, id);
            if (state[2] > 0) {
                int shown = (int) (100L * state[1] / state[2]);
                percent.setText(shown + "%");
                sizes.setText(mb(state[1]) + " / " + mb(state[2]));
                ((LinearLayout.LayoutParams) fill.getLayoutParams()).weight = shown;
                ((LinearLayout.LayoutParams) rest.getLayoutParams()).weight = 100 - shown;
                track.requestLayout();
            }
            if (state[0] == DownloadManager.STATUS_SUCCESSFUL) {
                // The receiver opens the installer; the card just goes.
                dialog.dismiss();
                return;
            }
            if (state[0] == DownloadManager.STATUS_FAILED || state[0] == 0) {
                dialog.dismiss();
                clearDownload(activity);
                Toast.makeText(activity, "받지 못했어요. 인터넷을 확인하고 다시 해 주세요.", Toast.LENGTH_LONG).show();
                return;
            }
            handler.postDelayed(poll[0], 500);
        };
        poll[0].run();
    }

    // ------------------------------------------------------------------
    // Installing
    // ------------------------------------------------------------------

    private static void onDownloaded(Activity activity) {
        if (activity.isFinishing() || activity.isDestroyed()) {
            return;
        }
        install(activity);
    }

    private static boolean canInstall(Context context) {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.O || context.getPackageManager().canRequestPackageInstalls();
    }

    /**
     * Opens Android's installer on the finished download - the download
     * provider's own URI, which the installer is let read. The first time,
     * Android wants this app allowed as an install source; the card opens that
     * setting and the install goes on by itself on the way back.
     */
    private static void install(final Activity activity) {
        long id = downloadId(activity);
        DownloadManager manager = (DownloadManager) activity.getSystemService(Context.DOWNLOAD_SERVICE);
        Uri uri = id == -1 || manager == null ? null : manager.getUriForDownloadedFile(id);
        if (uri == null) {
            clearDownload(activity);
            Toast.makeText(activity, "받은 파일을 찾지 못했어요. 다시 받아 주세요.", Toast.LENGTH_LONG).show();
            return;
        }
        if (!canInstall(activity)) {
            askInstallPermission(activity);
            return;
        }
        Intent intent = new Intent(Intent.ACTION_VIEW);
        intent.setDataAndType(uri, APK_TYPE);
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            activity.startActivity(intent);
        } catch (Exception e) {
            Toast.makeText(activity, "설치 화면을 열지 못했어요: " + e.getMessage(), Toast.LENGTH_LONG).show();
        }
    }

    /** The one-time "이 출처 허용" card. */
    private static void askInstallPermission(final Activity activity) {
        Pal pal = Pal.of(activity);
        Dialog dialog = cardDialog(activity);
        LinearLayout card = card(activity, pal);

        LinearLayout body = cardBody(activity);
        body.addView(iconTile(activity, pal, LineIcon.Kind.UPDATE));
        body.addView(title(activity, pal, "설치 허용이 한 번 필요해요"));
        body.addView(paragraph(activity, pal,
                "안드로이드는 앱이 업데이트를 설치하려면 한 번 허용을 받아요. 다음 화면에서 ‘이 출처 허용’을 켜고 돌아오면 바로 설치 화면이 떠요."));
        body.addView(meta(activity, pal, "한 번만 켜면 다음부터는 안 물어봐요"));
        card.addView(body);
        card.addView(divider(activity, pal));

        LinearLayout acts = actionRow(activity);
        acts.addView(button(activity, "취소", 0, pal.ghost, dialog::dismiss));
        acts.addView(button(activity, "설정 열기", pal.green, pal.onGreen, () -> {
            dialog.dismiss();
            installAfterPermission = true;
            try {
                activity.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:" + activity.getPackageName())));
            } catch (Exception e) {
                installAfterPermission = false;
                Toast.makeText(activity, "설정을 열 수 없어요. 휴대폰 설정 › 앱 › Mini Mobile에서 ‘출처를 알 수 없는 앱 설치’를 허용해 주세요.", Toast.LENGTH_LONG).show();
            }
        }));
        card.addView(acts);

        showCard(activity, dialog, card, true);
    }

    private static boolean openLink(Activity activity, String url) {
        try {
            activity.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            return true;
        } catch (Exception e) {
            return false;
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

    // ------------------------------------------------------------------
    // Card pieces, in the library's green look (light or dark as chosen).
    // ------------------------------------------------------------------

    private static Dialog cardDialog(Activity activity) {
        Dialog dialog = new Dialog(activity);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        return dialog;
    }

    private static void showCard(Activity activity, Dialog dialog, View card, boolean cancelable) {
        dialog.setContentView(card);
        dialog.setCanceledOnTouchOutside(cancelable);
        Window window = dialog.getWindow();
        if (window != null) {
            window.setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
            int width = Math.min(activity.getResources().getDisplayMetrics().widthPixels - dp(activity, 40), dp(activity, 360));
            window.setLayout(width, ViewGroup.LayoutParams.WRAP_CONTENT);
        }
        dialog.show();
    }

    private static LinearLayout card(Activity activity, Pal pal) {
        LinearLayout card = new LinearLayout(activity);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setBackground(roundedRect(activity, pal.panel, pal.line, 1, 20));
        return card;
    }

    private static LinearLayout cardBody(Activity activity) {
        LinearLayout body = new LinearLayout(activity);
        body.setOrientation(LinearLayout.VERTICAL);
        int side = dp(activity, 20);
        body.setPadding(side, side, side, dp(activity, 16));
        return body;
    }

    private static View iconTile(Activity activity, Pal pal, LineIcon.Kind kind) {
        ImageView icon = new ImageView(activity);
        icon.setImageDrawable(new LineIcon(kind, pal.greenDeep, 1.9f));
        int tile = dp(activity, 46);
        int pad = dp(activity, 11);
        icon.setPadding(pad, pad, pad, pad);
        icon.setBackground(roundedRect(activity, pal.greenSoft, pal.greenLine, 1, 14));
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(tile, tile);
        params.bottomMargin = dp(activity, 14);
        icon.setLayoutParams(params);
        return icon;
    }

    private static TextView title(Activity activity, Pal pal, String text) {
        TextView view = new TextView(activity);
        view.setText(text);
        view.setTextSize(18f);
        view.setTypeface(null, Typeface.BOLD);
        view.setTextColor(pal.ink);
        return view;
    }

    private static TextView paragraph(Activity activity, Pal pal, String text) {
        TextView view = new TextView(activity);
        view.setText(text);
        view.setTextSize(13.5f);
        view.setTextColor(pal.muted);
        view.setLineSpacing(dp(activity, 3), 1f);
        view.setPadding(0, dp(activity, 8), 0, 0);
        return view;
    }

    private static TextView meta(Activity activity, Pal pal, String text) {
        TextView view = new TextView(activity);
        view.setText(text);
        view.setTextSize(12f);
        view.setTextColor(pal.muted);
        view.setPadding(0, dp(activity, 10), 0, 0);
        return view;
    }

    private static TextView chip(Activity activity, Pal pal, String text, boolean fresh) {
        TextView view = new TextView(activity);
        view.setText(text);
        view.setTextSize(12.5f);
        view.setTypeface(null, Typeface.BOLD);
        view.setTextColor(fresh ? pal.greenDeep : pal.muted);
        int px = dp(activity, 9);
        int py = dp(activity, 3);
        view.setPadding(px, py, px, py);
        view.setBackground(roundedRect(activity,
                fresh ? pal.greenSoft : pal.raise, fresh ? pal.greenLine : pal.line, 1, 999));
        return view;
    }

    private static TextView arrow(Activity activity, Pal pal) {
        TextView view = new TextView(activity);
        view.setText("→");
        view.setTextSize(14f);
        view.setTextColor(pal.muted);
        int px = dp(activity, 8);
        view.setPadding(px, 0, px, 0);
        return view;
    }

    /** The hairline over the button row, as in the mockup. */
    private static View divider(Activity activity, Pal pal) {
        View line = new View(activity);
        line.setBackgroundColor(pal.line);
        line.setLayoutParams(new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Math.max(1, dp(activity, 1))));
        return line;
    }

    private static LinearLayout actionRow(Activity activity) {
        LinearLayout acts = new LinearLayout(activity);
        acts.setOrientation(LinearLayout.HORIZONTAL);
        acts.setGravity(Gravity.END);
        int pad = dp(activity, 10);
        acts.setPadding(pad, pad, pad, pad);
        return acts;
    }

    private static TextView button(Activity activity, String text, int fill, int fg, Runnable onClick) {
        TextView view = new TextView(activity);
        view.setText(text);
        view.setTextSize(14f);
        view.setTypeface(null, Typeface.BOLD);
        view.setTextColor(fg);
        int px = dp(activity, 16);
        int py = dp(activity, 11);
        view.setPadding(px, py, px, py);
        if (fill != 0) {
            view.setBackground(roundedRect(activity, fill, fill, 0, 12));
        }
        view.setOnClickListener(v -> onClick.run());
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        params.leftMargin = dp(activity, 6);
        view.setLayoutParams(params);
        return view;
    }

    private static GradientDrawable roundedRect(Activity activity, int fill, int stroke, int strokeDp, int radiusDp) {
        GradientDrawable shape = new GradientDrawable();
        shape.setShape(GradientDrawable.RECTANGLE);
        shape.setColor(fill);
        if (strokeDp > 0) {
            shape.setStroke(dp(activity, strokeDp), stroke);
        }
        shape.setCornerRadius(dp(activity, radiusDp));
        return shape;
    }

    private static int dp(Context context, int value) {
        return Math.round(value * context.getResources().getDisplayMetrics().density);
    }

    /** The card palette, light or dark as the library is set, matching its own. */
    private static final class Pal {
        final int panel;
        final int raise;
        final int ink;
        final int muted;
        final int line;
        final int green;
        final int onGreen;
        final int greenDeep;
        final int greenSoft;
        final int greenLine;
        final int ghost;
        final int track;
        final int danger;

        private Pal(boolean dark) {
            if (dark) {
                panel = Color.rgb(22, 29, 25);
                raise = Color.rgb(27, 36, 31);
                ink = Color.rgb(228, 236, 230);
                muted = Color.rgb(138, 156, 144);
                line = Color.rgb(36, 48, 42);
                green = Color.rgb(76, 194, 126);
                onGreen = Color.rgb(7, 19, 13);
                greenDeep = Color.rgb(111, 212, 154);
                greenSoft = Color.rgb(28, 52, 38);
                greenLine = Color.rgb(43, 74, 55);
                ghost = Color.rgb(159, 176, 165);
                track = Color.rgb(34, 48, 41);
                danger = Color.rgb(255, 154, 143);
            } else {
                panel = Color.rgb(255, 255, 255);
                raise = Color.rgb(241, 246, 243);
                ink = Color.rgb(26, 42, 32);
                muted = Color.rgb(100, 117, 104);
                line = Color.rgb(233, 241, 235);
                green = Color.rgb(46, 139, 87);
                onGreen = Color.rgb(255, 255, 255);
                greenDeep = Color.rgb(34, 114, 71);
                greenSoft = Color.rgb(220, 242, 226);
                greenLine = Color.rgb(199, 232, 209);
                ghost = Color.rgb(100, 117, 104);
                track = Color.rgb(223, 236, 230);
                danger = Color.rgb(201, 58, 58);
            }
        }

        static Pal of(Context context) {
            return new Pal(LibraryLook.dark(context));
        }
    }
}
