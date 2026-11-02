package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.Dialog;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
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
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
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
 * <p>The install goes through {@link PackageInstaller}, with the release named
 * as where the bytes came from, the way a browser attributes a download - so
 * Play Protect sees a known origin rather than an app quietly side-loading an
 * apk. The person still allows this app as an install source once, and still
 * confirms the install itself.
 *
 * <p>This is for the build distributed outside Google Play; a Play build must
 * not update itself this way and would leave the check turned off.
 */
public final class Updater {
    /** The release the CI build publishes to, under the project's public name. */
    private static final String BASE = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/";
    private static final String VERSION_URL = BASE + "version.txt";
    private static final String APK_URL = BASE + "MiniMobile.apk";
    /** The release page, named to the installer as where the apk came from. */
    private static final String RELEASE_PAGE = "https://github.com/jjongjjongs/Mini-moblie/releases/tag/mobile-dev";

    /** The broadcast the install session reports its result on. */
    static final String INSTALL_ACTION = "com.jjongjjongs.minimobile.INSTALL_RESULT";

    private static final String PREFS = "mini_update";
    private static final String KEY_LAST_CHECK = "last_check";
    /** The newest version the last successful check saw, so the bar can show a dot. */
    private static final String KEY_LATEST = "latest";
    /** Auto-check at most this often, so a launch is not a network round-trip. */
    private static final long CHECK_EVERY_MS = 24L * 60 * 60 * 1000;

    private Updater() {
    }

    /** Whether the last check found a build newer than the installed one. */
    static boolean updateAvailable(Context context) {
        int latest = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getInt(KEY_LATEST, -1);
        return latest > installedVersion(context);
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

    /** Where the downloaded apk is kept. */
    static File apkFile(Context context) {
        return new File(new File(context.getCacheDir(), "update"), "MiniMobile.apk");
    }

    /**
     * Checks once a day on launch, quietly: a newer build offers itself, and
     * anything else - up to date, or no network - says nothing.
     */
    static void autoCheck(Activity activity, Runnable onResult) {
        SharedPreferences prefs = activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
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
            activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putInt(KEY_LATEST, latest).apply();
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

    // ------------------------------------------------------------------
    // The cards
    // ------------------------------------------------------------------

    /** ① The new-version card: the version step, in the app's green look. */
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

        body.addView(paragraph(activity, pal, "설치해도 게임과 세이브는 그대로예요."));
        body.addView(meta(activity, pal, "앱 안에서 바로 받아서 설치해요 · 설치는 한 번 더 확인해요"));
        card.addView(body);
        card.addView(divider(activity, pal));

        LinearLayout acts = actionRow(activity, pal);
        acts.addView(button(activity, "나중에", 0, pal.ghost, dialog::dismiss));
        acts.addView(button(activity, "지금 업데이트", pal.green, pal.onGreen, () -> {
            dialog.dismiss();
            startUpdate(activity, latest);
        }));
        card.addView(acts);

        showCard(activity, dialog, card, true);
    }

    /**
     * ② The install-source card, shown only when the app is not yet an allowed
     * source, then ③ the download.
     */
    private static void startUpdate(final Activity activity, final int latest) {
        // From Android 8 the app needs the person's go-ahead to install apks it
        // downloads. Before that it was one system-wide setting, and launching
        // the installer is enough.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !activity.getPackageManager().canRequestPackageInstalls()) {
            Pal pal = Pal.of(activity);
            Dialog dialog = cardDialog(activity);
            LinearLayout card = card(activity, pal);

            LinearLayout body = cardBody(activity);
            body.addView(iconTile(activity, pal, LineIcon.Kind.SHIELD));
            body.addView(title(activity, pal, "설치 권한 한 번만"));
            body.addView(paragraph(activity, pal,
                    "앱에서 업데이트를 설치하려면 이 앱을 ‘이 출처 허용’으로 한 번 켜 주세요. 설정을 열어 드릴게요."));
            body.addView(meta(activity, pal, "한 번만 켜면 다음부터는 안 물어봐요"));
            card.addView(body);
            card.addView(divider(activity, pal));

            LinearLayout acts = actionRow(activity, pal);
            acts.addView(button(activity, "취소", 0, pal.ghost, dialog::dismiss));
            acts.addView(button(activity, "설정 열기", pal.green, pal.onGreen, () -> {
                dialog.dismiss();
                try {
                    activity.startActivity(new Intent(
                            Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                            Uri.parse("package:" + activity.getPackageName())));
                } catch (Exception e) {
                    Toast.makeText(activity, "설정을 열 수 없어요. 휴대폰 설정에서 ‘출처를 알 수 없는 앱 설치’를 켜 주세요.", Toast.LENGTH_LONG).show();
                }
            }));
            card.addView(acts);

            showCard(activity, dialog, card, true);
            return;
        }
        download(activity, latest);
    }

    /** ③ The download card: a big percent over a green bar, then the install. */
    private static void download(final Activity activity, final int latest) {
        Pal pal = Pal.of(activity);
        Dialog dialog = cardDialog(activity);
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

        body.addView(paragraph(activity, pal, "다 받으면 설치 화면이 떠요. 받는 동안 다른 걸 해도 괜찮아요."));
        card.addView(body);
        card.addView(divider(activity, pal));

        final boolean[] cancelled = {false};
        LinearLayout acts = actionRow(activity, pal);
        acts.addView(button(activity, "취소", 0, pal.danger, () -> {
            cancelled[0] = true;
            dialog.dismiss();
        }));
        card.addView(acts);

        dialog.setCancelable(false);
        showCard(activity, dialog, card, false);

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
                            int shown = (int) (got * 100 / total);
                            if (shown != lastPercent) {
                                lastPercent = shown;
                                final int shownPercent = shown;
                                final long shownGot = got;
                                final int shownTotal = total;
                                ui.post(() -> {
                                    percent.setText(shownPercent + "%");
                                    sizes.setText(mb(shownGot) + " / " + mb(shownTotal));
                                    ((LinearLayout.LayoutParams) fill.getLayoutParams()).weight = shownPercent;
                                    ((LinearLayout.LayoutParams) rest.getLayoutParams()).weight = 100 - shownPercent;
                                    track.requestLayout();
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

    /**
     * Hands the downloaded apk to {@link PackageInstaller}, naming the release
     * it came from. The session streams the apk's bytes in and commits; the
     * system then asks the person to confirm, and {@link InstallReceiver}
     * launches that prompt and reports the result.
     */
    private static void install(Activity activity, File apk) {
        try {
            PackageInstaller installer = activity.getPackageManager().getPackageInstaller();
            PackageInstaller.SessionParams params =
                    new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            // Say where the bytes came from, the way a browser does for a
            // download: the file as the origin, the release page as the
            // referrer. A named origin reads as a plain update rather than an
            // app side-loading an apk out of nowhere.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                params.setOriginatingUri(Uri.parse(APK_URL));
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                params.setReferrerUri(Uri.parse(RELEASE_PAGE));
            }
            int sessionId = installer.createSession(params);
            try (PackageInstaller.Session session = installer.openSession(sessionId)) {
                try (InputStream in = new FileInputStream(apk);
                     OutputStream out = session.openWrite("MiniMobile.apk", 0, apk.length())) {
                    byte[] buffer = new byte[65536];
                    int read;
                    while ((read = in.read(buffer)) >= 0) {
                        out.write(buffer, 0, read);
                    }
                    session.fsync(out);
                }
                Intent intent = new Intent(INSTALL_ACTION).setPackage(activity.getPackageName());
                int flags = PendingIntent.FLAG_UPDATE_CURRENT;
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    flags |= PendingIntent.FLAG_MUTABLE;
                }
                PendingIntent pending = PendingIntent.getBroadcast(activity, sessionId, intent, flags);
                session.commit(pending.getIntentSender());
            }
        } catch (Exception e) {
            Toast.makeText(activity, "설치를 시작하지 못했어요: " + e.getMessage(), Toast.LENGTH_LONG).show();
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

    private static LinearLayout actionRow(Activity activity, Pal pal) {
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
        final int track;
        final int ghost;
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
                track = Color.rgb(34, 48, 41);
                ghost = Color.rgb(159, 176, 165);
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
                track = Color.rgb(223, 236, 230);
                ghost = Color.rgb(100, 117, 104);
                danger = Color.rgb(201, 58, 58);
            }
        }

        static Pal of(Context context) {
            return new Pal(LibraryLook.dark(context));
        }
    }
}
