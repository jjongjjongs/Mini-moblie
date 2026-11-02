package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.Dialog;
import android.app.DownloadManager;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInfo;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.os.Handler;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;

/**
 * Tells whether a newer build has been published, and sends the player to get
 * it from the browser.
 *
 * <p>CI replaces the {@code mobile-dev} release's {@code MiniMobile.apk} and
 * {@code version.txt} on every push, {@code version.txt} being the apk's
 * version code (its CI run number). The app reads that number, and when it is
 * higher than the installed one offers to open the apk's download in the
 * browser. The new apk is signed with the same key as the installed one, so it
 * installs over it and the saves stay.
 *
 * <p>The download deliberately goes through the browser rather than installing
 * from inside the app. A self-signed apk installed by another app trips
 * Samsung's Auto Blocker ("악성 앱으로 의심", a block that cannot even be turned
 * off) and Play Protect's dropper warning, because what matters to them is
 * <em>who</em> starts the install, not the bytes - the very same apk downloaded
 * in the browser installs without a word, since the browser is a trusted
 * source. So the app hands the download to the browser and the person installs
 * it the way they would any download.
 *
 * <p>This is for the build distributed outside Google Play; a Play build must
 * not offer this and would leave the check turned off.
 */
public final class Updater {
    /** The release the CI build publishes to, under the project's public name. */
    private static final String BASE = "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/";
    private static final String VERSION_URL = BASE + "version.txt";
    private static final String APK_URL = BASE + "MiniMobile.apk";
    /** The release page, opened if the direct download cannot be. */
    private static final String RELEASE_PAGE = "https://github.com/jjongjjongs/Mini-moblie/releases/tag/mobile-dev";

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

    /** The new-version card, in the app's green look: get it, or not now. */
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

        body.addView(paragraph(activity, pal, "새 버전을 받아요. 다 받으면 뜨는 알림을 눌러 설치하세요."));
        body.addView(meta(activity, pal, "게임과 세이브는 그대로예요"));
        card.addView(body);
        card.addView(divider(activity, pal));

        LinearLayout acts = actionRow(activity);
        acts.addView(button(activity, "나중에", 0, pal.ghost, dialog::dismiss));
        acts.addView(button(activity, "지금 받기", pal.green, pal.onGreen, () -> {
            dialog.dismiss();
            openDownload(activity, latest);
        }));
        card.addView(acts);

        showCard(activity, dialog, card, true);
    }

    /**
     * Downloads the apk through the system's DownloadManager, which shows a
     * progress notification and, when it finishes, one the person taps to
     * install. The system opens the installer then, as a trusted source -
     * unlike this app installing it itself, which Samsung's Auto Blocker flags
     * as a suspected malicious app. The file lands in Downloads, so a missed
     * notification is still easy to find. If the download manager cannot be
     * used, the browser is the fallback.
     */
    private static void openDownload(Activity activity, int latest) {
        try {
            DownloadManager manager = (DownloadManager) activity.getSystemService(Context.DOWNLOAD_SERVICE);
            if (manager == null) {
                throw new IllegalStateException("다운로드 관리자를 쓸 수 없습니다.");
            }
            DownloadManager.Request request = new DownloadManager.Request(Uri.parse(APK_URL));
            request.setTitle("Mini Mobile 업데이트");
            request.setDescription("새 버전(버전 " + latest + ")을 받는 중…");
            request.setMimeType("application/vnd.android.package-archive");
            request.setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED);
            // A name with the version, so a new build never clashes with an apk
            // left from a previous update already sitting in Downloads.
            request.setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, "MiniMobile-" + latest + ".apk");
            manager.enqueue(request);
            Toast.makeText(activity, "새 버전을 받는 중이에요.\n다 받으면 뜨는 알림을 눌러 설치하세요.", Toast.LENGTH_LONG).show();
        } catch (Exception e) {
            // No download manager, or the file is already there: fall back to
            // the browser, which downloads it the same way.
            if (!openLink(activity, APK_URL) && !openLink(activity, RELEASE_PAGE)) {
                Toast.makeText(activity, "받을 수 없어요. 직접 받아 주세요:\n" + RELEASE_PAGE, Toast.LENGTH_LONG).show();
            }
        }
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
            }
        }

        static Pal of(Context context) {
            return new Pal(LibraryLook.dark(context));
        }
    }
}
