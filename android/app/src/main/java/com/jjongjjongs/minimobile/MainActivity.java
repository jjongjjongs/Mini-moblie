package com.jjongjjongs.minimobile;

import android.Manifest;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.content.res.Configuration;
import android.database.ContentObserver;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Outline;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.text.InputFilter;
import android.text.Spannable;
import android.text.SpannableString;
import android.text.TextUtils;
import android.text.style.ForegroundColorSpan;
import android.util.Log;
import android.util.SparseArray;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewOutlineProvider;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import android.view.WindowManager;

import java.io.BufferedInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.Arrays;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.Collections;
import java.util.Enumeration;
import java.util.Locale;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import java.util.zip.ZipInputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;

/**
 * Library of imported games plus the player that runs one.
 *
 * <p>The emulator runs on a single background thread which owns
 * {@link NativeBridge#nativeStart}, {@link NativeBridge#nativeTick} and frame
 * collection. Touches post key events from the UI thread; the native side
 * queues them.
 */
public final class MainActivity extends Activity {
    private static final String TAG = "WIE-Input";

    private static final int PICK_GAME = 1001;
    private static final int REQUEST_WRITE_DOWNLOADS = 1002;
    private static final int PICK_SAVE = 1003;
    private static final int REQUEST_CALL_PHONE = 1004;

    /**
     * How long a single tick may run, and the delay scheduled after it finishes.
     *
     * <p>The delay used to be the same whatever the tick did, so a CPU-bound
     * title paid it every time and the run/idle ratio was
     * {@code BUDGET / (BUDGET + INTERVAL)}: 20/16 starved the emulator to 56% of
     * real time, and 20/4 still gave away a sixth of it. The delay is there for
     * an idle title - the native tick returns the instant the emulator reports
     * every task asleep, and without a delay a menu would spin the thread - so
     * it is only owed when the tick actually went idle.
     *
     * <p>So the loop now asks how the tick went. One that used its whole budget
     * had work left and is re-armed at once ({@link #BUSY_INTERVAL_MS}); one
     * that came back early had nothing to do and sleeps
     * {@link #TICK_INTERVAL_MS}. A title that needs the CPU gets all of it, and
     * a menu still costs the same handful of wakeups a second it did before.
     */
    private static final int TICK_BUDGET_MS = 20;
    private static final int TICK_INTERVAL_MS = 4;
    private static final int BUSY_INTERVAL_MS = 0;

    /**
     * The longest the loop will sleep on the emulator's word, whatever it says.
     *
     * <p>Input is read at the top of a step, so a sleep is also how long a key
     * press can sit unhandled. A tick may already hold the thread for
     * {@link #TICK_BUDGET_MS}, so waiting about as long as that costs nothing a
     * player has not already been paying, while still covering the 15ms frame
     * timer these titles ask for. A title idling in a menu is asked about again
     * this often rather than every {@link #TICK_INTERVAL_MS}, which is fewer
     * wakeups than before, not more.
     */
    private static final int MAX_IDLE_SLEEP_MS = 16;

    /** Audio commands drained per tick, so a backlog cannot stall the loop. */
    private static final int MAX_AUDIO_PER_TICK = 32;

    /** Ticks without a frame before the status line shows what tick reported. */
    private static final int STATUS_TICKS = 60;

    /**
     * How long one tick may run before the player says the game has hung.
     *
     * <p>A healthy tick returns within {@link #TICK_BUDGET_MS}, so anything
     * near a second is already a title that has stopped yielding; four seconds
     * is far enough past a slow load or a long garbage collection to mean it.
     */
    private static final long WEDGE_MS = 4000;

    /** How often the watchdog looks. */
    private static final long WEDGE_POLL_MS = 1000;

    /** How the player splits its height between the screen and the keypad. */
    private static final float GAME_WEIGHT = 2.3f;
    private static final float KEYPAD_WEIGHT = 1f;

    /**
     * Share of the keypad's height taken by the function row, which sits where
     * the keys under a handset's screen did. Two keys over each half, so the
     * pad and the numbers below it each keep their whole half.
     */
    private static final float KEYPAD_TOP_ROW = 0.19f;

    /** How a key is painted. */
    // The handset keys a gamepad can reach, by the code the emulator takes.
    // These are the same codes the keypad below sends; a pad is another way of
    // pressing the same keys, not a second input path with its own numbering.
    static final int CODE_UP = 0;
    static final int CODE_DOWN = 1;
    static final int CODE_LEFT = 2;
    static final int CODE_RIGHT = 3;
    static final int CODE_OK = 4;
    static final int CODE_SOFT_L = 5;
    static final int CODE_SOFT_R = 6;
    static final int CODE_CLEAR = 7;
    static final int CODE_STAR = 18;
    static final int CODE_HASH = 19;
    /** 저장 - the key a handset marked SEND, which is how titles reach their save screen. */
    static final int CODE_SAVE = 20;

    /**
     * The groups the key-mapping screen breaks its rows into.
     *
     * <p>The rows are the keypad's own key set, because a key the pad cannot
     * reach is a key the player has to put the pad down for.
     */
    private static final String[] KEY_GROUP_TITLES = {"방향키 · 확인", "기능키", "숫자키", "기호키"};

    /** The handset key codes of each group, in row order. */
    private static final int[][] KEY_GROUPS = {
            {CODE_UP, CODE_DOWN, CODE_LEFT, CODE_RIGHT, CODE_OK},
            {CODE_SOFT_L, CODE_SOFT_R, CODE_SAVE, CODE_CLEAR},
            {9, 10, 11, 12, 13, 14, 15, 16, 17},
            {CODE_STAR, 8, CODE_HASH},
    };

    /** The badge on the left of a row, by handset key code. */
    private static final String[] KEY_BADGES = new String[21];

    /** What a row is called, by handset key code. */
    private static final String[] KEY_NAMES = new String[21];

    static {
        put(CODE_UP, "▲", "위");
        put(CODE_DOWN, "▼", "아래");
        put(CODE_LEFT, "◀", "왼쪽");
        put(CODE_RIGHT, "▶", "오른쪽");
        put(CODE_OK, "OK", "확인 (OK)");
        put(CODE_SOFT_L, "L", "좌상단 (L)");
        put(CODE_SOFT_R, "R", "우상단 (R)");
        put(CODE_SAVE, "SV", "저장");
        put(CODE_CLEAR, "BK", "뒤로가기");
        put(8, "0", "0 번");
        for (int digit = 1; digit <= 9; digit++) {
            put(8 + digit, String.valueOf(digit), digit + " 번");
        }
        put(CODE_STAR, "✱", "✱ (별표)");
        put(CODE_HASH, "#", "# (샵)");
    }

    private static void put(int code, String badge, String name) {
        KEY_BADGES[code] = badge;
        KEY_NAMES[code] = name;
    }

    /** Whether a code names a handset key a pad may be pointed at. */
    static boolean isHandsetKey(int code) {
        return code >= 0 && code < KEY_NAMES.length && KEY_NAMES[code] != null;
    }

    private static final int KEY_PLAIN = 0;
    private static final int KEY_SAVE = 1;
    private static final int KEY_CLEAR = 2;
    private static final int KEY_DIRECTION = 3;
    private static final int KEY_SOFT = 4;

    // Device-dark palette shared by the library and the player, so the whole
    // app reads as one piece of hardware rather than a list over a player.
    private static final int COLOR_BG = Color.rgb(18, 19, 23);        // ground
    private static final int COLOR_PANEL = Color.rgb(28, 30, 36);     // bars, cards, rows
    private static final int COLOR_PANEL_2 = Color.rgb(35, 38, 46);   // raised surface
    private static final int COLOR_HAIR = Color.rgb(43, 46, 55);      // hairline borders
    private static final int COLOR_TEXT = Color.rgb(233, 234, 237);
    private static final int COLOR_SUBTEXT = Color.rgb(154, 156, 166);
    private static final int COLOR_ACCENT = Color.rgb(84, 199, 214);  // toggle / active
    /** Behind the emulated LCD, so its letterbox reads as a screen bezel. */
    private static final int COLOR_SCREEN_BEZEL = Color.rgb(10, 11, 14);
    /** The pad the keys sit on: dark, matching the device body. */
    private static final int COLOR_KEYPAD_TRAY = Color.rgb(30, 26, 22);

    // The keypad is the gold-on-dark face of a Korean feature phone: a warm
    // near-black panel with every glyph and every key outline engraved in
    // champagne gold. One palette for every key - numbers, directions, soft
    // keys, save and back all read as the same milled surface, the way a
    // handset's pad does.
    private static final int COLOR_KEY_FACE_TOP = Color.rgb(54, 47, 39);
    private static final int COLOR_KEY_FACE_BOTTOM = Color.rgb(39, 34, 28);
    private static final int COLOR_KEY_EDGE = Color.rgb(150, 122, 74);
    private static final int COLOR_KEY_INK = Color.rgb(226, 194, 138);
    /** The letters engraved beside a digit, a stop dimmer than the digit. */
    private static final int COLOR_KEY_INK_SUB = Color.rgb(170, 140, 94);
    private static final int COLOR_KEY_PRESSED = Color.rgb(108, 89, 57);

    // Light "Mini Mobile" palette for the library/home screen: a clean white
    // ground with a single green accent, matching the approved home redesign.
    // The player screen keeps the dark device palette above.
    private static final int LIB_BG = Color.rgb(255, 255, 255);
    private static final int LIB_SURFACE = Color.rgb(255, 255, 255);
    private static final int LIB_INK = Color.rgb(26, 42, 32);          // #1a2a20 primary text
    private static final int LIB_MUTED = Color.rgb(100, 117, 104);     // #647568 secondary text
    private static final int LIB_LINE = Color.rgb(233, 241, 235);      // #e9f1eb card border
    private static final int LIB_DIVIDER = Color.rgb(238, 243, 239);   // #eef3ef row divider
    private static final int LIB_GREEN = Color.rgb(46, 139, 87);       // #2e8b57 accent
    private static final int LIB_GREEN_DEEP = Color.rgb(34, 114, 71);  // #227247 accent text
    private static final int LIB_GREEN_SOFT = Color.rgb(220, 242, 226);// #dcf2e2 chip/button fill
    private static final int LIB_GREEN_LINE = Color.rgb(199, 232, 209);// #c7e8d1 button border
    private static final int LIB_GREEN_SOFTER = Color.rgb(238, 248, 241);// #eef8f1 empty tile

    // Carrier badge/chip colours as {ink, soft fill, line}, soft tones chosen to
    // sit on the light library ground rather than the loud brand colours.
    private static final int[] CARRIER_SKT = {Color.rgb(194, 65, 12), Color.rgb(253, 234, 221), Color.rgb(246, 211, 189)};
    private static final int[] CARRIER_KTF = {Color.rgb(29, 95, 191), Color.rgb(226, 236, 251), Color.rgb(207, 224, 247)};
    private static final int[] CARRIER_LGT = {Color.rgb(163, 38, 143), Color.rgb(247, 226, 242), Color.rgb(239, 207, 230)};
    private static final int[] CARRIER_ETC = {Color.rgb(100, 117, 104), Color.rgb(238, 243, 239), Color.rgb(226, 233, 228)};
    // A DRM-locked download: a muted red, so the badge reads as "cannot run"
    // rather than as another carrier.
    private static final int[] CARRIER_DRM = {Color.rgb(153, 57, 57), Color.rgb(248, 232, 232), Color.rgb(237, 213, 213)};

    private static final int LIB_DELETE = Color.rgb(192, 57, 43);       // #c0392b delete button
    private static final int LIB_RED_SOFT = Color.rgb(253, 236, 235);   // #fdeceb danger icon tile
    private static final int LIB_SELECT_BG = Color.rgb(243, 250, 245);  // #f3faf5 selected row tint

    private static final int LIB_STAR = Color.rgb(230, 167, 0);         // #e6a700 favourite star
    private static final int LIB_STAR_OFF = Color.rgb(194, 204, 197);   // #c2ccc5 unfavourited star
    private static final int LIB_STAR_SOFT = Color.rgb(253, 243, 214);  // #fdf3d6 ⭐ chip fill
    private static final int LIB_STAR_LINE = Color.rgb(242, 224, 168);  // #f2e0a8 ⭐ chip border
    private static final int LIB_STAR_INK = Color.rgb(154, 116, 0);     // #9a7400 ⭐ chip text

    /**
     * How much stack the emulator thread gets.
     *
     * A WIPI title is emulated by running its ARM code and answering the
     * services it calls, and those answers run more of its code - loading a
     * class runs its initialiser, which loads another class - so how deep the
     * native stack goes is decided by the game, not by us. Nothing in the
     * native library asks for much on its own: built the way it ships, its
     * largest single stack frame is under 4 KB. Depth is what runs it out.
     *
     * A thread from {@link Executors#newSingleThreadScheduledExecutor()} takes
     * the platform's default, which is 1 MB, and the Y700 crash report is that
     * 1 MB spent - SIGSEGV with the stack pointer one page past the bottom of
     * the thread's mapping, while {@code nativeStart} was still loading the
     * game. So the emulator is given a thread whose stack is sized for it.
     *
     * The cost is address space, not memory: the pages are committed as they
     * are first touched, and the app is arm64-only, where 64 MB of reserved
     * address space is nothing.
     */
    private static final long EMULATOR_STACK_BYTES = 64L * 1024 * 1024;

    private static final ThreadFactory EMULATOR_THREADS =
            runnable -> new Thread(null, runnable, "emulator", EMULATOR_STACK_BYTES);

    private final ScheduledExecutorService emulatorThread = Executors.newSingleThreadScheduledExecutor(EMULATOR_THREADS);

    private AndroidAudioOutput audioOutput;
    private File gamesDir;

    // Library search + carrier filter state. librarySearch is the typed query;
    // libraryCarrier is "" for 전체 or one of "KTF"/"LGT"/"SKT"/"ETC". Carriers
    // are detected once per archive and cached by name+size+mtime so the list
    // does not re-read every file on each keystroke.
    private String librarySearch = "";
    private String libraryCarrier = "";
    // Favourites: a set of game file names, and whether the ⭐ chip is narrowing
    // the list to them. Favourited games also float to the top of the mixed list.
    private final java.util.HashSet<String> favorites = new java.util.HashSet<>();
    private boolean libraryFavOnly = false;
    private final java.util.HashMap<String, String> carrierCache = new java.util.HashMap<>();
    // Cover icons cached by name+size+mtime so refilling the list on each
    // keystroke or selection tap does not re-read every archive from disk. A
    // null value is cached too - it means "this archive carries no icon".
    private final java.util.HashMap<String, Bitmap> iconCache = new java.util.HashMap<>();
    private LinearLayout libraryListContainer;
    private LinearLayout libraryChipRow;
    private TextView librarySectCount;
    private TextView librarySectTitle;

    // Multi-select delete. selectMode swaps the rows to checkboxes and shows the
    // action bar; selected holds the chosen games by absolute path; libraryShown
    // is the current filtered list (what 전체 selects and what a rebuild draws).
    private boolean selectMode = false;
    private final java.util.HashSet<String> selected = new java.util.HashSet<>();
    private java.util.ArrayList<File> libraryShown = new java.util.ArrayList<>();
    private TextView librarySelectAction;
    private LinearLayout librarySelectBar;

    private GameView gameView;
    private KeypadView keypad;
    private TextView playerStatus;
    /** The two halves of what used to be one log button. See {@link #startLogCollect()}. */
    private Button collectButton;

    private Button stopButton;
    private Button rotateButton;

    /**
     * Whether the player is held in one orientation rather than following the
     * phone.
     *
     * <p>What the rotate button offers while the phone's own auto-rotate is on.
     * With it off the player is held either way - there is nothing to follow -
     * and the button is the two-way switch it has always been; the flag is
     * still kept true there, so that turning auto-rotate on afterwards finds
     * the player held rather than loose.
     */
    private boolean orientationPinned;

    /** Redraws the rotate button when the phone's auto-rotate setting changes. */
    private ContentObserver autoRotateObserver;
    private String currentGameName;
    /** The game the player is showing, kept so a rotation can relay it out. */
    private File currentGame;
    /** Which way the player is turned; the toggle in the title bar flips it. */
    private boolean landscapeMode;
    /** What is waiting on the storage permission, if anything. */
    private Runnable pendingDownload;
    /** Telephone number waiting for Android's runtime CALL_PHONE permission. */
    private String pendingPhoneCall;

    private volatile boolean running;
    private volatile boolean foreground = true;
    /** Set while the exit-confirmation dialog is up, to freeze the game. */
    private volatile boolean paused;
    /** How long back is held on a phone's keypad to open the game menu rather than press 취소. */
    private static final long BACK_HOLD_MS = 600;

    private boolean playerVisible;
    /**
     * Whether the on-screen keypad is put away for the running title, leaving
     * the whole screen to the game. Kept per title (see {@link #gameKeypadHidden}).
     */
    private boolean keypadHidden;
    /** A long press of back on a phone's own keypad, waiting to open the game menu. */
    private Runnable backHold;
    /** Whether the back key now held has already opened the game menu. */
    private boolean backHeldLong;
    private int statusCounter;
    /**
     * Set once the game has painted a frame. The boot status the tick reports
     * is only news until then: a running game skips a tick's frame whenever it
     * has not finished a new one, and reporting that in the title bar made the
     * game's name flicker in and out of it.
     */
    private volatile boolean framePainted;

    /**
     * When the tick now running started, on the elapsed-real-time clock, or
     * zero between ticks. Written by the emulator thread, read by the watchdog.
     */
    private volatile long tickStartedAt;

    /**
     * How long the last {@link NativeBridge#nativeTick} call took, in
     * milliseconds, and 0 for a step that did not tick at all. The loop reads it
     * to decide whether the emulator still had work when its budget ran out.
     */
    private long lastTickRanMs;

    /**
     * The step waiting to run, so a key press can pull it forward.
     *
     * <p>Written by the emulator thread as it re-arms and read by the UI thread
     * when it has input, which is why it is volatile.
     */
    private volatile ScheduledFuture<?> pendingStep;

    /**
     * Whether a key has arrived that the running step will not have seen.
     *
     * <p>Cancelling the waiting step covers a key that lands while the loop is
     * asleep. This covers the other half: one that lands after a step has
     * already taken the input it is going to take, which cannot be cancelled
     * into and would otherwise wait out the next sleep.
     */
    private final AtomicBoolean inputSinceStep = new AtomicBoolean();

    /** Whether the player is currently saying the game has stopped answering. */
    private boolean wedgeReported;

    /** Whether the player is currently saying the game is busy loading. */
    private boolean busyReported;

    /** `nativeGuestProgress` as of the last watchdog poll, to compare against. */
    private long lastGuestProgress;

    private final Handler wedgeWatch = new Handler(Looper.getMainLooper());

    /**
     * Says what a tick running far past its budget is actually doing.
     *
     * <p>A tick is as long as the emulated title makes it: guest code runs
     * until it awaits, so a loading routine that never yields is one tick, and
     * the emulator thread stays inside it. The screen stops changing and
     * nothing else about the app does, which looks like the app having died -
     * so the person has no reason to think the log button would still work.
     *
     * <p>But how long a tick has run does not say whether the title is stuck:
     * 에스테반루크's 새로하기 is a single tick of several seconds that then goes
     * on to the game perfectly well, and calling that "응답하지 않습니다" told a
     * person their game had hung when it was loading. What separates the two is
     * whether the guest is still retiring instructions, so that is what is
     * asked, and a title that is working says so.
     */
    private final Runnable watchForWedge = new Runnable() {
        @Override
        public void run() {
            long started = tickStartedAt;
            boolean overrunning = started != 0 && SystemClock.elapsedRealtime() - started >= WEDGE_MS;

            long progress = NativeBridge.nativeGuestProgress();
            boolean advancing = progress != lastGuestProgress;
            lastGuestProgress = progress;

            boolean busy = overrunning && advancing;
            boolean wedged = overrunning && !advancing;

            if (wedged != wedgeReported || busy != busyReported) {
                wedgeReported = wedged;
                busyReported = busy;
                if (playerStatus != null) {
                    String text;
                    if (wedged) {
                        text = "게임이 응답하지 않습니다 - 로그 저장을 누르면 여기까지가 저장됩니다";
                    } else if (busy) {
                        text = "불러오는 중입니다 - 잠시만 기다려 주세요";
                    } else {
                        text = currentGameName != null ? currentGameName : "";
                    }
                    playerStatus.setText(text);
                }
            }

            if (playerVisible) {
                wedgeWatch.postDelayed(this, WEDGE_POLL_MS);
            }
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        audioOutput = new AndroidAudioOutput(this);
        padMapping = PadMapping.load(this);
        padPresets = PadPresets.load(this, padMapping);

        gamesDir = new File(getFilesDir(), "games");
        if (!gamesDir.exists()) {
            gamesDir.mkdirs();
        }

        // Favourites persist across runs, keyed by the archive's file name.
        favorites.addAll(getSharedPreferences("mini_ui", MODE_PRIVATE).getStringSet("favorites", java.util.Collections.emptySet()));

        seedDataFoldersOnce();

        // The rotate button says one thing while the phone turns its own screen
        // and another while it does not, and the player can be left running
        // while that setting is changed from the notification shade.
        autoRotateObserver = new ContentObserver(new Handler(Looper.getMainLooper())) {
            @Override
            public void onChange(boolean selfChange) {
                showRotateState();
            }
        };
        getContentResolver().registerContentObserver(
                Settings.System.getUriFor(Settings.System.ACCELEROMETER_ROTATION), false, autoRotateObserver);

        showLibrary();

        scheduleEmulatorStep(0);
    }

    @Override
    protected void onResume() {
        super.onResume();
        foreground = true;
        // The AudioTracks are paused, not released, when we leave the
        // foreground, so playback picks up where it left off on return.
        audioOutput.resume();
        ControlPatch.onResume(this);
    }

    @Override
    protected void onPause() {
        ControlPatch.onPause(this);
        foreground = false;
        // Leaving the foreground while a key is held would otherwise leave it
        // stuck down: the finger's ACTION_UP is delivered to whatever takes
        // over the screen, never to us. Release everything now so the game
        // does not read a key as held across the interruption.
        releaseKeypad();
        // Silence the tracks while backgrounded rather than letting the game's
        // BGM and effects bleed on over whatever the player switched to.
        audioOutput.pause();
        super.onPause();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        ControlPatch.onFocus(this, hasFocus);
        super.onWindowFocusChanged(hasFocus);
        // A notification shade or dialog can take focus without pausing us,
        // and swallows the touch release the same way. Drop held keys as soon
        // as focus is lost.
        if (!hasFocus) {
            releaseKeypad();
        } else if (playerVisible) {
            // Regaining focus clears sticky immersive, so put it back.
            applyImmersive(true);
        }
    }

    /** Releases every key the keypad currently holds, if a keypad is shown. */
    private void releaseKeypad() {
        KeypadView view = keypad;
        if (view != null) {
            view.releaseAll();
        }
        // A finger on the game screen is lost the same way.
        if (gameView != null) {
            gameView.liftTouch();
        }

        // A pad's buttons go the same way a finger does: the release arrives
        // wherever focus went, not here.
        releasePad();
    }

    // --- gamepad ---------------------------------------------------------
    //
    // A pad presses the same handset keys the on-screen keypad does; what it
    // cannot reach is the number pad, which has more keys than a pad has
    // buttons, so that stays on the screen.
    //
    // The pad's own held-key state is kept here rather than in the keypad
    // view, because the two are pressed independently: a finger holding ▶
    // while a stick is pushed left must not have its key taken away when the
    // stick centres. Each side releases only what it is holding.

    /** Whether a handset key is held by the pad, by handset key code. */
    private final boolean[] padHeld = new boolean[21];

    /**
     * How far a stick leaves centre before it counts as a direction.
     *
     * A handset's D-pad is a switch, so an analogue stick has to be made into
     * one somewhere. Half travel is far enough that a resting stick's drift
     * never reads as a press, and near enough that a player pushing a
     * direction gets it well before the stick bottoms out.
     */
    private static final float STICK_THRESHOLD = 0.5f;

    /**
     * The player's key mapping, which every pad event is read through.
     *
     * <p>Loaded once and kept: a pad event arrives on the UI thread and must
     * not wait on storage, and the settings screen edits a copy and hands the
     * result back here when it is saved.
     */
    private PadMapping padMapping;

    /** The handset key a gamepad button presses, or -1 for one it does not. */
    private int handsetKeyFor(int keyCode) {
        return padMapping == null ? -1 : padMapping.handsetKeyForKeyCode(keyCode);
    }

    /**
     * Whether an event came from a pad.
     *
     * A pad reports several sources at once - a stick, a D-pad and buttons are
     * one device - so any of them being present is enough. Plain `SOURCE_DPAD`
     * is not asked for on its own: a keyboard's arrow keys carry it, and they
     * are not a pad.
     */
    private static boolean fromGamepad(InputEvent event) {
        int source = event.getSource();

        return (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;
    }

    /** Presses or releases a handset key the pad is driving. */
    private void padKey(int code, boolean down) {
        if (code < 0 || code >= padHeld.length || padHeld[code] == down) {
            return;
        }

        padHeld[code] = down;
        sendKey(code, down);
    }

    /** Releases every handset key the pad currently holds. */
    private void releasePad() {
        for (int code = 0; code < padHeld.length; code++) {
            padKey(code, false);
        }
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        int code = playerVisible && fromGamepad(event) ? handsetKeyFor(keyCode) : -1;
        if (code < 0) {
            return super.onKeyDown(keyCode, event);
        }

        // A held button repeats. The game reads a key as held or not, so the
        // repeats say nothing the first press has not already said.
        if (event.getRepeatCount() == 0) {
            padKey(code, true);
        }

        return true;
    }

    @Override
    public boolean onKeyUp(int keyCode, KeyEvent event) {
        int code = playerVisible && fromGamepad(event) ? handsetKeyFor(keyCode) : -1;
        if (code < 0) {
            return super.onKeyUp(keyCode, event);
        }

        padKey(code, false);

        return true;
    }

    @Override
    public boolean onGenericMotionEvent(MotionEvent event) {
        if (!playerVisible || !fromGamepad(event) || event.getAction() != MotionEvent.ACTION_MOVE) {
            return super.onGenericMotionEvent(event);
        }

        // Some pads report their D-pad as a hat rather than as key events, so
        // the hat is read first and the stick only answers for what the hat
        // leaves at rest - otherwise a pad carrying both would have its stick
        // undo what its D-pad just pressed.
        float x = event.getAxisValue(MotionEvent.AXIS_HAT_X);
        float y = event.getAxisValue(MotionEvent.AXIS_HAT_Y);

        if (x == 0.0f && y == 0.0f) {
            x = event.getAxisValue(MotionEvent.AXIS_X);
            y = event.getAxisValue(MotionEvent.AXIS_Y);
        }

        // An axis presses whatever its button was pointed at, so a player who
        // moves the D-pad somewhere else takes the stick with it.
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_DPAD_LEFT), x <= -STICK_THRESHOLD);
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_DPAD_RIGHT), x >= STICK_THRESHOLD);
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_DPAD_UP), y <= -STICK_THRESHOLD);
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_DPAD_DOWN), y >= STICK_THRESHOLD);

        // Many pads report a trigger only as an axis, never as a button, so
        // L2 and R2 would be unreachable on them without this. A trigger rests
        // at zero and runs to one, so half travel is a press.
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_L2),
                event.getAxisValue(MotionEvent.AXIS_LTRIGGER) >= STICK_THRESHOLD);
        padKey(padMapping.handsetKeyOf(PadMapping.PAD_R2),
                event.getAxisValue(MotionEvent.AXIS_RTRIGGER) >= STICK_THRESHOLD);

        return true;
    }

    // ControlPatch owns the gamepad: its mapping table, live capture and
    // preset slots. Consuming pad events here, before they reach onKeyDown /
    // onGenericMotionEvent, keeps a single owner and avoids a button firing
    // twice. Non-pad events return false and fall through untouched.
    @Override
    public boolean dispatchKeyEvent(KeyEvent event) {
        if (phoneBackKey(event)) {
            return true;
        }
        if (ControlPatch.onGamepadKey(this, event)) {
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent event) {
        if (ControlPatch.onGamepadMotion(this, event)) {
            return true;
        }
        return super.dispatchGenericMotionEvent(event);
    }

    @Override
    protected void onDestroy() {
        ControlPatch.onDestroy(this);
        running = false;
        if (autoRotateObserver != null) {
            getContentResolver().unregisterContentObserver(autoRotateObserver);
            autoRotateObserver = null;
        }
        NativeBridge.nativeStop();
        audioOutput.release();
        emulatorThread.shutdownNow();
        super.onDestroy();
    }

    @Override
    public void onBackPressed() {
        if (ControlPatch.onBack(this)) {
            return;
        }
        if (keyMapVisible) {
            // The screen keeps its own working copy, so back is the same
            // question its own arrow asks.
            leaveKeyMap(editingMapping);
            return;
        }

        if (!playerVisible) {
            // In the library, back first leaves multi-select rather than the app.
            if (selectMode) {
                exitSelectMode();
                return;
            }
            super.onBackPressed();
            return;
        }

        // In a game, back asks before leaving. The game is frozen while the
        // dialog is up: 예 quits to the library, 아니요 (or dismissing the dialog
        // with back / an outside tap) resumes it in place.
        confirmLeaveGame();
    }

    /** Stops the running game and returns to the library. */
    private void exitGameToLibrary() {
        running = false;
        paused = false;
        NativeBridge.nativeStop();
        audioOutput.release();
        showLibrary();
    }

    // --- library ---------------------------------------------------------

    private void showLibrary() {
        ControlPatch.onLeave(this);
        running = false;
        playerVisible = false;
        wedgeWatch.removeCallbacks(watchForWedge);
        wedgeReported = false;
        busyReported = false;
        lastGuestProgress = 0;
        tickStartedAt = 0;
        keyMapVisible = false;
        rotateButton = null;
        keypad = null;
        landscapeMode = false;
        // The library is always upright, whichever way the player was left.
        setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);
        // The home screen is light, so the status-bar icons must go dark.
        setLightStatusBar(true);
        // The library is an ordinary screen with its bars.
        applyImmersive(false);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(LIB_BG);

        // A small title strip at the very top, like the mockup's app bar.
        TextView bar = new TextView(this);
        bar.setText("Mini Mobile");
        bar.setTextSize(16f);
        bar.setTypeface(Typeface.DEFAULT_BOLD);
        bar.setTextColor(LIB_INK);
        bar.setPadding(dp(18), dp(12), dp(18), dp(6));
        root.addView(bar);

        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(16), dp(4), dp(16), dp(20));

        // Header name with the green status dot.
        LinearLayout nameRow = new LinearLayout(this);
        nameRow.setOrientation(LinearLayout.HORIZONTAL);
        nameRow.setGravity(android.view.Gravity.CENTER_VERTICAL);
        View dot = new View(this);
        dot.setBackground(circle(LIB_GREEN));
        LinearLayout.LayoutParams dotParams = new LinearLayout.LayoutParams(dp(8), dp(8));
        dotParams.rightMargin = dp(8);
        nameRow.addView(dot, dotParams);
        TextView name = new TextView(this);
        name.setText("Mini Mobile");
        name.setTextSize(19f);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setTextColor(LIB_INK);
        nameRow.addView(name, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        // Gamepad key mapping now lives in the in-game settings (the gear menu),
        // so the library no longer needs its own entry.

        content.addView(nameRow);

        // Every header line the mockup keeps, just at smaller sizes.
        TextView sub = new TextView(this);
        sub.setText("독립 실행형 WIPI 에뮬레이터");
        sub.setTextSize(11.5f);
        sub.setTextColor(LIB_MUTED);
        sub.setPadding(0, dp(4), 0, 0);
        content.addView(sub);

        TextView store = new TextView(this);
        store.setText("게임 저장소: " + gamesDir.getAbsolutePath());
        store.setTextSize(10.5f);
        store.setTextColor(LIB_MUTED);
        store.setPadding(0, dp(1), 0, 0);
        content.addView(store);

        TextView use = new TextView(this);
        use.setText("게임 실행: 한 번 누르기 · 세이브 꺼내기·불러오기 · 삭제: 길게 누르기");
        use.setTextSize(11f);
        use.setTextColor(LIB_MUTED);
        use.setPadding(0, dp(8), 0, dp(14));
        content.addView(use);

        // Two soft-green pill buttons, side by side.
        LinearLayout actions = new LinearLayout(this);
        Button refresh = flatButton("목록 새로고침");
        refresh.setOnClickListener(v -> showLibrary());
        actions.addView(refresh, buttonParams(0));
        Button pick = flatButton("ZIP 가져오기");
        pick.setOnClickListener(v -> openPicker());
        actions.addView(pick, buttonParams(dp(10)));
        content.addView(actions, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42)));

        // A returning session starts on the full, unfiltered list, not in
        // selection mode.
        librarySearch = "";
        libraryCarrier = "";
        libraryFavOnly = false;
        selectMode = false;
        selected.clear();

        // Search box: filters the list by title as you type.
        LinearLayout searchBox = new LinearLayout(this);
        searchBox.setOrientation(LinearLayout.HORIZONTAL);
        searchBox.setGravity(android.view.Gravity.CENTER_VERTICAL);
        searchBox.setBackground(roundedRect(LIB_GREEN_SOFTER, LIB_LINE, 1, 12));
        searchBox.setPadding(dp(12), dp(2), dp(10), dp(2));
        TextView mag = new TextView(this);
        mag.setText("🔍");
        mag.setTextSize(13f);
        mag.setPadding(0, 0, dp(8), 0);
        searchBox.addView(mag);
        EditText search = new EditText(this);
        search.setHint("게임 검색…");
        search.setTextSize(14f);
        search.setTextColor(LIB_INK);
        search.setHintTextColor(LIB_MUTED);
        search.setSingleLine(true);
        search.setBackground(null);
        search.setPadding(0, dp(8), 0, dp(8));
        search.setImeOptions(android.view.inputmethod.EditorInfo.IME_ACTION_SEARCH);
        search.addTextChangedListener(new android.text.TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void afterTextChanged(android.text.Editable e) {
                librarySearch = e.toString().trim();
                refreshLibraryList();
            }
        });
        searchBox.addView(search, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        LinearLayout.LayoutParams searchParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        searchParams.topMargin = dp(14);
        content.addView(searchBox, searchParams);

        // Carrier filter chips (전체 / SKT / KTF / LGT / 기타), rebuilt with live
        // counts each refresh.
        libraryChipRow = new LinearLayout(this);
        libraryChipRow.setOrientation(LinearLayout.HORIZONTAL);
        libraryChipRow.setPadding(0, dp(10), 0, 0);
        HorizontalScrollView chipScroll = new HorizontalScrollView(this);
        chipScroll.setHorizontalScrollBarEnabled(false);
        chipScroll.addView(libraryChipRow);
        content.addView(chipScroll);

        // Section header: title on the left, live (filtered) count on the right.
        LinearLayout sect = new LinearLayout(this);
        sect.setOrientation(LinearLayout.HORIZONTAL);
        sect.setGravity(android.view.Gravity.CENTER_VERTICAL);
        sect.setPadding(dp(2), dp(16), dp(2), dp(8));
        librarySectTitle = new TextView(this);
        librarySectTitle.setText("게임 목록");
        librarySectTitle.setTextSize(13f);
        librarySectTitle.setTypeface(Typeface.DEFAULT_BOLD);
        librarySectTitle.setTextColor(LIB_INK);
        sect.addView(librarySectTitle, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        librarySectCount = new TextView(this);
        librarySectCount.setTextSize(12f);
        librarySectCount.setTextColor(LIB_MUTED);
        sect.addView(librarySectCount);
        // A '선택' / '취소' toggle that enters and leaves multi-select.
        librarySelectAction = new TextView(this);
        librarySelectAction.setTextSize(12.5f);
        librarySelectAction.setTypeface(Typeface.DEFAULT_BOLD);
        librarySelectAction.setTextColor(LIB_GREEN_DEEP);
        librarySelectAction.setPadding(dp(12), dp(2), dp(2), dp(2));
        librarySelectAction.setOnClickListener(v -> {
            if (selectMode) {
                exitSelectMode();
            } else {
                enterSelectMode(null);
            }
        });
        sect.addView(librarySelectAction);
        content.addView(sect);

        // The list lives in a container we refill on search/filter changes, so
        // the whole screen (and the keyboard) does not rebuild on each keystroke.
        libraryListContainer = new LinearLayout(this);
        libraryListContainer.setOrientation(LinearLayout.VERTICAL);
        content.addView(libraryListContainer);

        // The multi-select action bar is created here but pinned to the bottom
        // of the screen (added to root below the scroll, not inside it), so it
        // stays reachable without scrolling the list to its end. Shown only in
        // select mode.
        librarySelectBar = new LinearLayout(this);
        librarySelectBar.setOrientation(LinearLayout.HORIZONTAL);
        librarySelectBar.setGravity(android.view.Gravity.CENTER_VERTICAL);
        librarySelectBar.setVisibility(View.GONE);

        refreshLibraryList();

        ScrollView scroll = new ScrollView(this);
        scroll.setVerticalScrollBarEnabled(false);
        scroll.addView(content);
        root.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

        // Pinned under the scroll: it holds the bottom while the list moves
        // behind it. root already carries the navigation-bar inset, so this sits
        // just above it.
        LinearLayout.LayoutParams barParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        barParams.setMargins(dp(14), dp(6), dp(14), dp(12));
        root.addView(librarySelectBar, barParams);

        applyStatusBarInset(root);
        setContentView(root);
    }

    private LinearLayout.LayoutParams buttonParams(int leftMargin) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f);
        params.leftMargin = leftMargin;
        return params;
    }

    // --- gamepad key mapping ---------------------------------------------

    /** Whether the key-mapping screen is the one on show. */
    private boolean keyMapVisible;

    /** The copy that screen is editing, which the system back button leaves through. */
    private PadMapping editingMapping;

    /**
     * The named mappings, one of which `padMapping` is a copy of.
     *
     * <p>See {@link PadPresets}: the active preset and the live mapping are
     * kept saying the same thing, which is what makes `working.sameAs(padMapping)`
     * the answer to "has this screen been changed" here too.
     */
    private PadPresets padPresets;

    /**
     * The gamepad glyph on the home screen's entry button.
     *
     * <p>Drawn rather than shipped as an asset: it is one rounded body, a cross
     * and two dots, and a drawable file for that would be one more thing to
     * keep in step with the palette.
     */
    private final class PadGlyphView extends View {
        private final Paint stroke = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);

        PadGlyphView(Activity activity) {
            super(activity);
            stroke.setStyle(Paint.Style.STROKE);
            stroke.setColor(LIB_GREEN_DEEP);
            stroke.setStrokeCap(Paint.Cap.ROUND);
            fill.setColor(LIB_GREEN_DEEP);
        }

        @Override
        protected void onDraw(Canvas canvas) {
            float w = getWidth();
            float h = getHeight();
            float body = Math.min(w, h) * 0.62f;
            float tall = body * 0.62f;
            float cx = w / 2f;
            float cy = h / 2f;
            float thickness = Math.max(dp(1), body / 12f);

            stroke.setStrokeWidth(thickness);
            RectF shell = new RectF(cx - body / 2f, cy - tall / 2f, cx + body / 2f, cy + tall / 2f);
            canvas.drawRoundRect(shell, tall / 2f, tall / 2f, stroke);

            float arm = body / 9f;
            float dx = cx - body / 4f;
            canvas.drawLine(dx - arm, cy, dx + arm, cy, stroke);
            canvas.drawLine(dx, cy - arm, dx, cy + arm, stroke);

            float bx = cx + body / 4f;
            float dot = Math.max(dp(1), body / 13f);
            canvas.drawCircle(bx - arm, cy, dot, fill);
            canvas.drawCircle(bx + arm, cy, dot, fill);
        }
    }

    /** Opens the key mapping on a copy, so leaving it keeps what was saved. */
    private void showKeyMap() {
        showKeyMap(padMapping.copy());
    }

    private void showKeyMap(PadMapping working) {
        keyMapVisible = true;
        editingMapping = working;
        setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);
        setLightStatusBar(true);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(LIB_BG);

        TextView bar = new TextView(this);
        bar.setText("Mini Mobile");
        bar.setTextSize(16f);
        bar.setTypeface(Typeface.DEFAULT_BOLD);
        bar.setTextColor(LIB_INK);
        bar.setPadding(dp(18), dp(12), dp(18), dp(6));
        root.addView(bar);

        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(16), dp(4), dp(16), dp(24));

        LinearLayout titleRow = new LinearLayout(this);
        titleRow.setOrientation(LinearLayout.HORIZONTAL);
        titleRow.setGravity(android.view.Gravity.CENTER_VERTICAL);

        TextView back = new TextView(this);
        back.setText("‹");
        back.setTextSize(22f);
        back.setTypeface(Typeface.DEFAULT_BOLD);
        back.setTextColor(LIB_GREEN_DEEP);
        back.setGravity(android.view.Gravity.CENTER);
        back.setBackground(roundedRect(LIB_GREEN_SOFTER, LIB_LINE, 1, 12));
        back.setOnClickListener(v -> leaveKeyMap(working));
        titleRow.addView(back, new LinearLayout.LayoutParams(dp(38), dp(38)));

        TextView title = new TextView(this);
        title.setText("게임패드 키매핑");
        title.setTextSize(19f);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setTextColor(LIB_INK);
        title.setPadding(dp(12), 0, dp(8), 0);
        title.setMaxLines(1);
        title.setEllipsize(TextUtils.TruncateAt.END);
        // The title gives way, not the chip: a preset's name is the thing on
        // this row a player cannot work out from anywhere else.
        titleRow.addView(title, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        titleRow.addView(presetChip(working), new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, dp(40)));
        content.addView(titleRow);

        TextView pad = new TextView(this);
        pad.setText("연결된 패드: " + connectedPadName());
        pad.setTextSize(11.5f);
        pad.setTextColor(LIB_MUTED);
        pad.setPadding(0, dp(10), 0, 0);
        content.addView(pad);

        TextView how = new TextView(this);
        how.setText("키를 눌러 그 키를 누를 패드 버튼을 고릅니다.");
        how.setTextSize(11.5f);
        how.setTextColor(LIB_MUTED);
        how.setPadding(0, dp(2), 0, dp(14));
        content.addView(how);

        LinearLayout actions = new LinearLayout(this);
        Button reset = flatButton("기본값으로");
        reset.setOnClickListener(v -> {
            working.resetToDefaults();
            showKeyMap(working);
        });
        actions.addView(reset, buttonParams(0));

        Button save = flatButton("저장");
        save.setTextColor(LIB_BG);
        save.setBackground(roundedRect(LIB_GREEN, LIB_GREEN, 1, 15));
        save.setOnClickListener(v -> {
            // Both, always: the preset is what the screen was editing, and the
            // live mapping is what the runtime presses keys through.
            padMapping.copyFrom(working);
            padMapping.save(this);
            padPresets.active().mapping.copyFrom(working);
            padPresets.save(this);
            Toast.makeText(this, "‘" + padPresets.activeName() + "’에 저장했습니다.", Toast.LENGTH_SHORT).show();
            showLibrary();
        });
        actions.addView(save, buttonParams(dp(10)));
        content.addView(actions, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42)));

        for (int group = 0; group < KEY_GROUPS.length; group++) {
            TextView heading = new TextView(this);
            heading.setText(KEY_GROUP_TITLES[group]);
            heading.setTextSize(11.5f);
            heading.setTypeface(Typeface.DEFAULT_BOLD);
            heading.setTextColor(LIB_MUTED);
            heading.setPadding(dp(2), dp(16), 0, dp(8));
            content.addView(heading);

            LinearLayout card = new LinearLayout(this);
            card.setOrientation(LinearLayout.VERTICAL);
            card.setBackground(roundedRect(LIB_SURFACE, LIB_LINE, 1, 14));
            card.setPadding(0, dp(4), 0, dp(4));

            int[] codes = KEY_GROUPS[group];
            for (int index = 0; index < codes.length; index++) {
                card.addView(keyMapRow(working, codes[index]));
                if (index < codes.length - 1) {
                    View line = new View(this);
                    line.setBackgroundColor(LIB_DIVIDER);
                    LinearLayout.LayoutParams lineParams =
                            new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Math.max(1, dp(1) / 2));
                    lineParams.leftMargin = dp(12);
                    lineParams.rightMargin = dp(12);
                    card.addView(line, lineParams);
                }
            }

            content.addView(card);
        }

        ScrollView scroll = new ScrollView(this);
        scroll.setVerticalScrollBarEnabled(false);
        scroll.addView(content);
        root.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

        applyStatusBarInset(root);
        setContentView(root);
    }

    /** One handset key: its badge, its name, and the pad buttons pressing it. */
    private View keyMapRow(PadMapping working, int handsetKey) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(dp(12), dp(7), dp(12), dp(7));
        row.setOnClickListener(v -> showPadPicker(working, handsetKey));

        TextView badge = new TextView(this);
        badge.setText(KEY_BADGES[handsetKey]);
        badge.setTextSize(12f);
        badge.setTypeface(Typeface.DEFAULT_BOLD);
        badge.setTextColor(LIB_GREEN_DEEP);
        badge.setGravity(android.view.Gravity.CENTER);
        badge.setBackground(roundedRect(LIB_GREEN_SOFTER, LIB_LINE, 1, 9));
        row.addView(badge, new LinearLayout.LayoutParams(dp(32), dp(32)));

        TextView name = new TextView(this);
        name.setText(KEY_NAMES[handsetKey]);
        name.setTextSize(13.5f);
        name.setTextColor(LIB_INK);
        name.setPadding(dp(12), 0, dp(8), 0);
        row.addView(name, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        boolean assigned = !working.padsFor(handsetKey).isEmpty();
        TextView chip = new TextView(this);
        chip.setText(working.labelFor(handsetKey) + "  ▾");
        chip.setTextSize(12f);
        chip.setTypeface(assigned ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
        chip.setTextColor(assigned ? LIB_GREEN_DEEP : LIB_MUTED);
        chip.setGravity(android.view.Gravity.CENTER);
        chip.setMinWidth(dp(86));
        chip.setPadding(dp(10), dp(6), dp(10), dp(6));
        chip.setBackground(assigned
                ? roundedRect(LIB_GREEN_SOFT, LIB_GREEN_LINE, 1, 11)
                : roundedRect(LIB_SURFACE, LIB_LINE, 1, 11));
        row.addView(chip);

        return row;
    }

    /**
     * Picks the pad buttons that press one handset key.
     *
     * <p>Choosing a button that another key had takes it from that key, because
     * a button pressing two keys at once is not something a game asks for. The
     * small line under a button is the key it is on now, so what a choice costs
     * is readable before it is made.
     */
    private void showPadPicker(PadMapping working, int handsetKey) {
        LinearLayout sheet = new LinearLayout(this);
        sheet.setOrientation(LinearLayout.VERTICAL);
        sheet.setBackgroundColor(LIB_BG);
        sheet.setPadding(dp(20), dp(20), dp(20), dp(16));

        TextView title = new TextView(this);
        title.setText(KEY_NAMES[handsetKey] + " 키를 누를 패드 버튼");
        title.setTextSize(15f);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setTextColor(LIB_INK);
        title.setGravity(android.view.Gravity.CENTER);
        sheet.addView(title);

        TextView hint = new TextView(this);
        hint.setText("여러 개를 고를 수 있습니다");
        hint.setTextSize(10.5f);
        hint.setTextColor(LIB_MUTED);
        hint.setGravity(android.view.Gravity.CENTER);
        hint.setPadding(0, dp(4), 0, dp(14));
        sheet.addView(hint);

        final LinearLayout[] cells = new LinearLayout[PadMapping.PAD_COUNT];
        final TextView[] cellLabels = new TextView[PadMapping.PAD_COUNT];
        final TextView[] cellNotes = new TextView[PadMapping.PAD_COUNT];

        final int columns = 4;
        for (int first = 0; first < PadMapping.PAD_COUNT; first += columns) {
            LinearLayout rowView = new LinearLayout(this);
            rowView.setOrientation(LinearLayout.HORIZONTAL);

            for (int column = 0; column < columns; column++) {
                final int pad = first + column;
                LinearLayout.LayoutParams params =
                        new LinearLayout.LayoutParams(0, dp(58), 1f);
                params.leftMargin = column == 0 ? 0 : dp(8);
                params.bottomMargin = dp(8);

                if (pad >= PadMapping.PAD_COUNT) {
                    View filler = new View(this);
                    rowView.addView(filler, params);
                    continue;
                }

                LinearLayout cell = new LinearLayout(this);
                cell.setOrientation(LinearLayout.VERTICAL);
                cell.setGravity(android.view.Gravity.CENTER);

                TextView label = new TextView(this);
                label.setText(PadMapping.PAD_LABELS[pad]);
                label.setTextSize(PadMapping.PAD_LABELS[pad].length() > 2 ? 11f : 15f);
                label.setGravity(android.view.Gravity.CENTER);
                cell.addView(label);

                TextView note = new TextView(this);
                note.setTextSize(8.5f);
                note.setGravity(android.view.Gravity.CENTER);
                cell.addView(note);

                cells[pad] = cell;
                cellLabels[pad] = label;
                cellNotes[pad] = note;
                rowView.addView(cell, params);
            }

            sheet.addView(rowView);
        }

        final Runnable refresh = () -> {
            for (int pad = 0; pad < PadMapping.PAD_COUNT; pad++) {
                int on = working.handsetKeyOf(pad);
                boolean mine = on == handsetKey;

                cells[pad].setBackground(mine
                        ? roundedRect(LIB_GREEN_SOFT, LIB_GREEN, 2, 13)
                        : roundedRect(LIB_SURFACE, LIB_LINE, 1, 13));
                cellLabels[pad].setTextColor(mine ? LIB_GREEN_DEEP : LIB_INK);
                cellLabels[pad].setTypeface(mine ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);

                // Under the button: the key it is on now, or the warning a
                // button carries when the system may never hand it over.
                String under = mine ? KEY_BADGES[handsetKey]
                        : isHandsetKey(on) ? KEY_NAMES[on]
                        : PadMapping.PAD_NOTES[pad];
                cellNotes[pad].setText(under == null ? "" : under);
                cellNotes[pad].setTextColor(mine ? LIB_GREEN : LIB_MUTED);
                cellNotes[pad].setVisibility(under == null ? View.GONE : View.VISIBLE);
            }
        };

        for (int pad = 0; pad < PadMapping.PAD_COUNT; pad++) {
            final int which = pad;
            cells[pad].setOnClickListener(v -> {
                boolean mine = working.handsetKeyOf(which) == handsetKey;
                working.assign(which, mine ? PadMapping.UNASSIGNED : handsetKey);
                refresh.run();
            });
        }

        refresh.run();

        TextView moved = new TextView(this);
        moved.setText("작은 글씨 = 그 버튼이 지금 맡은 키. 고르면 이쪽으로 옮겨옵니다.\n"
                + "왼쪽 스틱은 언제나 방향키와 같이 동작합니다.");
        moved.setTextSize(10f);
        moved.setTextColor(LIB_MUTED);
        moved.setPadding(dp(2), dp(4), dp(2), dp(14));
        sheet.addView(moved);

        AlertDialog dialog = new AlertDialog.Builder(this).setView(sheet).create();

        Button done = flatButton("완료");
        done.setTextColor(LIB_BG);
        done.setBackground(roundedRect(LIB_GREEN, LIB_GREEN, 1, 15));
        done.setOnClickListener(v -> dialog.dismiss());
        sheet.addView(done, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42)));

        // The rows behind carry the pad buttons that were just moved about, so
        // the screen is built again rather than left saying what was true.
        dialog.setOnDismissListener(d -> showKeyMap(working));
        dialog.show();
    }

    /**
     * The chip that names the preset being edited, and opens the list.
     *
     * <p>Two lines rather than one: the name alone beside a title reads as part
     * of the title, and a chip that only says `기본 ▼` leaves a player to guess
     * what it is a chip of.
     */
    private View presetChip(PadMapping working) {
        LinearLayout chip = new LinearLayout(this);
        chip.setOrientation(LinearLayout.VERTICAL);
        chip.setGravity(android.view.Gravity.CENTER_VERTICAL);
        chip.setBackground(roundedRect(LIB_GREEN_SOFT, LIB_GREEN_LINE, 1, 12));
        chip.setPadding(dp(11), 0, dp(11), 0);
        chip.setContentDescription("프리셋 " + padPresets.activeName() + ", 누르면 목록");
        chip.setOnClickListener(v -> showPresetSheet(working));

        TextView tag = new TextView(this);
        tag.setText("프리셋");
        tag.setTextSize(8.5f);
        tag.setTextColor(LIB_MUTED);
        tag.setIncludeFontPadding(false);
        chip.addView(tag);

        LinearLayout nameRow = new LinearLayout(this);
        nameRow.setOrientation(LinearLayout.HORIZONTAL);
        nameRow.setGravity(android.view.Gravity.CENTER_VERTICAL);
        nameRow.setPadding(0, dp(2), 0, 0);

        TextView name = new TextView(this);
        name.setText(padPresets.activeName());
        name.setTextSize(13f);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setTextColor(LIB_GREEN_DEEP);
        name.setMaxLines(1);
        name.setEllipsize(TextUtils.TruncateAt.END);
        name.setMaxWidth(dp(92));
        name.setIncludeFontPadding(false);
        nameRow.addView(name);

        TextView caret = new TextView(this);
        caret.setText("▼");
        caret.setTextSize(7f);
        caret.setTextColor(LIB_GREEN_DEEP);
        caret.setPadding(dp(4), 0, 0, 0);
        caret.setIncludeFontPadding(false);
        nameRow.addView(caret);

        chip.addView(nameRow);

        return chip;
    }

    /** The presets, to pick one from, to rename one, or to keep this one. */
    private void showPresetSheet(PadMapping working) {
        LinearLayout sheet = new LinearLayout(this);
        sheet.setOrientation(LinearLayout.VERTICAL);
        sheet.setBackgroundColor(LIB_BG);
        sheet.setPadding(dp(20), dp(20), dp(20), dp(16));

        TextView title = new TextView(this);
        title.setText("프리셋");
        title.setTextSize(15f);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setTextColor(LIB_INK);
        title.setGravity(android.view.Gravity.CENTER);
        sheet.addView(title);

        TextView hint = new TextView(this);
        hint.setText("고르면 그 매핑을 불러옵니다. 이름은 연필을 눌러 바꿉니다.");
        hint.setTextSize(10.5f);
        hint.setTextColor(LIB_MUTED);
        hint.setGravity(android.view.Gravity.CENTER);
        hint.setPadding(0, dp(4), 0, dp(14));
        sheet.addView(hint);

        ScrollView scroll = new ScrollView(this);
        scroll.setVerticalScrollBarEnabled(false);
        scroll.addView(sheet);

        AlertDialog dialog = new AlertDialog.Builder(this).setView(scroll).create();

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setBackground(roundedRect(LIB_SURFACE, LIB_LINE, 1, 14));
        card.setPadding(0, dp(4), 0, dp(4));

        for (int index = 0; index < padPresets.size(); index++) {
            card.addView(presetRow(dialog, working, index));

            if (index < padPresets.size() - 1) {
                View line = new View(this);
                line.setBackgroundColor(LIB_DIVIDER);
                LinearLayout.LayoutParams lineParams =
                        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Math.max(1, dp(1) / 2));
                lineParams.leftMargin = dp(12);
                lineParams.rightMargin = dp(12);
                card.addView(line, lineParams);
            }
        }

        sheet.addView(card);

        Button add = flatButton("＋  지금 설정을 새 프리셋으로");
        if (padPresets.canAdd()) {
            add.setOnClickListener(v -> {
                padPresets.add(padPresets.suggestName(), working);
                padMapping.copyFrom(working);
                padMapping.save(this);
                padPresets.save(this);
                Toast.makeText(this, "‘" + padPresets.activeName() + "’을(를) 만들었습니다.", Toast.LENGTH_SHORT).show();
                dialog.dismiss();
            });
        } else {
            add.setEnabled(false);
            add.setTextColor(LIB_MUTED);
            add.setBackground(roundedRect(LIB_SURFACE, LIB_LINE, 1, 15));
        }

        LinearLayout.LayoutParams addParams =
                new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42));
        addParams.topMargin = dp(12);
        sheet.addView(add, addParams);

        TextView cap = new TextView(this);
        cap.setText("프리셋은 " + PadPresets.MAX_PRESETS + "개까지 만들 수 있습니다.");
        cap.setTextSize(10f);
        cap.setTextColor(LIB_MUTED);
        cap.setGravity(android.view.Gravity.CENTER);
        cap.setPadding(0, dp(10), 0, dp(14));
        sheet.addView(cap);

        Button done = flatButton("완료");
        done.setTextColor(LIB_BG);
        done.setBackground(roundedRect(LIB_GREEN, LIB_GREEN, 1, 15));
        done.setOnClickListener(v -> dialog.dismiss());
        sheet.addView(done, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(42)));

        // Whatever the sheet did - chose, made, renamed - the screen behind it
        // is saying something that was true before, so it is built again.
        dialog.setOnDismissListener(d -> showKeyMap(working));
        dialog.show();
    }

    /** One preset: whether it is the one in use, its name, and what it holds. */
    private View presetRow(AlertDialog sheet, PadMapping working, int index) {
        PadPresets.Preset preset = padPresets.at(index);
        boolean inUse = index == padPresets.activeIndex();

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(dp(12), dp(6), dp(8), dp(6));
        row.setOnClickListener(v -> choosePreset(sheet, working, index));

        TextView mark = new TextView(this);
        mark.setText(inUse ? "✓" : "");
        mark.setTextSize(11f);
        mark.setTypeface(Typeface.DEFAULT_BOLD);
        mark.setTextColor(LIB_BG);
        mark.setGravity(android.view.Gravity.CENTER);
        mark.setBackground(circle(inUse ? LIB_GREEN : LIB_DIVIDER));
        row.addView(mark, new LinearLayout.LayoutParams(dp(22), dp(22)));

        LinearLayout text = new LinearLayout(this);
        text.setOrientation(LinearLayout.VERTICAL);
        text.setPadding(dp(10), dp(2), dp(8), dp(2));

        TextView name = new TextView(this);
        name.setText(preset.name);
        name.setTextSize(14f);
        name.setTypeface(inUse ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
        name.setTextColor(inUse ? LIB_GREEN_DEEP : LIB_INK);
        name.setMaxLines(1);
        name.setEllipsize(TextUtils.TruncateAt.END);
        text.addView(name);

        TextView note = new TextView(this);
        note.setText("패드 버튼 " + preset.mapping.assignedCount() + "개 지정");
        note.setTextSize(10f);
        note.setTextColor(LIB_MUTED);
        note.setPadding(0, dp(2), 0, 0);
        text.addView(note);

        row.addView(text, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        TextView rename = new TextView(this);
        rename.setText("✎");
        rename.setTextSize(15f);
        rename.setTextColor(LIB_GREEN_DEEP);
        rename.setGravity(android.view.Gravity.CENTER);
        rename.setBackground(roundedRect(LIB_GREEN_SOFTER, LIB_LINE, 1, 10));
        rename.setContentDescription(preset.name + " 이름 바꾸기");
        rename.setOnClickListener(v -> {
            sheet.setOnDismissListener(null);
            sheet.dismiss();
            showPresetRename(working, index);
        });
        row.addView(rename, new LinearLayout.LayoutParams(dp(36), dp(36)));

        return row;
    }

    /** Puts the pad on another preset, asking first if it would drop a change. */
    private void choosePreset(AlertDialog sheet, PadMapping working, int index) {
        if (index == padPresets.activeIndex()) {
            sheet.dismiss();
            return;
        }

        if (working.sameAs(padMapping)) {
            loadPreset(sheet, working, index);
            return;
        }

        new AlertDialog.Builder(this)
                .setTitle("프리셋 바꾸기")
                .setMessage("바꾼 키매핑이 저장되지 않습니다.")
                .setPositiveButton("바꾸기", (dialog, which) -> loadPreset(sheet, working, index))
                .setNegativeButton("계속 편집", null)
                .show();
    }

    private void loadPreset(AlertDialog sheet, PadMapping working, int index) {
        padPresets.setActive(index);
        padMapping.copyFrom(padPresets.active().mapping);
        padMapping.save(this);
        padPresets.save(this);
        working.copyFrom(padMapping);

        Toast.makeText(this, "‘" + padPresets.activeName() + "’을(를) 불러왔습니다.", Toast.LENGTH_SHORT).show();
        sheet.dismiss();
    }

    /** The name of one preset, and the one place it can be deleted from. */
    private void showPresetRename(PadMapping working, int index) {
        if (!padPresets.holds(index)) {
            showKeyMap(working);
            return;
        }

        PadPresets.Preset preset = padPresets.at(index);

        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(22), dp(14), dp(22), dp(4));

        EditText field = new EditText(this);
        field.setText(preset.name);
        field.setSelection(field.getText().length());
        field.setSingleLine(true);
        field.setTextSize(15f);
        field.setTextColor(LIB_INK);
        field.setFilters(new InputFilter[]{new InputFilter.LengthFilter(PadPresets.MAX_NAME)});
        field.setBackground(roundedRect(LIB_BG, LIB_GREEN, 2, 13));
        field.setPadding(dp(13), dp(11), dp(13), dp(11));
        box.addView(field, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT));

        TextView cap = new TextView(this);
        cap.setText(PadPresets.MAX_NAME + "자까지");
        cap.setTextSize(10f);
        cap.setTextColor(LIB_MUTED);
        cap.setPadding(dp(2), dp(7), 0, 0);
        box.addView(cap);

        // Set while the delete confirmation is taking over, so the screen is
        // built once - by whichever dialog the player actually leaves through.
        final boolean[] handedOver = {false};

        AlertDialog.Builder builder = new AlertDialog.Builder(this)
                .setTitle("프리셋 이름")
                .setView(box)
                .setPositiveButton("저장", (dialog, which) -> {
                    preset.name = PadPresets.cleanName(field.getText().toString(), index);
                    padPresets.save(this);
                })
                .setNegativeButton("취소", null);

        if (padPresets.canRemove()) {
            builder.setNeutralButton("삭제", (dialog, which) -> {
                handedOver[0] = true;
                confirmPresetRemove(working, index);
            });
        }

        AlertDialog dialog = builder.create();
        dialog.setOnDismissListener(d -> {
            if (!handedOver[0]) {
                showKeyMap(working);
            }
        });
        dialog.show();
    }

    private void confirmPresetRemove(PadMapping working, int index) {
        if (!padPresets.holds(index) || !padPresets.canRemove()) {
            showKeyMap(working);
            return;
        }

        String name = padPresets.at(index).name;
        boolean inUse = index == padPresets.activeIndex();

        AlertDialog dialog = new AlertDialog.Builder(this)
                .setTitle("‘" + name + "’ 삭제")
                .setMessage("이 프리셋의 키매핑이 사라집니다.")
                .setPositiveButton("삭제", (d, which) -> {
                    padPresets.remove(index);

                    // Deleting the one in use leaves the pad on its neighbour,
                    // and the screen with it: nothing may be left editing a
                    // preset that is no longer there.
                    if (inUse) {
                        padMapping.copyFrom(padPresets.active().mapping);
                        padMapping.save(this);
                        working.copyFrom(padMapping);
                    }

                    padPresets.save(this);
                    Toast.makeText(this, "‘" + name + "’을(를) 지웠습니다.", Toast.LENGTH_SHORT).show();
                })
                .setNegativeButton("취소", null)
                .create();

        dialog.setOnDismissListener(d -> showKeyMap(working));
        dialog.show();
    }

    /** What the pad calls itself, or that there is none. */
    private String connectedPadName() {
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice device = InputDevice.getDevice(id);
            if (device == null) {
                continue;
            }

            int source = device.getSources();
            boolean isPad = (source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                    || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK;

            if (isPad && !device.isVirtual()) {
                return device.getName();
            }
        }

        return "없음 (지금 연결된 패드가 없습니다)";
    }

    /** Leaves the key mapping, asking first if it would drop a change. */
    private void leaveKeyMap(PadMapping working) {
        if (working.sameAs(padMapping)) {
            showLibrary();
            return;
        }

        new AlertDialog.Builder(this)
                .setTitle("저장하지 않고 나가기")
                .setMessage("바꾼 키매핑이 저장되지 않습니다.")
                .setPositiveButton("나가기", (dialog, which) -> showLibrary())
                .setNegativeButton("계속 편집", null)
                .show();
    }

    /**
     * Refills the game list for the current search text and carrier filter, and
     * rebuilds the carrier chips with live counts. Called on each keystroke and
     * chip tap, so it only touches the list container - not the whole screen -
     * to keep the search field and its keyboard alive.
     */
    private void refreshLibraryList() {
        if (libraryListContainer == null) {
            return;
        }

        File[] all = gamesDir.listFiles(File::isFile);
        if (all == null) {
            all = new File[0];
        }
        Arrays.sort(all, Comparator.comparing(File::getName, String.CASE_INSENSITIVE_ORDER));

        int skt = 0, ktf = 0, lgt = 0, etc = 0, fav = 0;
        for (File game : all) {
            switch (carrierBucket(game)) {
                case "SKT": skt++; break;
                case "KTF": ktf++; break;
                case "LGT": lgt++; break;
                default: etc++; break;
            }
            if (isFavorite(game)) {
                fav++;
            }
        }
        rebuildCarrierChips(all.length, fav, skt, ktf, lgt, etc);

        String query = librarySearch.toLowerCase(java.util.Locale.ROOT);
        ArrayList<File> shown = new ArrayList<>();
        for (File game : all) {
            if (libraryFavOnly && !isFavorite(game)) {
                continue;
            }
            // DRM downloads are counted and filtered under 기타 (the count
            // switch above defaults them there), though their badge stays the
            // distinct red "DRM".
            if (!libraryCarrier.isEmpty() && !filterBucket(game).equals(libraryCarrier)) {
                continue;
            }
            if (!query.isEmpty() && !displayName(game).toLowerCase(java.util.Locale.ROOT).contains(query)) {
                continue;
            }
            shown.add(game);
        }
        libraryShown = shown;

        // Drop any selection that filtering has hidden, so 삭제 only ever acts on
        // what is on screen.
        selected.retainAll(pathsOf(shown));

        libraryListContainer.removeAllViews();
        if (shown.isEmpty() && all.length > 0) {
            // The library has games, but none match the filter - a different
            // empty state from the "import your first game" one.
            libraryListContainer.addView(buildNoMatchState());
        } else {
            // Favourites float to the top as their own group, unless we are
            // already showing only favourites (the ⭐ chip) or there is no mix.
            ArrayList<File> favs = new ArrayList<>();
            ArrayList<File> rest = new ArrayList<>();
            for (File game : shown) {
                (isFavorite(game) ? favs : rest).add(game);
            }
            if (!libraryFavOnly && !favs.isEmpty() && !rest.isEmpty()) {
                libraryListContainer.addView(listSubheader("★  즐겨찾기", LIB_STAR_INK));
                libraryListContainer.addView(buildGameList(favs.toArray(new File[0])));
                libraryListContainer.addView(listSubheader("그 외 게임", LIB_MUTED));
                libraryListContainer.addView(buildGameList(rest.toArray(new File[0])));
            } else {
                libraryListContainer.addView(buildGameList(shown.toArray(new File[0])));
            }
        }

        updateSelectionUi();
    }

    /** A small group heading inside the list container. */
    private TextView listSubheader(String text, int color) {
        TextView header = new TextView(this);
        header.setText(text);
        header.setTextSize(11.5f);
        header.setTypeface(Typeface.DEFAULT_BOLD);
        header.setTextColor(color);
        header.setPadding(dp(4), dp(12), dp(4), dp(6));
        return header;
    }

    // --- favourites ------------------------------------------------------

    private String favoriteKey(File game) {
        return game.getName();
    }

    private boolean isFavorite(File game) {
        return favorites.contains(favoriteKey(game));
    }

    private void toggleFavorite(File game) {
        String key = favoriteKey(game);
        if (!favorites.remove(key)) {
            favorites.add(key);
        }
        getSharedPreferences("mini_ui", MODE_PRIVATE).edit()
                .putStringSet("favorites", new java.util.HashSet<>(favorites)).apply();
        refreshLibraryList();
    }

    private java.util.HashSet<String> pathsOf(java.util.List<File> files) {
        java.util.HashSet<String> paths = new java.util.HashSet<>();
        for (File file : files) {
            paths.add(file.getAbsolutePath());
        }
        return paths;
    }

    // --- multi-select delete ---------------------------------------------

    private void enterSelectMode(File preselect) {
        selectMode = true;
        if (preselect != null) {
            selected.add(preselect.getAbsolutePath());
        }
        refreshLibraryList();
    }

    private void exitSelectMode() {
        selectMode = false;
        selected.clear();
        refreshLibraryList();
    }

    private void toggleSelect(File game) {
        String path = game.getAbsolutePath();
        if (!selected.remove(path)) {
            selected.add(path);
        }
        refreshLibraryList();
    }

    /** Updates the section count, the 선택/취소 toggle, and the action bar. */
    private void updateSelectionUi() {
        int shownCount = libraryShown.size();
        if (librarySectTitle != null) {
            librarySectTitle.setText(libraryFavOnly ? "즐겨찾기" : "게임 목록");
        }
        if (librarySectCount != null) {
            librarySectCount.setText(selectMode
                    ? (shownCount + "개 중 " + selected.size() + "개 선택")
                    : (shownCount + "개"));
        }
        if (librarySelectAction != null) {
            librarySelectAction.setText(selectMode ? "취소" : "선택");
            // Nothing to select when the (filtered) list is empty.
            librarySelectAction.setVisibility(!selectMode && shownCount == 0 ? View.GONE : View.VISIBLE);
        }
        if (librarySelectBar == null) {
            return;
        }
        librarySelectBar.removeAllViews();
        if (!selectMode) {
            librarySelectBar.setVisibility(View.GONE);
            return;
        }
        librarySelectBar.setVisibility(View.VISIBLE);
        librarySelectBar.setBackground(roundedRect(LIB_SURFACE, LIB_LINE, 1, 14));
        librarySelectBar.setPadding(dp(14), dp(9), dp(12), dp(9));

        long totalBytes = 0;
        for (String path : selected) {
            totalBytes += new File(path).length();
        }
        TextView count = new TextView(this);
        count.setTextColor(LIB_INK);
        count.setTextSize(13.5f);
        count.setTypeface(Typeface.DEFAULT_BOLD);
        count.setText(selected.isEmpty() ? "게임 선택" : (selected.size() + "개 선택 · " + formatSize(totalBytes)));
        librarySelectBar.addView(count, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        // ⋯ opens the actions menu. One game keeps its full per-game menu
        // (which also has the per-game 세이브 불러오기·삭제); two or more open the
        // batch menu, each action applying to every selected game at once.
        if (!selected.isEmpty()) {
            TextView more = new TextView(this);
            more.setText("⋯");
            styleSelectButton(more, false);
            if (selected.size() == 1) {
                File one = selectedGames().get(0);
                more.setOnClickListener(v -> showGameMenu(one));
            } else {
                more.setOnClickListener(v -> showBatchMenu());
            }
            LinearLayout.LayoutParams moreParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            moreParams.rightMargin = dp(8);
            librarySelectBar.addView(more, moreParams);
        }

        boolean allSelected = shownCount > 0 && selected.size() >= shownCount;
        TextView all = new TextView(this);
        all.setText(allSelected ? "해제" : "전체");
        styleSelectButton(all, false);
        all.setOnClickListener(v -> {
            if (allSelected) {
                selected.clear();
            } else {
                selected.addAll(pathsOf(libraryShown));
            }
            refreshLibraryList();
        });
        LinearLayout.LayoutParams allParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        allParams.rightMargin = dp(8);
        librarySelectBar.addView(all, allParams);

        TextView delete = new TextView(this);
        delete.setText("삭제");
        styleSelectButton(delete, true);
        boolean any = !selected.isEmpty();
        delete.setAlpha(any ? 1f : 0.4f);
        delete.setOnClickListener(v -> {
            if (any) {
                confirmDeleteSelected();
            }
        });
        librarySelectBar.addView(delete);
    }

    /** The selected games, in the order they appear in the (filtered) list. */
    private ArrayList<File> selectedGames() {
        ArrayList<File> games = new ArrayList<>();
        for (File game : libraryShown) {
            if (selected.contains(game.getAbsolutePath())) {
                games.add(game);
            }
        }
        return games;
    }

    private void styleSelectButton(TextView button, boolean danger) {
        button.setTextSize(13f);
        button.setTypeface(Typeface.DEFAULT_BOLD);
        button.setGravity(android.view.Gravity.CENTER);
        button.setPadding(dp(14), dp(8), dp(14), dp(8));
        if (danger) {
            button.setTextColor(Color.WHITE);
            button.setBackground(roundedRect(LIB_DELETE, 0, 0, 10));
        } else {
            button.setTextColor(LIB_GREEN_DEEP);
            button.setBackground(roundedRect(LIB_GREEN_SOFT, LIB_GREEN_LINE, 1, 10));
        }
    }

    private void confirmDeleteSelected() {
        ArrayList<File> targets = selectedGames();
        if (targets.isEmpty()) {
            return;
        }

        new AlertDialog.Builder(this)
                .setTitle(targets.size() + "개 게임을 삭제할까요?")
                .setMessage("목록에서 삭제됩니다. 저장한 내용은 그대로 남습니다.\n\n" + batchNames(targets))
                .setNegativeButton("취소", null)
                .setPositiveButton("삭제", (dialog, which) -> {
                    int removed = 0;
                    for (File game : targets) {
                        if (game.delete()) {
                            removed++;
                        }
                    }
                    Toast.makeText(this, removed + "개 삭제됨", Toast.LENGTH_SHORT).show();
                    selectMode = false;
                    selected.clear();
                    showLibrary();
                })
                .show();
    }

    /** Rebuilds the carrier filter chips with the given per-bucket counts. */
    private void rebuildCarrierChips(int total, int fav, int skt, int ktf, int lgt, int etc) {
        if (libraryChipRow == null) {
            return;
        }
        libraryChipRow.removeAllViews();
        libraryChipRow.addView(carrierChip("전체", total, "", 0));
        // The ⭐ chip toggles favourites-only, and shows only when some exist.
        if (fav > 0) {
            libraryChipRow.addView(favoriteChip(fav));
        }
        if (skt > 0) {
            libraryChipRow.addView(carrierChip("SKT", skt, "SKT", CARRIER_SKT[0]));
        }
        if (ktf > 0) {
            libraryChipRow.addView(carrierChip("KTF", ktf, "KTF", CARRIER_KTF[0]));
        }
        if (lgt > 0) {
            libraryChipRow.addView(carrierChip("LGT", lgt, "LGT", CARRIER_LGT[0]));
        }
        if (etc > 0) {
            libraryChipRow.addView(carrierChip("기타", etc, "ETC", CARRIER_ETC[0]));
        }
    }

    /** One carrier filter chip; `dotColor` 0 means no colour dot (전체). */
    private View carrierChip(String label, int count, String value, int dotColor) {
        boolean selected = libraryCarrier.equals(value);

        LinearLayout chip = new LinearLayout(this);
        chip.setOrientation(LinearLayout.HORIZONTAL);
        chip.setGravity(android.view.Gravity.CENTER_VERTICAL);
        chip.setPadding(dp(11), dp(6), dp(11), dp(6));
        chip.setBackground(roundedRect(
                selected ? LIB_GREEN_SOFT : LIB_BG,
                selected ? LIB_GREEN_LINE : LIB_LINE, 1, 999));

        if (dotColor != 0) {
            View dot = new View(this);
            dot.setBackground(circle(dotColor));
            LinearLayout.LayoutParams dotParams = new LinearLayout.LayoutParams(dp(8), dp(8));
            dotParams.rightMargin = dp(6);
            chip.addView(dot, dotParams);
        }

        TextView text = new TextView(this);
        text.setText(label + "  " + count);
        text.setTextSize(12f);
        text.setTypeface(Typeface.DEFAULT_BOLD);
        text.setTextColor(selected ? LIB_GREEN_DEEP : LIB_MUTED);
        chip.addView(text);

        chip.setOnClickListener(v -> {
            libraryCarrier = value;
            refreshLibraryList();
        });

        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        params.rightMargin = dp(7);
        chip.setLayoutParams(params);
        return chip;
    }

    /** The ⭐ chip: an independent toggle that narrows the list to favourites. */
    private View favoriteChip(int count) {
        boolean on = libraryFavOnly;

        LinearLayout chip = new LinearLayout(this);
        chip.setOrientation(LinearLayout.HORIZONTAL);
        chip.setGravity(android.view.Gravity.CENTER_VERTICAL);
        chip.setPadding(dp(11), dp(6), dp(11), dp(6));
        chip.setBackground(roundedRect(on ? LIB_STAR_SOFT : LIB_BG, on ? LIB_STAR_LINE : LIB_LINE, 1, 999));

        TextView text = new TextView(this);
        text.setText("⭐ 즐겨찾기  " + count);
        text.setTextSize(12f);
        text.setTypeface(Typeface.DEFAULT_BOLD);
        text.setTextColor(on ? LIB_STAR_INK : LIB_MUTED);
        chip.addView(text);

        chip.setOnClickListener(v -> {
            libraryFavOnly = !libraryFavOnly;
            refreshLibraryList();
        });

        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        params.rightMargin = dp(7);
        chip.setLayoutParams(params);
        return chip;
    }

    /** Empty state shown when a search or filter matches nothing. */
    private View buildNoMatchState() {
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setGravity(android.view.Gravity.CENTER_HORIZONTAL);
        box.setPadding(dp(16), dp(36), dp(16), dp(36));

        TextView et = new TextView(this);
        et.setText("검색 결과가 없어요");
        et.setTextSize(15f);
        et.setTypeface(Typeface.DEFAULT_BOLD);
        et.setTextColor(LIB_INK);
        et.setGravity(android.view.Gravity.CENTER);
        box.addView(et);

        TextView es = new TextView(this);
        es.setText("다른 검색어나 통신사를 눌러보세요.");
        es.setTextSize(12.5f);
        es.setTextColor(LIB_MUTED);
        es.setGravity(android.view.Gravity.CENTER);
        es.setPadding(0, dp(6), 0, 0);
        box.addView(es);

        return box;
    }

    /** Badge colours {ink, soft, line} for a carrier bucket. */
    private int[] carrierColors(String bucket) {
        switch (bucket) {
            case "SKT": return CARRIER_SKT;
            case "KTF": return CARRIER_KTF;
            case "LGT": return CARRIER_LGT;
            case "DRM": return CARRIER_DRM;
            default: return CARRIER_ETC;
        }
    }

    /**
     * Which carrier bucket a game falls in: "SKT", "KTF", "LGT", or "ETC" for
     * anything the native detector does not claim. The native result is cached
     * by name+size+mtime so the list does not re-read the file on each refresh.
     */
    private String carrierBucket(File game) {
        String key = game.getName() + ":" + game.length() + ":" + game.lastModified();
        String cached = carrierCache.get(key);
        if (cached != null) {
            return cached;
        }

        String bucket = "ETC";
        try {
            byte[] data = readGameBytes(game);
            String carrier = NativeBridge.nativeDetectCarrier(data);
            if (carrier != null && !carrier.isEmpty()) {
                bucket = carrier;
            }
        } catch (Throwable ignored) {
            // Unreadable or unrecognised: treated as 기타.
        }

        carrierCache.put(key, bucket);
        return bucket;
    }

    /**
     * The bucket a game filters and counts under, which folds DRM into 기타 so a
     * DRM download appears under the 기타 chip it was counted in. The badge uses
     * the raw {@link #carrierBucket} so it still shows the distinct red "DRM".
     */
    private String filterBucket(File game) {
        String bucket = carrierBucket(game);
        return bucket.equals("DRM") ? "ETC" : bucket;
    }

    /** The archive's cover icon, cached by name+size+mtime (null = no icon). */
    private Bitmap cachedIcon(File game) {
        String key = game.getName() + ":" + game.length() + ":" + game.lastModified();
        if (iconCache.containsKey(key)) {
            return iconCache.get(key);
        }
        Bitmap bitmap = readArchiveIcon(game);
        iconCache.put(key, bitmap);
        return bitmap;
    }

    /** Reads a game archive fully into memory (games are small - a few hundred KB). */
    private byte[] readGameBytes(File game) throws java.io.IOException {
        try (FileInputStream input = new FileInputStream(game)) {
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream((int) Math.max(1, game.length()));
            byte[] chunk = new byte[8192];
            int read;
            while ((read = input.read(chunk)) != -1) {
                out.write(chunk, 0, read);
            }
            return out.toByteArray();
        }
    }

    /** The rounded list card, or the empty state when nothing is imported. */
    private View buildGameList(File[] games) {
        if (games == null || games.length == 0) {
            return buildEmptyState();
        }

        Arrays.sort(games, Comparator.comparing(File::getName, String.CASE_INSENSITIVE_ORDER));

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setBackground(roundedRect(LIB_SURFACE, LIB_LINE, 1, 16));
        roundCorners(card, 16);

        for (int i = 0; i < games.length; i++) {
            card.addView(createGameRow(games[i]));
            if (i < games.length - 1) {
                View divider = new View(this);
                divider.setBackgroundColor(LIB_DIVIDER);
                card.addView(divider, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Math.max(1, dp(1))));
            }
        }
        return card;
    }

    private View buildEmptyState() {
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setGravity(android.view.Gravity.CENTER_HORIZONTAL);
        box.setPadding(dp(16), dp(44), dp(16), dp(44));

        TextView tile = new TextView(this);
        tile.setText("+");
        tile.setTextSize(30f);
        tile.setTypeface(Typeface.DEFAULT_BOLD);
        tile.setTextColor(LIB_GREEN);
        tile.setGravity(android.view.Gravity.CENTER);
        tile.setBackground(roundedRect(LIB_GREEN_SOFTER, 0, 0, 20));
        box.addView(tile, new LinearLayout.LayoutParams(dp(62), dp(62)));

        TextView et = new TextView(this);
        et.setText("아직 게임이 없어요");
        et.setTextSize(15f);
        et.setTypeface(Typeface.DEFAULT_BOLD);
        et.setTextColor(LIB_INK);
        et.setGravity(android.view.Gravity.CENTER);
        et.setPadding(0, dp(12), 0, 0);
        box.addView(et);

        TextView es = new TextView(this);
        es.setText("위의 ‘ZIP 가져오기’로 게임을 추가하세요.");
        es.setTextSize(12.5f);
        es.setTextColor(LIB_MUTED);
        es.setGravity(android.view.Gravity.CENTER);
        es.setPadding(0, dp(6), 0, 0);
        box.addView(es);

        return box;
    }

    private View createGameRow(File game) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(dp(12), dp(10), dp(12), dp(10));

        boolean isSelected = selected.contains(game.getAbsolutePath());
        if (selectMode) {
            row.setBackgroundColor(isSelected ? LIB_SELECT_BG : Color.TRANSPARENT);

            // A checkbox leads the row; ticked ones fill green.
            TextView check = new TextView(this);
            check.setText(isSelected ? "✓" : "");
            check.setTextColor(Color.WHITE);
            check.setTextSize(13f);
            check.setTypeface(Typeface.DEFAULT_BOLD);
            check.setGravity(android.view.Gravity.CENTER);
            check.setBackground(roundedRect(isSelected ? LIB_GREEN : LIB_BG, isSelected ? LIB_GREEN : Color.rgb(205, 216, 209), isSelected ? 0 : 2, 6));
            LinearLayout.LayoutParams checkParams = new LinearLayout.LayoutParams(dp(22), dp(22));
            checkParams.rightMargin = dp(10);
            row.addView(check, checkParams);
        }

        // Cover: the archive's own icon if it carries one, otherwise a colour
        // tile with the title's first character.
        Bitmap bitmap = cachedIcon(game);
        View cover;
        if (bitmap != null) {
            ImageView icon = new ImageView(this);
            icon.setImageBitmap(bitmap);
            icon.setScaleType(ImageView.ScaleType.CENTER_CROP);
            roundCorners(icon, 11);
            cover = icon;
        } else {
            String label = displayName(game);
            TextView tile = new TextView(this);
            tile.setText(label.isEmpty() ? "?" : label.substring(0, 1));
            tile.setTextColor(Color.WHITE);
            tile.setTextSize(18f);
            tile.setTypeface(Typeface.DEFAULT_BOLD);
            tile.setGravity(android.view.Gravity.CENTER);
            tile.setBackground(roundedRect(colorForName(game.getName()), 0, 0, 11));
            cover = tile;
        }
        row.addView(cover, new LinearLayout.LayoutParams(dp(42), dp(42)));

        // Title, then a tag chip plus the file size.
        LinearLayout meta = new LinearLayout(this);
        meta.setOrientation(LinearLayout.VERTICAL);
        LinearLayout.LayoutParams metaParams = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
        metaParams.leftMargin = dp(12);
        metaParams.rightMargin = dp(10);

        TextView name = new TextView(this);
        name.setText(displayName(game));
        name.setTextColor(LIB_INK);
        name.setTextSize(14f);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setSingleLine(true);
        name.setEllipsize(android.text.TextUtils.TruncateAt.END);
        meta.addView(name);

        LinearLayout metaLine = new LinearLayout(this);
        metaLine.setOrientation(LinearLayout.HORIZONTAL);
        metaLine.setGravity(android.view.Gravity.CENTER_VERTICAL);
        metaLine.setPadding(0, dp(2), 0, 0);

        // Carrier badge in place of the old file-extension tag.
        String bucket = carrierBucket(game);
        int[] colors = carrierColors(bucket);
        TextView tag = new TextView(this);
        tag.setText(bucket.equals("ETC") ? "기타" : bucket);
        tag.setTextSize(10f);
        tag.setTypeface(Typeface.DEFAULT_BOLD);
        tag.setTextColor(colors[0]);
        tag.setBackground(roundedRect(colors[1], colors[2], 1, 6));
        tag.setPadding(dp(6), dp(1), dp(6), dp(1));
        LinearLayout.LayoutParams tagParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        tagParams.rightMargin = dp(6);
        metaLine.addView(tag, tagParams);

        TextView size = new TextView(this);
        size.setText(formatSize(game.length()));
        size.setTextSize(11.5f);
        size.setTextColor(LIB_MUTED);
        metaLine.addView(size);

        meta.addView(metaLine);
        row.addView(meta, metaParams);

        // A favourite star then the play affordance on the right - only when
        // not selecting, where a row tap toggles the checkbox instead.
        if (!selectMode) {
            boolean fav = isFavorite(game);
            TextView star = new TextView(this);
            star.setText(fav ? "★" : "☆");
            star.setTextSize(16f);
            star.setTextColor(fav ? LIB_STAR : LIB_STAR_OFF);
            star.setGravity(android.view.Gravity.CENTER);
            star.setOnClickListener(v -> toggleFavorite(game));
            LinearLayout.LayoutParams starParams = new LinearLayout.LayoutParams(dp(30), dp(30));
            starParams.rightMargin = dp(2);
            row.addView(star, starParams);

            TextView play = new TextView(this);
            play.setText("▶");
            play.setTextSize(11f);
            play.setTextColor(LIB_GREEN);
            play.setGravity(android.view.Gravity.CENTER);
            play.setBackground(circle(LIB_GREEN_SOFT));
            row.addView(play, new LinearLayout.LayoutParams(dp(29), dp(29)));
        }

        if (selectMode) {
            row.setOnClickListener(v -> toggleSelect(game));
        } else {
            row.setOnClickListener(v -> showPlayer(game));
            // Long-press starts multi-select with this game already ticked.
            row.setOnLongClickListener(v -> {
                enterSelectMode(game);
                return true;
            });
        }

        return row;
    }

    private String formatSize(long bytes) {
        if (bytes >= 1024L * 1024L) {
            return String.format(java.util.Locale.ROOT, "%.1f MB", bytes / (1024.0 * 1024.0));
        }
        return String.format(java.util.Locale.ROOT, "%.0f KB", Math.max(1.0, bytes / 1024.0));
    }

    /** A filled, optionally bordered, rounded-rectangle background. */
    private GradientDrawable roundedRect(int fill, int stroke, int strokeDp, int radiusDp) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setColor(fill);
        drawable.setCornerRadius(dp(radiusDp));
        if (strokeDp > 0) {
            drawable.setStroke(dp(strokeDp), stroke);
        }
        return drawable;
    }

    private GradientDrawable circle(int fill) {
        GradientDrawable drawable = new GradientDrawable();
        drawable.setShape(GradientDrawable.OVAL);
        drawable.setColor(fill);
        return drawable;
    }

    /** Clips a view to rounded corners so bitmaps and rows follow the card. */
    private void roundCorners(View view, int radiusDp) {
        final float radius = dp(radiusDp);
        view.setClipToOutline(true);
        view.setOutlineProvider(new ViewOutlineProvider() {
            @Override
            public void getOutline(View v, Outline outline) {
                outline.setRoundRect(0, 0, v.getWidth(), v.getHeight(), radius);
            }
        });
    }

    /** Dark status-bar icons for the light home screen; light for the player. */
    private void setLightStatusBar(boolean light) {
        View decor = getWindow().getDecorView();
        int flags = decor.getSystemUiVisibility();
        if (light) {
            flags |= View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR;
        } else {
            flags &= ~View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR;
        }
        decor.setSystemUiVisibility(flags);
        getWindow().setStatusBarColor(light ? LIB_BG : COLOR_PANEL);
    }

    /** The status- and navigation-bar flags immersive mode owns. */
    private static final int IMMERSIVE_FLAGS =
            View.SYSTEM_UI_FLAG_FULLSCREEN
            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
            | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION;

    /**
     * Sticky immersive while the player is up: the status and navigation bars
     * hide so the game and keypad have the whole screen, and a swipe in from an
     * edge brings them back translucent for a moment before they slide away
     * again. Off for the library, which is an ordinary screen with its bars.
     *
     * Only the immersive flags are touched, so the light/dark status-bar icon
     * choice {@link #setLightStatusBar} makes is left as it is. The system
     * clears sticky immersive whenever the window loses and regains focus - a
     * dialog, the shade, coming back from the background - so
     * {@link #onWindowFocusChanged} re-applies it.
     */
    private void applyImmersive(boolean on) {
        View decor = getWindow().getDecorView();
        int flags = decor.getSystemUiVisibility();
        if (on) {
            flags |= IMMERSIVE_FLAGS;
        } else {
            flags &= ~IMMERSIVE_FLAGS;
        }
        decor.setSystemUiVisibility(flags);
    }

    /** What a long press offers: move the saves about, or drop the game. */
    private void showGameMenu(File game) {
        // The two actions shared with the batch menu (📁 폴더 내보내기, 📥 폴더
        // 불러오기, 🗑 초기화) keep the same icons there; the save-zip pair and
        // 목록에서 삭제 get their own so nothing collides.
        MenuItem[] items = {
                new MenuItem("📤", "세이브 파일 꺼내기 (.zip)", "", "세이브를 .zip 으로 백업", false),
                new MenuItem("📦", "세이브 불러오기 (.zip)", "", ".zip 세이브를 되돌리기", false),
                new MenuItem("📁", "데이터 폴더로 내보내기", "", "다운로드/Mini Mobile 폴더로", false),
                new MenuItem("📥", "데이터 폴더에서 불러오기", "", "폴더의 내용을 게임에 적용", false),
                new MenuItem("🗑", "게임 데이터 초기화", "", "세이브·기록을 모두 삭제", true),
                new MenuItem("✕", "목록에서 삭제", "", "목록에서만 삭제 (세이브는 유지)", true),
        };
        lightAlert()
                .setTitle(displayName(game))
                .setAdapter(menuAdapter(items), (dialog, which) -> {
                    if (which == 0) {
                        exportSaves(game);
                    } else if (which == 1) {
                        importSaves(game);
                    } else if (which == 2) {
                        exportDataFolder(game);
                    } else if (which == 3) {
                        restoreDataFolder(game);
                    } else if (which == 4) {
                        confirmErase(game);
                    } else {
                        confirmDelete(game);
                    }
                })
                .setNegativeButton("취소", null)
                .show();
    }

    /**
     * The batch menu shown from ⋯ in select mode: each action runs over every
     * selected game at once and reports a single summary. Save import (one .zip
     * routed by id into one game) stays per-game and is not offered here.
     */
    private void showBatchMenu() {
        ArrayList<File> games = selectedGames();
        if (games.isEmpty()) {
            return;
        }
        String n = "(" + games.size() + "개)";
        MenuItem[] items = {
                new MenuItem("📤", "세이브 파일 꺼내기", n, "각 게임의 .zip 을 세이브 폴더로", false),
                new MenuItem("📁", "데이터 폴더로 내보내기", n, "다운로드/Mini Mobile/게임 이름 에 저장", false),
                new MenuItem("📥", "데이터 폴더에서 불러오기", n, "각 폴더의 내용을 게임에 적용", false),
                new MenuItem("🗑", "게임 데이터 초기화", n, "세이브·기록을 모두 삭제", true),
        };
        lightAlert()
                .setTitle(games.size() + "개 게임에 적용")
                .setAdapter(menuAdapter(items), (dialog, which) -> {
                    if (which == 0) {
                        batchExportSaves(games);
                    } else if (which == 1) {
                        batchExportDataFolder(games);
                    } else if (which == 2) {
                        confirmBatchRestore(games);
                    } else {
                        confirmBatchErase(games);
                    }
                })
                .setNegativeButton("취소", null)
                .show();
    }

    /**
     * One icon row for a game-actions menu: an emoji in a tinted badge, a bold
     * title, an optional count in the accent colour (the batch menu's "(N개)"),
     * and a one-line subtitle. Shared by the batch menu and the single-game menu.
     */
    private static final class MenuItem {
        final String emoji;
        final String title;
        final String count;    // "" for the single-game menu
        final String subtitle;
        final boolean danger;

        MenuItem(String emoji, String title, String count, String subtitle, boolean danger) {
            this.emoji = emoji;
            this.title = title;
            this.count = count;
            this.subtitle = subtitle;
            this.danger = danger;
        }
    }

    /**
     * An {@link AlertDialog.Builder} forced to the light dialog theme, so the
     * icon-row menus keep their white sheet and dark text stay readable even
     * when the device (and the default dialog theme) is dark - which left the
     * ink titles and grey subtitles all but invisible.
     */
    private AlertDialog.Builder lightAlert() {
        return new AlertDialog.Builder(new android.view.ContextThemeWrapper(this, android.R.style.Theme_Material_Light_Dialog_Alert));
    }

    /** An {@link ArrayAdapter} that renders each {@link MenuItem} via {@link #menuItemView}. */
    private ArrayAdapter<MenuItem> menuAdapter(MenuItem[] items) {
        return new ArrayAdapter<MenuItem>(this, 0, items) {
            @Override
            public View getView(int position, View convertView, ViewGroup parent) {
                return menuItemView(getItem(position));
            }
        };
    }

    /** Builds the view for one {@link MenuItem}, matching the library palette. */
    private View menuItemView(MenuItem item) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(dp(18), dp(11), dp(18), dp(11));

        TextView icon = new TextView(this);
        icon.setText(item.emoji);
        icon.setTextSize(15f);
        icon.setGravity(android.view.Gravity.CENTER);
        icon.setBackground(roundedRect(item.danger ? LIB_RED_SOFT : LIB_GREEN_SOFTER, 0, 0, 9));
        LinearLayout.LayoutParams iconParams = new LinearLayout.LayoutParams(dp(34), dp(34));
        iconParams.rightMargin = dp(12);
        row.addView(icon, iconParams);

        LinearLayout text = new LinearLayout(this);
        text.setOrientation(LinearLayout.VERTICAL);

        TextView title = new TextView(this);
        title.setTextSize(15f);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setTextColor(LIB_INK);
        if (item.count == null || item.count.isEmpty()) {
            title.setText(item.title);
        } else {
            // The title stays ink; only the "(N개)" count takes the accent colour.
            String full = item.title + " " + item.count;
            SpannableString span = new SpannableString(full);
            span.setSpan(new ForegroundColorSpan(item.danger ? LIB_DELETE : LIB_GREEN_DEEP),
                    item.title.length() + 1, full.length(), Spannable.SPAN_EXCLUSIVE_EXCLUSIVE);
            title.setText(span);
        }
        text.addView(title);

        TextView sub = new TextView(this);
        sub.setText(item.subtitle);
        sub.setTextSize(11.5f);
        sub.setTextColor(LIB_MUTED);
        LinearLayout.LayoutParams subParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        subParams.topMargin = dp(1);
        text.addView(sub, subParams);

        row.addView(text, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        return row;
    }

    /** A bulleted list of the games, capped so a long selection stays readable. */
    private String batchNames(ArrayList<File> games) {
        StringBuilder names = new StringBuilder();
        int listed = Math.min(games.size(), 8);
        for (int i = 0; i < listed; i++) {
            names.append("• ").append(displayName(games.get(i))).append('\n');
        }
        if (games.size() > listed) {
            names.append("… 외 ").append(games.size() - listed).append("개");
        }
        return names.toString().trim();
    }

    /**
     * One toast line for a finished batch: the lead (which carries the success
     * count) when anything succeeded, then how many were skipped for having no
     * data and how many failed.
     */
    private String batchSummary(String doneLead, String emptyNote, int done, int empty, int failed) {
        StringBuilder sb = new StringBuilder();
        if (done > 0) {
            sb.append(doneLead);
        }
        if (empty > 0) {
            if (sb.length() > 0) {
                sb.append(" · ");
            }
            sb.append(empty).append("개는 ").append(emptyNote);
        }
        if (failed > 0) {
            if (sb.length() > 0) {
                sb.append(" · ");
            }
            sb.append(failed).append("개 실패");
        }
        return sb.length() == 0 ? "처리할 내용이 없습니다." : sb.toString();
    }

    /** {@link #exportSaves} over every selected game, with one summary toast. */
    private void batchExportSaves(ArrayList<File> games) {
        withDownloadPermission(() -> {
            Toast.makeText(this, games.size() + "개 세이브를 꺼내는 중...", Toast.LENGTH_SHORT).show();
            emulatorThread.execute(() -> {
                int done = 0, empty = 0, failed = 0;
                for (File game : games) {
                    try {
                        if (SaveExporter.export(this, game, displayName(game)) == null) {
                            empty++;
                        } else {
                            done++;
                        }
                    } catch (Exception e) {
                        failed++;
                    }
                }
                final int fDone = done, fEmpty = empty, fFailed = failed;
                runOnUiThread(() -> Toast.makeText(this,
                        batchSummary(fDone + "개 꺼냄 · 다운로드/Mini Mobile/세이브", "저장된 내용 없음", fDone, fEmpty, fFailed),
                        Toast.LENGTH_LONG).show());
            });
        });
    }

    /** {@link #exportDataFolder} over every selected game, one summary toast. */
    private void batchExportDataFolder(ArrayList<File> games) {
        withDownloadPermission(() -> {
            Toast.makeText(this, games.size() + "개를 데이터 폴더로 내보내는 중...", Toast.LENGTH_SHORT).show();
            emulatorThread.execute(() -> {
                int done = 0, empty = 0, failed = 0;
                for (File game : games) {
                    try {
                        if (DataFolder.export(this, game, displayName(game)) == null) {
                            empty++;
                        } else {
                            done++;
                        }
                    } catch (Exception e) {
                        failed++;
                    }
                }
                final int fDone = done, fEmpty = empty, fFailed = failed;
                runOnUiThread(() -> Toast.makeText(this,
                        batchSummary(fDone + "개 내보냄 · 다운로드/Mini Mobile", "저장된 내용 없음", fDone, fEmpty, fFailed),
                        Toast.LENGTH_LONG).show());
            });
        });
    }

    /**
     * {@link #restoreDataFolder} over every selected game. Confirmed first, as
     * it overwrites each game's saved data from its folder and cannot be undone.
     */
    private void confirmBatchRestore(ArrayList<File> games) {
        new AlertDialog.Builder(this)
                .setTitle(games.size() + "개 데이터 폴더에서 불러오기")
                .setMessage("각 게임의 데이터 폴더(다운로드/Mini Mobile/<게임 이름>)의 내용을 지금 저장된 내용에 덮어씁니다.\n덮어쓴 뒤에는 되돌릴 수 없습니다.\n\n" + batchNames(games))
                .setNegativeButton("취소", null)
                .setPositiveButton("불러오기", (dialog, which) -> withDownloadPermission(() ->
                        emulatorThread.execute(() -> {
                            int done = 0, empty = 0, failed = 0;
                            for (File game : games) {
                                try {
                                    if (DataFolder.restore(this, displayName(game)) == null) {
                                        empty++;
                                    } else {
                                        done++;
                                    }
                                } catch (Exception e) {
                                    failed++;
                                }
                            }
                            final int fDone = done, fEmpty = empty, fFailed = failed;
                            runOnUiThread(() -> Toast.makeText(this,
                                    batchSummary(fDone + "개 불러옴 · 게임을 다시 시작하면 적용됩니다", "폴더에 데이터 없음", fDone, fEmpty, fFailed),
                                    Toast.LENGTH_LONG).show());
                        })))
                .show();
    }

    /**
     * {@link #confirmErase} over every selected game. Confirmed first with the
     * list of games, as it removes each one's saves and cannot be undone.
     */
    private void confirmBatchErase(ArrayList<File> games) {
        new AlertDialog.Builder(this)
                .setTitle(games.size() + "개 게임 데이터 초기화")
                .setMessage("선택한 게임이 저장한 내용을 모두 지웁니다.\n세이브도 함께 지워지고, 되돌릴 수 없습니다.\n남겨두려면 먼저 \"세이브 파일 꺼내기\"로 백업하세요.\n\n" + batchNames(games))
                .setNegativeButton("취소", null)
                .setPositiveButton(games.size() + "개 초기화", (dialog, which) ->
                        emulatorThread.execute(() -> {
                            int done = 0, empty = 0, failed = 0;
                            for (File game : games) {
                                try {
                                    if (SaveEraser.erase(this, game) == 0) {
                                        empty++;
                                    } else {
                                        done++;
                                    }
                                } catch (Exception e) {
                                    failed++;
                                }
                            }
                            final int fDone = done, fEmpty = empty, fFailed = failed;
                            runOnUiThread(() -> Toast.makeText(this,
                                    batchSummary(fDone + "개 초기화됨", "저장된 내용 없음", fDone, fEmpty, fFailed),
                                    Toast.LENGTH_LONG).show());
                        }))
                .show();
    }

    /**
     * On the first run, drops a short guide file into 다운로드/Mini Mobile/ and
     * its 세이브/ subfolder so both show up in a file manager before the first
     * export. Only where it needs no permission prompt (Android 10+); on older
     * devices the folders appear on the first export, which asks for storage
     * anyway. Runs once, tracked by a preference.
     */
    private void seedDataFoldersOnce() {
        if (Downloads.needsPermission()) {
            return;
        }

        SharedPreferences prefs = getSharedPreferences("mini_ui", MODE_PRIVATE);
        if (prefs.getBoolean("folders_seeded", false)) {
            return;
        }
        prefs.edit().putBoolean("folders_seeded", true).apply();

        new Thread(() -> {
            try {
                Downloads.writeInto(this, DataFolder.ROOT, "읽어주세요.txt", "text/plain",
                        ("이 폴더는 Mini Mobile 앱의 게임 데이터가 저장되는 곳입니다.\n\n"
                                + "- 세이브/ : '세이브 파일 꺼내기'로 만든 세이브 백업(.zip)\n"
                                + "- <게임 이름>/ : '데이터 폴더로 내보내기'로 꺼낸 세이브 데이터 파일\n\n"
                                + "게임 메뉴에서 내보내기를 하면 이 폴더가 채워집니다.\n").getBytes("UTF-8"));
                Downloads.writeInto(this, Downloads.SAVES_DIR, "읽어주세요.txt", "text/plain",
                        ("이 폴더에는 게임 세이브 백업(.zip)이 저장됩니다.\n\n"
                                + "- 앱에서 '세이브 파일 꺼내기'를 하면 '<게임 이름> 세이브.zip'이 여기 저장됩니다.\n"
                                + "- '세이브 불러오기'를 누르면 이 폴더의 세이브를 골라 되돌릴 수 있습니다.\n").getBytes("UTF-8"));
            } catch (Exception ignored) {
                // Best effort: the folders still appear on the first real export.
            }
        }, "seed-folders").start();
    }

    /**
     * Mirrors a game's save data out to its browsable folder,
     * {@code 다운로드/Mini Mobile/<game>/}, refreshing whatever was there.
     */
    private void exportDataFolder(File game) {
        String title = displayName(game);
        withDownloadPermission(() -> {
            Toast.makeText(this, "데이터 폴더로 내보내는 중...", Toast.LENGTH_SHORT).show();
            emulatorThread.execute(() -> {
                try {
                    DataFolder.Result result = DataFolder.export(this, game, title);
                    runOnUiThread(() -> Toast.makeText(
                            this,
                            result == null
                                    ? "저장된 내용이 없습니다."
                                    : (result.path + " 에 저장 (파일 " + result.files + "개)"),
                            Toast.LENGTH_LONG).show());
                } catch (Exception e) {
                    runOnUiThread(() -> Toast.makeText(this, "내보내기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
                }
            });
        });
    }

    /** Reads a game's data folder back into its saves, overwriting them. */
    private void restoreDataFolder(File game) {
        String title = displayName(game);
        new AlertDialog.Builder(this)
                .setTitle(displayName(game))
                .setMessage("데이터 폴더(" + DataFolder.displayPath(title) + ")의 내용을 지금 저장된 내용에 덮어씁니다.\n덮어쓴 뒤에는 되돌릴 수 없습니다.")
                .setNegativeButton("취소", null)
                .setPositiveButton("불러오기", (dialog, which) -> withDownloadPermission(() ->
                        emulatorThread.execute(() -> {
                            try {
                                DataFolder.Result result = DataFolder.restore(this, title);
                                runOnUiThread(() -> Toast.makeText(
                                        this,
                                        result == null
                                                ? "폴더에 세이브 데이터가 없습니다."
                                                : ("폴더에서 불러왔습니다 (파일 " + result.files + "개). 게임을 다시 시작하면 적용됩니다."),
                                        Toast.LENGTH_LONG).show());
                            } catch (Exception e) {
                                runOnUiThread(() -> Toast.makeText(this, "불러오기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
                            }
                        })))
                .show();
    }

    /**
     * Offers to take a title's storage away so it starts as it would on a phone
     * that had never run it.
     *
     * <p>It is here because a title can be left unable to start by what an
     * earlier build wrote for it, and nothing at runtime can tell such a file
     * from one the title meant to write. 던전앤파이터 격투가 drew a white screen
     * on every run until its options file went; the run that made that file is
     * fixed, and the file it made is not something a fix can reach.
     *
     * <p>The saves go with it, which is why it asks first and why 꺼내기 sits
     * above it in the same menu.
     */
    private void confirmErase(File game) {
        new AlertDialog.Builder(this)
                .setTitle(displayName(game))
                .setMessage("이 게임이 저장한 내용을 모두 지웁니다.\n세이브도 함께 지워지고, 되돌릴 수 없습니다.\n\n남겨두려면 먼저 \"세이브 파일 꺼내기\"로 백업하세요.")
                .setNegativeButton("취소", null)
                .setPositiveButton("초기화", (dialog, which) -> eraseSavesNow(game))
                .show();
    }

    private void eraseSavesNow(File game) {
        emulatorThread.execute(() -> {
            try {
                int removed = SaveEraser.erase(this, game);

                runOnUiThread(() -> Toast.makeText(
                        this,
                        removed == 0 ? "저장된 내용이 없습니다." : "게임 데이터를 초기화했습니다 (" + removed + "개).",
                        Toast.LENGTH_LONG).show());
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "초기화 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    private void confirmDelete(File game) {
        new AlertDialog.Builder(this)
                .setTitle(displayName(game))
                .setMessage("이 게임을 목록에서 삭제할까요?\n저장한 내용은 그대로 남습니다.")
                .setNegativeButton("취소", null)
                .setPositiveButton("삭제", (dialog, which) -> {
                    game.delete();
                    showLibrary();
                })
                .show();
    }

    // --- saves -----------------------------------------------------------

    /**
     * Copies a title's saved data into Downloads, so it can be backed up or
     * moved to another phone. Saves live in the app's private directory, where
     * nothing else can reach them.
     */
    private void exportSaves(File game) {
        String title = displayName(game);

        withDownloadPermission(() -> {
            Toast.makeText(this, "세이브 파일을 꺼내는 중...", Toast.LENGTH_SHORT).show();
            exportSavesNow(game, title);
        });
    }

    private void exportSavesNow(File game, String title) {
        emulatorThread.execute(() -> {
            try {
                SaveExporter.Result result = SaveExporter.export(this, game, title);

                runOnUiThread(() -> {
                    if (result == null) {
                        Toast.makeText(this, "저장된 내용이 없습니다.", Toast.LENGTH_LONG).show();
                        return;
                    }

                    Toast.makeText(this, "다운로드/Mini Mobile/세이브 에 저장: " + result.name + " (" + result.files + "개)", Toast.LENGTH_LONG).show();
                });
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "꺼내기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    /**
     * 세이브 불러오기: the exported save zips to pick one from - the game's own
     * first, newest first and by the time each was taken, then other games'
     * - and, by a long press, to share or remove one. A save kept anywhere
     * else is still reached through the file picker at the foot of the list.
     *
     * <p>Picking one puts it in place. The zip's own {@code db/<id>}/{@code
     * fs/<id>} paths route each file to the game it belongs to, so another
     * game's save goes to that game.
     */
    private void importSaves(File game) {
        String title = displayName(game);
        emulatorThread.execute(() -> {
            String[] ids;
            try {
                ids = SaveExporter.ids(game);
            } catch (Exception e) {
                ids = null;
            }
            List<SaveShelf.Entry> saves = SaveShelf.list(this, ids);
            runOnUiThread(() -> showSaveList(game, title, saves));
        });
    }

    private void showSaveList(File game, String title, List<SaveShelf.Entry> saves) {
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);

        TextView heading = new TextView(this);
        heading.setText("세이브 불러오기");
        heading.setTextSize(20f);
        heading.setTypeface(Typeface.DEFAULT_BOLD);
        heading.setTextColor(LIB_INK);
        heading.setPadding(dp(22), dp(20), dp(22), 0);
        root.addView(heading);

        TextView where = new TextView(this);
        where.setText(title + " · 다운로드/Mini Mobile/세이브");
        where.setTextSize(12.5f);
        where.setTextColor(LIB_MUTED);
        where.setSingleLine(true);
        where.setEllipsize(TextUtils.TruncateAt.END);
        where.setPadding(dp(22), dp(3), dp(22), dp(6));
        root.addView(where);

        LinearLayout list = new LinearLayout(this);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setPadding(dp(10), 0, dp(10), dp(4));

        AlertDialog[] shown = new AlertDialog[1];
        int ours = 0;
        for (SaveShelf.Entry entry : saves) {
            if (entry.ours) {
                ours++;
            }
        }

        if (saves.isEmpty()) {
            TextView empty = new TextView(this);
            empty.setText("아직 꺼낸 세이브가 없어요.\n게임을 길게 눌러 ‘세이브 파일 꺼내기’로 만들 수 있어요.");
            empty.setTextSize(13.5f);
            empty.setTextColor(LIB_MUTED);
            empty.setLineSpacing(0f, 1.25f);
            empty.setPadding(dp(12), dp(14), dp(12), dp(14));
            list.addView(empty);
        } else {
            if (ours > 0) {
                list.addView(saveSection("이 게임의 세이브 · " + ours + "개", LIB_GREEN_DEEP));
            }
            boolean latestMarked = false;
            boolean othersHeaded = false;
            for (SaveShelf.Entry entry : saves) {
                if (!entry.ours && !othersHeaded) {
                    list.addView(saveSection("다른 게임의 세이브 · " + (saves.size() - ours) + "개", LIB_MUTED));
                    othersHeaded = true;
                }
                boolean latest = entry.ours && !entry.beforeImport() && !latestMarked;
                latestMarked |= latest;

                View row = saveRow(entry, latest);
                row.setOnClickListener(v -> {
                    shown[0].dismiss();
                    confirmSaveImport(game, title, entry);
                });
                row.setOnLongClickListener(v -> {
                    shown[0].dismiss();
                    showSaveActions(game, title, entry);
                    return true;
                });
                LinearLayout.LayoutParams rowParams = new LinearLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
                rowParams.bottomMargin = dp(2);
                list.addView(row, rowParams);
            }
        }

        // However many saves there are, the list scrolls within a part of the
        // screen and the way to the file picker stays in sight below it.
        int maxHeight = Math.round(getResources().getDisplayMetrics().heightPixels * 0.55f);
        ScrollView scroll = new ScrollView(this) {
            @Override
            protected void onMeasure(int widthSpec, int heightSpec) {
                super.onMeasure(widthSpec, MeasureSpec.makeMeasureSpec(maxHeight, MeasureSpec.AT_MOST));
            }
        };
        scroll.addView(list);
        root.addView(scroll);

        View divider = new View(this);
        divider.setBackgroundColor(LIB_DIVIDER);
        LinearLayout.LayoutParams dividerParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(1));
        dividerParams.leftMargin = dp(20);
        dividerParams.rightMargin = dp(20);
        dividerParams.topMargin = dp(4);
        root.addView(divider, dividerParams);

        TextView elsewhere = new TextView(this);
        elsewhere.setText("📂   다른 위치에서 찾기…");
        elsewhere.setTextSize(15f);
        elsewhere.setTypeface(Typeface.DEFAULT_BOLD);
        elsewhere.setTextColor(LIB_GREEN_DEEP);
        elsewhere.setPadding(dp(22), dp(14), dp(22), dp(10));
        elsewhere.setForeground(saveRipple());
        elsewhere.setOnClickListener(v -> {
            shown[0].dismiss();
            openSavePicker();
        });
        root.addView(elsewhere);

        shown[0] = lightAlert()
                .setView(root)
                .setNegativeButton("취소", null)
                .show();
    }

    /** A heading over a part of the save list, with a rule out to the edge. */
    private View saveSection(String text, int color) {
        LinearLayout section = new LinearLayout(this);
        section.setOrientation(LinearLayout.HORIZONTAL);
        section.setGravity(android.view.Gravity.CENTER_VERTICAL);
        section.setPadding(dp(10), dp(12), dp(10), dp(6));

        TextView label = new TextView(this);
        label.setText(text);
        label.setTextSize(12f);
        label.setTypeface(Typeface.DEFAULT_BOLD);
        label.setTextColor(color);
        section.addView(label);

        View rule = new View(this);
        rule.setBackgroundColor(LIB_LINE);
        LinearLayout.LayoutParams ruleParams = new LinearLayout.LayoutParams(0, dp(1), 1f);
        ruleParams.leftMargin = dp(8);
        section.addView(rule, ruleParams);
        return section;
    }

    /** A touch ripple over a rounded row. */
    private android.graphics.drawable.RippleDrawable saveRipple() {
        return new android.graphics.drawable.RippleDrawable(
                android.content.res.ColorStateList.valueOf(0x222E8B57), null, roundedRect(Color.WHITE, 0, 0, 12));
    }

    /**
     * One save in the list: its badge, when it was taken (or, for another
     * game's, which game), what it holds, and how long ago.
     */
    private View saveRow(SaveShelf.Entry entry, boolean latest) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(dp(10), dp(9), dp(10), dp(9));
        if (latest) {
            row.setBackground(roundedRect(LIB_SELECT_BG, LIB_GREEN_LINE, 1, 12));
        }
        row.setForeground(saveRipple());

        row.addView(saveBadge(entry));

        LinearLayout text = new LinearLayout(this);
        text.setOrientation(LinearLayout.VERTICAL);

        LinearLayout top = new LinearLayout(this);
        top.setOrientation(LinearLayout.HORIZONTAL);
        top.setGravity(android.view.Gravity.CENTER_VERTICAL);

        TextView main = new TextView(this);
        main.setText(entry.ours ? saveDay(entry.modified) : entry.title());
        main.setTextSize(16f);
        main.setTypeface(entry.ours ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
        main.setTextColor(entry.ours ? LIB_INK : LIB_MUTED);
        main.setSingleLine(true);
        main.setEllipsize(TextUtils.TruncateAt.END);
        top.addView(main, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        if (latest) {
            top.addView(saveTag("최신", LIB_GREEN_SOFT, LIB_GREEN_DEEP));
        } else if (entry.beforeImport()) {
            top.addView(saveTag("가져오기 전", LIB_STAR_SOFT, LIB_STAR_INK));
        }
        text.addView(top);

        TextView sub = new TextView(this);
        if (!entry.ours) {
            sub.setText(saveDay(entry.modified));
        } else if (entry.beforeImport()) {
            sub.setText("불러오기 직전에 자동으로 남긴 백업");
        } else {
            sub.setText(saveStamp(entry.modified) + " · 파일 " + entry.files + "개 · " + formatSize(entry.bytes));
        }
        sub.setTextSize(12f);
        sub.setTextColor(LIB_MUTED);
        sub.setSingleLine(true);
        sub.setEllipsize(TextUtils.TruncateAt.END);
        LinearLayout.LayoutParams subParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        subParams.topMargin = dp(2);
        text.addView(sub, subParams);

        row.addView(text, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        TextView ago = new TextView(this);
        ago.setText(saveAgo(entry.modified));
        ago.setTextSize(12f);
        ago.setTextColor(LIB_MUTED);
        ago.setPadding(dp(8), 0, 0, 0);
        row.addView(ago);
        return row;
    }

    /** 📦 for a save, 🛟 for the copy kept before an import; faded for another game's. */
    private View saveBadge(SaveShelf.Entry entry) {
        TextView icon = new TextView(this);
        icon.setText(entry.beforeImport() ? "🛟" : "📦");
        icon.setTextSize(18f);
        icon.setGravity(android.view.Gravity.CENTER);
        icon.setBackground(roundedRect(entry.beforeImport() ? LIB_STAR_SOFT : LIB_GREEN_SOFTER, 0, 0, 11));
        if (!entry.ours) {
            icon.setAlpha(0.6f);
        }
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(dp(40), dp(40));
        params.rightMargin = dp(12);
        icon.setLayoutParams(params);
        return icon;
    }

    private TextView saveTag(String text, int fill, int ink) {
        TextView tag = new TextView(this);
        tag.setText(text);
        tag.setTextSize(10.5f);
        tag.setTypeface(Typeface.DEFAULT_BOLD);
        tag.setTextColor(ink);
        tag.setPadding(dp(7), dp(1), dp(7), dp(2));
        tag.setBackground(roundedRect(fill, 0, 0, 99));
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        params.leftMargin = dp(6);
        tag.setLayoutParams(params);
        return tag;
    }

    /** 10월 10일 (토) 09:02 */
    private static String saveDay(long millis) {
        return new java.text.SimpleDateFormat("M월 d일 (E) HH:mm", Locale.KOREAN).format(new java.util.Date(millis));
    }

    /** 2026-10-10 09:02 */
    private static String saveStamp(long millis) {
        return new java.text.SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.ROOT).format(new java.util.Date(millis));
    }

    /** 방금 전, 5분 전, 9시간 전, 어제, 12일 전, 1년 전. */
    private static String saveAgo(long millis) {
        long elapsed = Math.max(0L, System.currentTimeMillis() - millis);
        if (elapsed < 60_000L) {
            return "방금 전";
        }
        if (elapsed < 3_600_000L) {
            return (elapsed / 60_000L) + "분 전";
        }
        if (elapsed < 86_400_000L) {
            return (elapsed / 3_600_000L) + "시간 전";
        }
        long days = elapsed / 86_400_000L;
        if (days == 1) {
            return "어제";
        }
        return days < 365 ? days + "일 전" : (days / 365) + "년 전";
    }

    /** Asks before a save from the list is put in place; 취소 goes back to the list. */
    private void confirmSaveImport(File game, String title, SaveShelf.Entry entry) {
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(22), dp(6), dp(22), dp(4));

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.HORIZONTAL);
        card.setGravity(android.view.Gravity.CENTER_VERTICAL);
        card.setPadding(dp(12), dp(12), dp(12), dp(12));
        card.setBackground(roundedRect(LIB_GREEN_SOFTER, 0, 0, 12));
        card.addView(saveBadge(entry));
        LinearLayout cardText = new LinearLayout(this);
        cardText.setOrientation(LinearLayout.VERTICAL);
        TextView cardMain = new TextView(this);
        cardMain.setText(entry.ours ? saveDay(entry.modified) : entry.title());
        cardMain.setTextSize(15f);
        cardMain.setTypeface(Typeface.DEFAULT_BOLD);
        cardMain.setTextColor(LIB_INK);
        cardText.addView(cardMain);
        TextView cardSub = new TextView(this);
        cardSub.setText((entry.ours ? "" : saveDay(entry.modified) + " · ")
                + "파일 " + entry.files + "개 · " + formatSize(entry.bytes) + " · " + saveAgo(entry.modified));
        cardSub.setTextSize(12f);
        cardSub.setTextColor(LIB_MUTED);
        cardText.addView(cardSub);
        card.addView(cardText, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        LinearLayout.LayoutParams cardParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        cardParams.bottomMargin = dp(12);
        box.addView(card, cardParams);

        TextView message = new TextView(this);
        message.setText(entry.ours
                ? "지금 저장된 내용을 이 세이브로 바꿉니다."
                : "‘" + entry.title() + "’ 게임의 세이브예요.\n그 게임에 저장된 내용을 이 세이브로 바꿉니다.");
        message.setTextSize(13.5f);
        message.setTextColor(LIB_INK);
        message.setLineSpacing(0f, 1.25f);
        box.addView(message);

        TextView note = new TextView(this);
        note.setText(entry.ours
                ? "🛟 지금 세이브는 ‘가져오기 전’ 백업으로 목록에 남겨둬서, 언제든 다시 되돌릴 수 있어요."
                : "되돌릴 수 없으니, 필요하면 그 게임에서 먼저 세이브를 꺼내 두세요.");
        note.setTextSize(12.5f);
        note.setTextColor(entry.ours ? LIB_GREEN_DEEP : LIB_MUTED);
        note.setLineSpacing(0f, 1.2f);
        note.setPadding(dp(11), dp(9), dp(11), dp(9));
        note.setBackground(roundedRect(LIB_SELECT_BG, 0, 0, 10));
        LinearLayout.LayoutParams noteParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        noteParams.topMargin = dp(10);
        box.addView(note, noteParams);

        lightAlert()
                .setTitle("이 세이브를 불러올까요?")
                .setView(box)
                .setNegativeButton("취소", (dialog, which) -> importSaves(game))
                .setPositiveButton("불러오기", (dialog, which) -> withDownloadPermission(() -> restoreSave(game, title, entry)))
                .show();
    }

    /**
     * Puts a save from the list in place. Over the game's own saves, what is
     * there now is exported first, as the PC version does, so the import can
     * be undone from the same list.
     */
    private void restoreSave(File game, String title, SaveShelf.Entry entry) {
        Toast.makeText(this, "세이브를 불러오는 중...", Toast.LENGTH_SHORT).show();
        emulatorThread.execute(() -> {
            try {
                SaveExporter.Result kept = entry.ours ? SaveExporter.export(this, game, title, SaveShelf.BEFORE_IMPORT) : null;
                SaveImporter.Result result;
                try (InputStream input = SaveShelf.open(this, entry)) {
                    result = SaveImporter.importZip(this, input);
                }
                String done = "세이브를 불러왔습니다 (" + result.files + "개). 게임을 다시 시작하면 적용됩니다."
                        + (kept == null ? "" : "\n이전 세이브는 ‘가져오기 전’으로 남겨뒀어요.");
                runOnUiThread(() -> Toast.makeText(this, done, Toast.LENGTH_LONG).show());
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "불러오기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    /** What a long press on a save offers. */
    private void showSaveActions(File game, String title, SaveShelf.Entry entry) {
        String label = entry.ours ? saveDay(entry.modified) : entry.title();
        MenuItem[] items = {
                new MenuItem("📦", "불러오기", "", "이 세이브로 바꾸기", false),
                new MenuItem("📤", "공유하기", "", "카톡·드라이브 등으로 보내기", false),
                new MenuItem("🗑", "이 세이브 파일 지우기", "", "지우면 되돌릴 수 없습니다", true),
        };
        lightAlert()
                .setTitle(label + " 세이브")
                .setAdapter(menuAdapter(items), (dialog, which) -> {
                    if (which == 0) {
                        confirmSaveImport(game, title, entry);
                    } else if (which == 1) {
                        shareSave(entry);
                    } else {
                        confirmSaveDelete(game, entry, label);
                    }
                })
                .setNegativeButton("취소", (dialog, which) -> importSaves(game))
                .show();
    }

    /** Hands the zip to the share sheet. */
    private void shareSave(SaveShelf.Entry entry) {
        emulatorThread.execute(() -> {
            try {
                Uri uri = SaveShelf.shareUri(this, entry);
                runOnUiThread(() -> {
                    Intent send = new Intent(Intent.ACTION_SEND);
                    send.setType("application/zip");
                    send.putExtra(Intent.EXTRA_STREAM, uri);
                    send.setClipData(android.content.ClipData.newRawUri(entry.name, uri));
                    send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                    startActivity(Intent.createChooser(send, "세이브 공유하기"));
                });
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "공유할 수 없습니다: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    private void confirmSaveDelete(File game, SaveShelf.Entry entry, String label) {
        lightAlert()
                .setTitle("이 세이브 파일을 지울까요?")
                .setMessage(label + "\n" + entry.name + "\n\n지운 파일은 되돌릴 수 없습니다.")
                .setNegativeButton("취소", (dialog, which) -> importSaves(game))
                .setPositiveButton("지우기", (dialog, which) -> emulatorThread.execute(() -> {
                    boolean all = SaveShelf.delete(this, entry);
                    runOnUiThread(() -> {
                        Toast.makeText(this, all
                                ? "세이브 파일을 지웠습니다."
                                : "앱 안의 사본은 지웠어요. 다운로드/Mini Mobile/세이브 의 파일은 파일 앱에서 지워주세요.",
                                Toast.LENGTH_LONG).show();
                        importSaves(game);
                    });
                }))
                .show();
    }

    private void openSavePicker() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{
                "application/zip",
                "application/octet-stream",
        });
        // Start where exported saves are kept, rather than wherever the
        // picker was last left.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI, Downloads.savesFolderUri());
        }
        startActivityForResult(intent, PICK_SAVE);
    }

    private void importSaveNow(Uri uri) {
        emulatorThread.execute(() -> {
            try (InputStream input = getContentResolver().openInputStream(uri)) {
                if (input == null) {
                    throw new IllegalStateException("파일을 열 수 없습니다.");
                }

                SaveImporter.Result result = SaveImporter.importZip(this, input);

                runOnUiThread(() -> Toast.makeText(
                        this,
                        "세이브를 불러왔습니다 (" + result.files + "개). 게임을 다시 시작하면 적용됩니다.",
                        Toast.LENGTH_LONG).show());
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "불러오기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    /**
     * Opens a log collection window.
     *
     * <p>Throws away what is held and turns every area on, so the file that
     * comes out of {@link #stopLogCollectAndSave()} covers exactly the stretch
     * of play between the two presses - and covers all of it, whatever part of
     * the emulator the question turns out to be about.
     */
    private void startLogCollect() {
        String error = NativeBridge.nativeStartLogCollect();
        showCollectState();

        Toast.makeText(this,
                error.isEmpty() ? "로그 수집 시작 - 문제를 재현한 뒤 종료를 누르세요" : "수집 시작 실패: " + error,
                Toast.LENGTH_LONG).show();
    }

    /**
     * Closes the window and saves what it caught to Downloads.
     *
     * <p>Also the way to save without having collected: pressed on its own it
     * writes the running game's log as the single log button used to, which is
     * what a title that hangs rather than stops needs.
     */
    private void stopLogCollectAndSave() {
        String error = NativeBridge.nativeStopLogCollect();
        showCollectState();

        if (!error.isEmpty()) {
            Toast.makeText(this, "수집 종료 실패: " + error, Toast.LENGTH_LONG).show();
            return;
        }

        withDownloadPermission(() -> {
            Toast.makeText(this, "로그를 저장하는 중...", Toast.LENGTH_SHORT).show();
            // Its own thread, not the emulator's. The log a person presses this
            // for is most often the log of a title that has hung, and the
            // emulator thread is inside that hang: anything queued behind it
            // waits as long as the hang lasts, which is why the one log worth
            // having was the one that could never be written. Nothing here
            // needs the emulator - the capture and the run's status are read
            // through their own locks.
            new Thread(() -> writeLogToDownloads(null, true), "log-save").start();
        });
    }

    /** Dims whichever of the two buttons is not the one to press next. */
    private void showCollectState() {
        if (collectButton == null || stopButton == null) {
            return;
        }

        boolean collecting = NativeBridge.nativeLogCollecting() != 0;
        collectButton.setAlpha(collecting ? 0.45f : 1f);
        stopButton.setAlpha(collecting ? 1f : 0.45f);
    }

    /**
     * The settings a collection window does not decide for itself.
     *
     * <p>Pressing 수집 needs no setup: the window turns every log area on and
     * records the branches the emulated code takes, which is what a question
     * about an unfamiliar title needs and what used to be guessed in advance
     * and compiled in. Two things are left. A write watch needs an address,
     * because recording every store is the one thing a window cannot afford.
     * And the filter here is the one the always-on capture runs under between
     * windows - the one a crash auto-save is written from.
     */
    private void showDiagnosticsDialog() {
        LinearLayout fields = new LinearLayout(this);
        fields.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(16);
        fields.setPadding(pad, pad / 2, pad, 0);

        TextView watchLabel = new TextView(this);
        watchLabel.setText("프로브 감시  (w:1518700 - 그 주소에 쓰는 순간을 기록, 쉼표로 여러 개)");
        watchLabel.setTextSize(12f);
        fields.addView(watchLabel);

        EditText watches = new EditText(this);
        watches.setText(NativeBridge.nativeProbeWatches());
        watches.setSingleLine(true);
        fields.addView(watches);

        TextView filterLabel = new TextView(this);
        filterLabel.setText("\n로그 필터  (비워두면 수집 중 전체 기록)");
        filterLabel.setTextSize(12f);
        fields.addView(filterLabel);

        EditText filter = new EditText(this);
        filter.setText(NativeBridge.nativeLogFilter());
        filter.setSingleLine(true);
        fields.addView(filter);

        new AlertDialog.Builder(this)
                .setTitle("진단 설정")
                .setView(fields)
                .setPositiveButton("적용", (dialog, which) -> {
                    String watchError = NativeBridge.nativeSetProbeWatches(watches.getText().toString().trim());
                    String filterError = NativeBridge.nativeSetLogFilter(filter.getText().toString().trim());

                    String problem = !watchError.isEmpty() ? "감시 오류: " + watchError
                            : !filterError.isEmpty() ? "필터 오류: " + filterError : "";
                    Toast.makeText(this, problem.isEmpty() ? "적용됨" : problem, Toast.LENGTH_LONG).show();
                })
                .setNeutralButton("기본값", (dialog, which) -> {
                    NativeBridge.nativeSetProbeWatches("");
                    NativeBridge.nativeSetLogFilter("");
                    Toast.makeText(this, "기본값으로 되돌림", Toast.LENGTH_SHORT).show();
                })
                .setNegativeButton("취소", null)
                .show();
    }

    /**
     * Writes the current run's log to Downloads. Runs on the emulator thread so
     * the native reads happen off the UI thread.
     *
     * @param suffix   appended to the file name (before ".txt") to tell an
     *                 auto-saved crash log apart from a manual save, or null
     * @param announce whether to toast the result; an auto-save that the person
     *                 did not ask for stays quiet on failure
     */
    private void writeLogToDownloads(String suffix, boolean announce) {
        String title = currentGameName != null ? currentGameName : "wie";
        try {
            // The message shown in the title bar is cut off at one line, so the
            // whole of it goes at the top of the file.
            String error = NativeBridge.nativeLastError();
            // The apk's version code, so a log says which build produced it -
            // the development build is replaced on every push and the installed
            // one is easy to mistake for a newer release.
            long versionCode = -1;
            try {
                android.content.pm.PackageInfo info = getPackageManager().getPackageInfo(getPackageName(), 0);
                versionCode = Build.VERSION.SDK_INT >= Build.VERSION_CODES.P ? info.getLongVersionCode() : info.versionCode;
            } catch (Exception ignored) {
            }
            String header = "wie " + NativeBridge.nativeVersion() + " (build " + versionCode + ")\n"
                    + "game: " + title + "\n"
                    + "running: " + (NativeBridge.nativeRunning() != 0) + "\n"
                    + "filter: " + NativeBridge.nativeLogFilter() + "\n"
                    + (error.isEmpty() ? "" : "last error: " + error + "\n")
                    + "\n";

            byte[] contents = (header + NativeBridge.nativeLog()).getBytes("UTF-8");
            String name = Downloads.safeName(title) + " 로그" + (suffix == null ? "" : suffix) + ".txt";
            Downloads.write(this, name, "text/plain", contents);

            if (announce) {
                runOnUiThread(() -> Toast.makeText(this,
                        "다운로드 폴더에 저장: " + name + " (" + (contents.length / 1024) + "KB)",
                        Toast.LENGTH_LONG).show());
            }
        } catch (Exception e) {
            if (announce) {
                runOnUiThread(() -> Toast.makeText(this, "로그 저장 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        }
    }

    /**
     * Saves the log by itself when a game stops, so a title that crashes or ends
     * before the person can reach the log button still leaves one behind. Best
     * effort: on pre-Android 10 without the storage permission it is skipped
     * silently rather than prompting mid-crash.
     */
    private void autoSaveLogOnStop() {
        if (Downloads.needsPermission()
                && checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED) {
            return;
        }

        // A timestamp so successive crashes do not clobber one another and the
        // auto-saved file is easy to tell from a manual save.
        String stamp = new java.text.SimpleDateFormat("MMdd_HHmmss", java.util.Locale.US).format(new java.util.Date());
        writeLogToDownloads(" 자동 " + stamp, true);
    }

    /**
     * Runs {@code action} once Downloads can be written to. Before Android 10
     * that needs asking; from 10 on the MediaStore insert needs nothing.
     */
    private void withDownloadPermission(Runnable action) {
        if (!Downloads.needsPermission()
                || checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED) {
            action.run();
            return;
        }

        pendingDownload = action;
        requestPermissions(new String[]{Manifest.permission.WRITE_EXTERNAL_STORAGE}, REQUEST_WRITE_DOWNLOADS);
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] granted) {
        super.onRequestPermissionsResult(requestCode, permissions, granted);

        if (requestCode == REQUEST_CALL_PHONE) {
            String number = pendingPhoneCall;
            pendingPhoneCall = null;

            if (number != null
                    && granted.length > 0
                    && granted[0] == PackageManager.PERMISSION_GRANTED) {
                placePhoneCall(number);
            } else if (number != null) {
                Toast.makeText(this, "전화 권한이 없어 통화 요청을 실행할 수 없습니다.", Toast.LENGTH_LONG).show();
            }
            return;
        }

        Runnable action = pendingDownload;
        pendingDownload = null;

        if (requestCode != REQUEST_WRITE_DOWNLOADS || action == null) {
            return;
        }

        if (granted.length > 0 && granted[0] == PackageManager.PERMISSION_GRANTED) {
            action.run();
        } else {
            Toast.makeText(this, "저장 공간 권한이 없어 다운로드 폴더에 쓸 수 없습니다.", Toast.LENGTH_LONG).show();
        }
    }

    /**
     * Uses a reasonably sized image in the archive as cover art.
     *
     * <p>Which file that is depends on the carrier. LGT archives name their
     * icons {@code big.png}/{@code middle.png}/{@code small.png}, and those were
     * all this looked for. KTF archives name the same three
     * {@code big.icon}/{@code middle.icon}/{@code small.icon} - and a few name
     * them after the application, like {@code 0002E1A1_3_s.PNG} - so a KTF title
     * came up with the placeholder tile every time.
     *
     * <p>So the name is not what says an entry is an icon; its first bytes are.
     * They are PNGs whatever they are called, except where they are Windows
     * bitmaps - 데몬헌터's `small.icon` is one - and both decode here.
     *
     * <p>Where the icon sits in the archive is not fixed either. This used to
     * read the entries in order and give up after two hundred, which is every
     * entry a handset archive has - but an archive carrying a title's extra
     * downloaded data has hundreds more, and the icons can land behind all of
     * them: 오즈 천공의기사단 with its data packed in keeps them at entry 334.
     * Reading the archive's index instead of streaming it means the whole list
     * is available cheaply, so the entries that look like icons by name are
     * tried first and every other candidate is still there to fall back on.
     */
    private Bitmap readArchiveIcon(File game) {
        final int maxCandidates = 200;
        final int maxIconBytes = 512 * 1024;

        // Entry names are read as Latin-1, which decodes any byte. SKT archives
        // name their save data in EUC-KR (드래곤아이즈's 드래곤아이즈.dat), and
        // the default UTF-8 refuses the whole archive on that one name - every
        // other entry, the icon too, with it.
        try (ZipFile zip = new ZipFile(game, StandardCharsets.ISO_8859_1)) {
            List<ZipEntry> candidates = new ArrayList<>();
            Enumeration<? extends ZipEntry> entries = zip.entries();
            while (entries.hasMoreElements()) {
                ZipEntry entry = entries.nextElement();
                if (entry.isDirectory()) {
                    continue;
                }
                long size = entry.getSize();
                if (size > maxIconBytes) {
                    continue;
                }
                candidates.add(entry);
            }

            // The named icons first, then everything else in the archive's own
            // order, so a title that names them something else still resolves.
            Collections.sort(candidates, new Comparator<ZipEntry>() {
                @Override
                public int compare(ZipEntry left, ZipEntry right) {
                    return iconNameRank(left.getName()) - iconNameRank(right.getName());
                }
            });

            int tried = 0;
            for (ZipEntry entry : candidates) {
                if (tried >= maxCandidates) {
                    break;
                }
                tried++;

                Bitmap bitmap = readIconEntry(zip, entry, maxIconBytes);
                if (bitmap != null) {
                    return bitmap;
                }
            }

            // Nothing in the archive's own entries decoded. Its jar still has
            // the title's own pictures, and one of those beats the placeholder.
            Bitmap fromJar = readIconInsideJar(zip, maxIconBytes);
            if (fromJar != null) {
                return fromJar;
            }
        } catch (Exception e) {
            // A corrupt or unreadable archive still gets a placeholder tile.
        }

        return null;
    }

    /**
     * Cover art out of the title's own jar, for an archive whose icons this
     * cannot read.
     *
     * <p>장금이의꿈's three icons are not pictures in any format here: they open
     * {@code SAF\0}, a KTF container whose pixels are packed its own way, and
     * every other entry in that archive is the jar - one entry, far past the
     * size cap, so the archive came up with the placeholder tile. Its jar holds
     * three hundred PNGs, the title art among them.
     *
     * <p>Which one is the same question the named icons answer by being named
     * largest first, and the same answer: the biggest picture that will fit a
     * tile. A title's own artwork is the largest thing it carries - 장금이의꿈's
     * is {@code img/menu/introchar.png}, 장금이 under the title in 151x193 -
     * while the rest of a jar is sprites and strips of HUD. Names are no help
     * here: this title's {@code logo.png} is the publisher's mark.
     *
     * <p>Only the bounds are decoded while looking, which reads each picture's
     * header and not its pixels, and only the winner is kept.
     */
    private Bitmap readIconInsideJar(ZipFile zip, int maxIconBytes) {
        final int maxJarBytes = 16 * 1024 * 1024;
        final int minSide = 32;
        final int maxSide = 512;

        for (Enumeration<? extends ZipEntry> entries = zip.entries(); entries.hasMoreElements(); ) {
            ZipEntry jar = entries.nextElement();
            if (jar.isDirectory() || !jar.getName().toLowerCase(Locale.US).endsWith(".jar")) {
                continue;
            }
            if (jar.getSize() > maxJarBytes) {
                continue;
            }

            byte[] best = null;
            long bestArea = 0;

            try (ZipInputStream stream = new ZipInputStream(skipSkvmJarHeader(zip.getInputStream(jar)))) {
                byte[] chunk = new byte[8192];
                ZipEntry inner;
                while ((inner = stream.getNextEntry()) != null) {
                    if (inner.isDirectory()) {
                        continue;
                    }

                    byte[] header = new byte[8];
                    int headerRead = 0;
                    while (headerRead < header.length) {
                        int read = stream.read(header, headerRead, header.length - headerRead);
                        if (read <= 0) {
                            break;
                        }
                        headerRead += read;
                    }
                    if (!looksLikeImage(header, headerRead)) {
                        continue;
                    }

                    ByteArrayOutputStream buffer = new ByteArrayOutputStream();
                    buffer.write(header, 0, headerRead);
                    int read;
                    while (buffer.size() <= maxIconBytes && (read = stream.read(chunk)) > 0) {
                        buffer.write(chunk, 0, read);
                    }
                    if (buffer.size() > maxIconBytes) {
                        continue;
                    }

                    byte[] bytes = buffer.toByteArray();
                    BitmapFactory.Options bounds = new BitmapFactory.Options();
                    bounds.inJustDecodeBounds = true;
                    BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);

                    if (bounds.outWidth < minSide || bounds.outHeight < minSide
                            || bounds.outWidth > maxSide || bounds.outHeight > maxSide) {
                        continue;
                    }

                    long area = (long) bounds.outWidth * bounds.outHeight;
                    if (area > bestArea) {
                        bestArea = area;
                        best = bytes;
                    }
                }
            } catch (Exception e) {
                continue;
            }

            if (best != null) {
                Bitmap bitmap = BitmapFactory.decodeByteArray(best, 0, best.length);
                if (bitmap != null) {
                    return bitmap;
                }
            }
        }

        return null;
    }

    /**
     * The jar's zip, past the 32 bytes of SK-VM header an SKT jar opens with,
     * which a zip reader would otherwise take for no entries at all.
     */
    private static InputStream skipSkvmJarHeader(InputStream input) throws IOException {
        final int skvmHeaderBytes = 32;

        BufferedInputStream stream = new BufferedInputStream(input);
        stream.mark(skvmHeaderBytes + 4);
        byte[] head = new byte[skvmHeaderBytes + 4];
        int headRead = 0;
        while (headRead < head.length) {
            int read = stream.read(head, headRead, head.length - headRead);
            if (read <= 0) {
                break;
            }
            headRead += read;
        }
        stream.reset();

        boolean zipAtStart = headRead >= 2 && head[0] == 'P' && head[1] == 'K';
        boolean zipAfterHeader = headRead >= skvmHeaderBytes + 2 && head[skvmHeaderBytes] == 'P' && head[skvmHeaderBytes + 1] == 'K';
        if (!zipAtStart && zipAfterHeader) {
            long skipped = 0;
            while (skipped < skvmHeaderBytes) {
                long step = stream.skip(skvmHeaderBytes - skipped);
                if (step <= 0) {
                    break;
                }
                skipped += step;
            }
        }
        return stream;
    }

    /** Lower ranks are tried first: the names an icon usually has. */
    private static int iconNameRank(String path) {
        String name = path;
        int slash = name.lastIndexOf('/');
        if (slash >= 0) {
            name = name.substring(slash + 1);
        }
        name = name.toLowerCase(Locale.US);

        // Largest first, so the tile gets the best picture the archive has.
        if (name.startsWith("big.")) {
            return 0;
        }
        if (name.startsWith("middle.")) {
            return 1;
        }
        if (name.startsWith("small.")) {
            return 2;
        }
        if (name.endsWith(".icon") || name.contains("icon") || name.endsWith(".wmr") || name.endsWith(".mif")) {
            return 3;
        }
        if (name.endsWith("_l.png") || name.endsWith("_ad.png") || name.endsWith("_m.png") || name.endsWith("_s.png")) {
            return 3;
        }
        return 4;
    }

    /** The entry decoded as cover art, or null when it is not one. */
    private Bitmap readIconEntry(ZipFile zip, ZipEntry entry, int maxIconBytes) {
        final int minSide = 12;
        final int maxSide = 256;

        try (InputStream stream = zip.getInputStream(entry)) {
            // The header first, so an archive's own jar is passed over on its
            // first eight bytes rather than read to the cap.
            byte[] header = new byte[8];
            int headerRead = 0;
            while (headerRead < header.length) {
                int read = stream.read(header, headerRead, header.length - headerRead);
                if (read <= 0) {
                    break;
                }
                headerRead += read;
            }

            boolean skvmIcon = isSkvmIconResource(header, headerRead);
            boolean brewInfo = entry.getName().toLowerCase(Locale.US).endsWith(".mif");
            if (!skvmIcon && !brewInfo && !looksLikeImage(header, headerRead)) {
                return null;
            }

            ByteArrayOutputStream buffer = new ByteArrayOutputStream();
            buffer.write(header, 0, headerRead);
            byte[] chunk = new byte[8192];
            int read;
            while ((read = stream.read(chunk)) > 0 && buffer.size() <= maxIconBytes) {
                buffer.write(chunk, 0, read);
            }

            byte[] bytes = buffer.toByteArray();
            Bitmap bitmap;
            if (skvmIcon) {
                bitmap = decodeSkvmIcon(bytes);
            } else if (brewInfo) {
                bitmap = decodeBrewIcon(bytes);
            } else {
                bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.length);
            }
            if (bitmap != null
                    && bitmap.getWidth() >= minSide && bitmap.getHeight() >= minSide
                    && bitmap.getWidth() <= maxSide && bitmap.getHeight() <= maxSide) {
                return bitmap;
            }
        } catch (Exception e) {
            // Not an icon, or unreadable; the next candidate still gets a turn.
        }

        return null;
    }

    /**
     * Whether these bytes open an SK-VM icon resource, the {@code .wmr} beside
     * an SKT title's jar.
     *
     * <p>An SKT archive carries no picture of its own: its entries are the
     * jar, the {@code .msd} descriptor, the {@code .mod} and the {@code .wmr},
     * and the jar's leading 32 bytes of SK-VM header keep it from reading as a
     * zip. So every SKT title came up with the placeholder tile.
     */
    private static boolean isSkvmIconResource(byte[] header, int length) {
        return length >= 4
                && (header[0] & 0xff) == 0xad && (header[1] & 0xff) == 0xde
                && (header[2] & 0xff) == 0xce && (header[3] & 0xff) == 0xfa;
    }

    /**
     * The menu icon out of an SK-VM icon resource.
     *
     * <p>The file is the magic {@code 0xFACEDEAD} and its total length, then
     * records of an index and a byte length, each followed by its body. Record
     * 0 is the handset menu's still icon, a plain 23x23 BMP; record 1 is the
     * animated one, in a format of its own.
     */
    private static Bitmap decodeSkvmIcon(byte[] bytes) {
        int offset = 8;
        while (offset + 8 <= bytes.length) {
            int index = readLittleEndianInt(bytes, offset);
            int length = readLittleEndianInt(bytes, offset + 4);
            int body = offset + 8;
            if (length < 0 || length > bytes.length - body) {
                return null;
            }
            if (length > 6 && (index == 0 || (bytes[body] == 'B' && bytes[body + 1] == 'M'))) {
                // Some titles' icons declare a file size short of their own
                // pixels (1638 bytes for 1710), which a strict decoder refuses,
                // so the record's own length stands in for it.
                byte[] bmp = Arrays.copyOfRange(bytes, body, body + length);
                bmp[2] = (byte) length;
                bmp[3] = (byte) (length >> 8);
                bmp[4] = (byte) (length >> 16);
                bmp[5] = (byte) (length >> 24);
                Bitmap bitmap = BitmapFactory.decodeByteArray(bmp, 0, bmp.length);
                if (bitmap != null) {
                    // Blown up by whole pixels here so the tile, which fills
                    // at many times the icon's size, keeps its pixels sharp
                    // rather than smearing them.
                    int scale = 4;
                    return Bitmap.createScaledBitmap(bitmap, bitmap.getWidth() * scale, bitmap.getHeight() * scale, false);
                }
            }
            offset = body + length;
        }
        return null;
    }

    private static int readLittleEndianInt(byte[] bytes, int offset) {
        return (bytes[offset] & 0xff)
                | (bytes[offset + 1] & 0xff) << 8
                | (bytes[offset + 2] & 0xff) << 16
                | (bytes[offset + 3] & 0xff) << 24;
    }

    /**
     * The largest picture a BREW module information file ({@code .mif})
     * carries.
     *
     * <p>A BREW package is a {@code .mod} beside its {@code .mif} and the
     * title's own data files, and no picture of its own - the handset menu's
     * icons are inside the {@code .mif}. Its span table is at the offset the
     * word at {@code 0x10} names, with the count at {@code 0x14}, one offset per
     * span and then the end of the last. An image span opens with a 16-bit
     * length covering itself and a NUL-terminated MIME type, {@code image/bmp}
     * in 카샨's, and the picture follows: there a 120x80 icon and a 20x20 one.
     */
    private static Bitmap decodeBrewIcon(byte[] bytes) {
        final int maxSpans = 64;

        if (bytes.length < 0x20) {
            return null;
        }

        int table = readLittleEndianInt(bytes, 0x10);
        int count = readLittleEndianInt(bytes, 0x14);
        if (count <= 0 || count > maxSpans || table < 0 || (long) table + 4L * (count + 1) > bytes.length) {
            return null;
        }

        Bitmap best = null;
        for (int index = 0; index < count; index++) {
            int start = readLittleEndianInt(bytes, table + 4 * index);
            int end = readLittleEndianInt(bytes, table + 4 * (index + 1));
            if (start < 0 || end > bytes.length || end - start < 4) {
                continue;
            }

            int headerLength = (bytes[start] & 0xff) | (bytes[start + 1] & 0xff) << 8;
            if (headerLength < 8 || headerLength >= end - start || bytes[start + headerLength - 1] != 0) {
                continue;
            }
            String type = new String(bytes, start + 2, headerLength - 3, StandardCharsets.ISO_8859_1);
            if (!type.startsWith("image/")) {
                continue;
            }

            Bitmap bitmap = BitmapFactory.decodeByteArray(bytes, start + headerLength, end - start - headerLength);
            if (bitmap != null && (best == null || bitmap.getWidth() * bitmap.getHeight() > best.getWidth() * best.getHeight())) {
                best = bitmap;
            }
        }

        return best;
    }

    /** Whether these bytes open the way an image these archives carry does. */
    private static boolean looksLikeImage(byte[] header, int length) {
        if (length < 4) {
            return false;
        }

        boolean png = (header[0] & 0xff) == 0x89 && header[1] == 'P' && header[2] == 'N' && header[3] == 'G';
        boolean bmp = header[0] == 'B' && header[1] == 'M';
        boolean jpeg = (header[0] & 0xff) == 0xff && (header[1] & 0xff) == 0xd8;

        return png || bmp || jpeg;
    }

    // --- import ----------------------------------------------------------

    private void openPicker() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{
                "application/vnd.android.package-archive",
                "application/zip",
                "application/java-archive",
                "application/octet-stream",
        });
        // Let the player pick several games at once.
        intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
        startActivityForResult(intent, PICK_GAME);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);

        if (ControlPatch.onActivityResult(this, requestCode, resultCode, data)) {
            return;
        }

        if (resultCode != RESULT_OK || data == null) {
            return;
        }
        if (requestCode != PICK_GAME && requestCode != PICK_SAVE) {
            return;
        }

        if (requestCode == PICK_SAVE) {
            Uri uri = data.getData();
            if (uri == null) {
                return;
            }
            Toast.makeText(this, "세이브를 불러오는 중...", Toast.LENGTH_SHORT).show();
            importSaveNow(uri);
            return;
        }

        // A multi-select returns the picks as ClipData; a single pick as getData.
        java.util.ArrayList<Uri> uris = new java.util.ArrayList<>();
        android.content.ClipData clip = data.getClipData();
        if (clip != null) {
            for (int i = 0; i < clip.getItemCount(); i++) {
                Uri item = clip.getItemAt(i).getUri();
                if (item != null) {
                    uris.add(item);
                }
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }
        if (uris.isEmpty()) {
            return;
        }

        Toast.makeText(this, uris.size() == 1 ? "게임을 가져오는 중..." : (uris.size() + "개 게임을 가져오는 중..."), Toast.LENGTH_SHORT).show();
        emulatorThread.execute(() -> importGames(uris));
    }

    /** Imports each picked game, then refreshes the library once at the end. */
    private void importGames(java.util.List<Uri> uris) {
        int done = 0;
        String lastError = null;
        for (Uri uri : uris) {
            try {
                importGameFile(uri);
                done++;
            } catch (Exception e) {
                lastError = e.getMessage();
            }
        }
        final int ok = done;
        final int failed = uris.size() - done;
        final String error = lastError;
        runOnUiThread(() -> {
            String message;
            if (failed == 0) {
                message = ok == 1 ? "가져오기 완료" : ok + "개 가져오기 완료";
            } else if (ok == 0) {
                message = "가져오기 실패" + (error != null ? ": " + error : "");
            } else {
                message = ok + "개 완료 · " + failed + "개 실패";
            }
            Toast.makeText(this, message, failed == 0 ? Toast.LENGTH_SHORT : Toast.LENGTH_LONG).show();
            showLibrary();
        });
    }

    /** Copies one picked game into the library. Throws on failure. */
    private void importGameFile(Uri uri) throws Exception {
        String name = queryName(uri).replaceAll("[^A-Za-z0-9가-힣._ -]", "_");
        if (name.isEmpty()) {
            name = "game_" + System.currentTimeMillis() + ".zip";
        }

        File target = uniqueFile(name);
        try (InputStream input = getContentResolver().openInputStream(uri);
             FileOutputStream output = new FileOutputStream(target)) {
            if (input == null) {
                throw new IllegalStateException("파일을 열 수 없습니다.");
            }

            byte[] chunk = new byte[32768];
            int read;
            while ((read = input.read(chunk)) >= 0) {
                output.write(chunk, 0, read);
            }
        } catch (Exception e) {
            target.delete();
            throw e;
        }
    }

    private String queryName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                return cursor.getString(0);
            }
        } catch (Exception e) {
            // Providers are free to reject the query; fall through to the path.
        }

        String last = uri.getLastPathSegment();
        return last != null ? last : "game.zip";
    }

    private File uniqueFile(String name) {
        File candidate = new File(gamesDir, name);
        if (!candidate.exists()) {
            return candidate;
        }

        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String extension = dot > 0 ? name.substring(dot) : "";

        for (int index = 2; ; index++) {
            candidate = new File(gamesDir, stem + " (" + index + ")" + extension);
            if (!candidate.exists()) {
                return candidate;
            }
        }
    }

    // --- player ----------------------------------------------------------

    private void showPlayer(File game) {
        playerVisible = true;
        running = false;
        paused = false;
        currentGame = game;
        currentGameName = displayName(game);
        keypadHidden = gameKeypadHidden(game);
        framePainted = false;
        // Whichever way the phone is being held: the player opens the way the
        // window already is, not the way the last one was.
        landscapeMode = getResources().getConfiguration().orientation == Configuration.ORIENTATION_LANDSCAPE;
        orientationPinned = false;
        // The player is a dark device again, so restore light status-bar icons.
        setLightStatusBar(false);
        // The game and keypad take the whole screen; the bars come back on a
        // swipe from an edge and slide away again.
        applyImmersive(true);

        // Persistent views, kept across rotations so the last frame and any
        // held keys survive a re-layout instead of being torn down.
        gameView = new GameView(this);
        keypad = new KeypadView(this);

        // The phone decides, the way it decides for everything else:
        // SCREEN_ORIENTATION_USER follows the sensor while the phone's
        // auto-rotate is on and holds the orientation the user locked while it
        // is off. The title-bar toggle then only has to say what the phone is
        // not already saying - see `toggleOrientation`.
        //
        // Except a player that opens on its side with auto-rotate off: that is
        // held in landscape the way the toggle holds it, so it still turns
        // over with the phone - see `heldOrientation`.
        if (landscapeMode && !autoRotateOn()) {
            orientationPinned = true;
            setRequestedOrientation(heldOrientation());
        } else {
            setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_USER);
        }
        buildPlayerContent();

        wedgeReported = false;
        busyReported = false;
        // Seeded from the counter as it stands, so the first poll compares
        // against this run rather than reading a jump from zero as progress.
        lastGuestProgress = NativeBridge.nativeGuestProgress();
        wedgeWatch.removeCallbacks(watchForWedge);
        wedgeWatch.postDelayed(watchForWedge, WEDGE_POLL_MS);

        emulatorThread.execute(() -> startGame(game));
    }

    /**
     * Lays the player out for the current orientation, reusing the persistent
     * game and keypad views. Portrait stacks the screen over the keypad;
     * landscape floats the screen in the gap between the two key columns.
     */
    private void buildPlayerContent() {
        detach(gameView);
        detach(keypad);
        // With the keypad put away the screen has the whole area, fitted to
        // it at its own shape either way up.
        gameView.landscape = landscapeMode && !keypadHidden;
        keypad.landscape = landscapeMode;
        keypad.requestLayout();

        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setBackgroundColor(COLOR_BG);

        if (keypadHidden) {
            gameView.setBackgroundColor(Color.BLACK);
            content.setBackgroundColor(Color.BLACK);
            content.addView(gameView, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        } else if (landscapeMode) {
            // One keypad view across the whole area with the two key columns,
            // the screen floated over the empty gap between them, so a finger
            // on each side is still one view's business.
            FrameLayout arena = new FrameLayout(this);
            arena.setBackgroundColor(COLOR_KEYPAD_TRAY);
            arena.addView(keypad,
                    new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
            FrameLayout.LayoutParams screenParams =
                    new FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.MATCH_PARENT);
            screenParams.gravity = android.view.Gravity.CENTER;
            gameView.setBackgroundColor(COLOR_SCREEN_BEZEL);
            arena.addView(gameView, screenParams);
            content.addView(arena, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        } else {
            gameView.setBackgroundColor(COLOR_SCREEN_BEZEL);
            content.addView(gameView, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, GAME_WEIGHT));
            content.addView(keypad, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, KEYPAD_WEIGHT));
        }

        // The title bar is gone so the screen gets its space back. Everything it
        // held - log collect/stop, the control settings and the rotate toggle -
        // now lives behind a single translucent gear floating over the screen's
        // top-right corner (see showGameMenu).
        FrameLayout overlay = new FrameLayout(this);
        overlay.addView(content,
                new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        FrameLayout.LayoutParams gearParams = new FrameLayout.LayoutParams(dp(38), dp(38));
        gearParams.gravity = android.view.Gravity.TOP | android.view.Gravity.END;
        gearParams.topMargin = dp(8);
        gearParams.rightMargin = dp(8);
        overlay.addView(buildGearButton(), gearParams);

        applyStatusBarInset(overlay);
        setContentView(overlay);
        ControlPatch.onPlayerBuilt(this);
    }

    /**
     * The translucent gear that stands in for the whole title bar. It floats
     * over the screen rather than taking a row of its own, so the game gets the
     * space the bar used to occupy.
     */
    private Button buildGearButton() {
        Button gear = new Button(this);
        gear.setText("⚙");
        gear.setAllCaps(false);
        gear.setTextSize(18f);
        gear.setTextColor(Color.argb(235, 255, 255, 255));
        gear.setPadding(0, 0, 0, 0);
        gear.setMinWidth(0);
        gear.setMinimumWidth(0);
        gear.setMinHeight(0);
        gear.setMinimumHeight(0);
        gear.setContentDescription("게임 메뉴");
        GradientDrawable bg = new GradientDrawable();
        bg.setShape(GradientDrawable.OVAL);
        bg.setColor(Color.argb(140, 0, 0, 0));
        bg.setStroke(Math.max(1, dp(1)), Color.argb(90, 255, 255, 255));
        gear.setBackground(bg);
        gear.setAlpha(gearOpacity() / 100f);
        gear.setOnClickListener(v -> showGameMenu());
        gear.setOnLongClickListener(v -> {
            showGearOpacityDialog(gear);
            return true;
        });
        return gear;
    }

    private int gearOpacity() {
        int v = getSharedPreferences("mini_ui", MODE_PRIVATE).getInt("gear_opacity", 70);
        return Math.max(15, Math.min(100, v));
    }

    private void setGearOpacity(int value) {
        getSharedPreferences("mini_ui", MODE_PRIVATE).edit().putInt("gear_opacity", value).apply();
    }

    /**
     * Long-pressing the gear opens this: a slider that changes the gear's
     * transparency live (15–100%). 적용 keeps it, 취소 or dismissing puts the
     * old value back, 기본값 resets to 70%.
     */
    private void showGearOpacityDialog(final Button gear) {
        final int original = gearOpacity();
        final int[] current = {original};

        int themeId = getResources().getIdentifier("MiniControlsDialogThemeDark", "style", getPackageName());
        android.view.ContextThemeWrapper ctx = new android.view.ContextThemeWrapper(
                this, themeId != 0 ? themeId : android.R.style.Theme_Material_Dialog_Alert);

        LinearLayout content = new LinearLayout(ctx);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(22), dp(10), dp(22), dp(4));

        LinearLayout labelRow = new LinearLayout(ctx);
        labelRow.setGravity(android.view.Gravity.CENTER_VERTICAL);
        TextView label = new TextView(ctx);
        label.setText("투명도");
        label.setTextColor(COLOR_SUBTEXT);
        label.setTextSize(13f);
        labelRow.addView(label, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        final TextView value = new TextView(ctx);
        value.setText(original + "%");
        value.setTextColor(COLOR_ACCENT);
        value.setTextSize(16f);
        value.setTypeface(Typeface.DEFAULT_BOLD);
        labelRow.addView(value);
        content.addView(labelRow);

        final android.widget.SeekBar bar = new android.widget.SeekBar(ctx);
        bar.setMax(85);
        bar.setProgress(original - 15);
        bar.setOnSeekBarChangeListener(new android.widget.SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(android.widget.SeekBar seekBar, int progress, boolean fromUser) {
                int v = 15 + progress;
                current[0] = v;
                value.setText(v + "%");
                gear.setAlpha(v / 100f);
            }

            @Override
            public void onStartTrackingTouch(android.widget.SeekBar seekBar) {
            }

            @Override
            public void onStopTrackingTouch(android.widget.SeekBar seekBar) {
            }
        });
        LinearLayout.LayoutParams barParams = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        barParams.topMargin = dp(6);
        content.addView(bar, barParams);

        LinearLayout ends = new LinearLayout(ctx);
        TextView low = new TextView(ctx);
        low.setText("흐리게 15%");
        low.setTextColor(COLOR_SUBTEXT);
        low.setTextSize(11f);
        ends.addView(low, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        TextView high = new TextView(ctx);
        high.setText("진하게 100%");
        high.setTextColor(COLOR_SUBTEXT);
        high.setTextSize(11f);
        high.setGravity(android.view.Gravity.END);
        ends.addView(high, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        content.addView(ends);

        AlertDialog dialog = new AlertDialog.Builder(ctx)
                .setTitle("메뉴 버튼 투명도")
                .setView(content)
                .setPositiveButton("적용", (d, w) -> setGearOpacity(current[0]))
                .setNegativeButton("취소", (d, w) -> gear.setAlpha(original / 100f))
                .setNeutralButton("기본값", null)
                .setOnCancelListener(d -> gear.setAlpha(original / 100f))
                .create();
        dialog.show();
        // 기본값: reset to 70% without closing (the slider drives the live gear).
        dialog.getButton(AlertDialog.BUTTON_NEUTRAL).setOnClickListener(v -> bar.setProgress(70 - 15));
    }

    /**
     * What the rotate menu item says, which is what tapping it does. With the
     * phone's auto-rotate on it locks or unlocks the current orientation
     * (고정/해제); with auto-rotate off it forces the other orientation.
     */
    private String rotateMenuLabel() {
        if (autoRotateOn()) {
            return orientationPinned ? "화면 고정 해제" : "화면 고정";
        }
        return landscapeMode ? "세로 화면으로" : "가로 화면으로";
    }

    /**
     * The list the gear opens: the same actions the title bar carried, chosen
     * from a menu and applied on tap. Log collect is one entry that reads start
     * or stop from the current state; rotate names the orientation it switches
     * to.
     */
    private void showGameMenu() {
        boolean collecting = NativeBridge.nativeLogCollecting() != 0;
        List<GameMenuRow> rows = new ArrayList<>();
        rows.add(new GameMenuRow("⌨", keypadHidden ? "키패드 꺼내기" : "키패드 숨기기", null, this::toggleKeypad));
        rows.add(new GameMenuRow("📋", collecting ? "로그 수집 종료·저장" : "로그 수집 시작", null, () -> {
            if (collecting) {
                stopLogCollectAndSave();
            } else {
                startLogCollect();
            }
        }));
        rows.add(new GameMenuRow("🔧", "로그 진단 설정", null, this::showDiagnosticsDialog));
        rows.add(new GameMenuRow("⚙", "조작 설정 (키패드·게임패드)", null, () -> ControlPatch.showSettings(this)));
        rows.add(new GameMenuRow("🔄", rotateMenuLabel(), null, this::toggleOrientation));
        // The speed row says what the speed is, so the menu answers the
        // question without opening anything.
        String speedValue = currentGame != null ? formatSpeed(gameSpeed(currentGame)) : formatSpeed(1f);
        rows.add(new GameMenuRow("⏩", "게임 속도", speedValue, this::showSpeedDialog));
        rows.add(new GameMenuRow("🖼", "화질", QUALITY_NAMES[screenQuality], this::showQualityDialog));
        rows.add(new GameMenuRow("👆", "화면 터치", touchOn ? "켜짐" : "꺼짐", this::toggleTouch));
        // A phone with its own keypad may have no touch screen to reach back
        // through, so the way out is here too.
        if (hasPhoneKeypad()) {
            rows.add(new GameMenuRow("⏏", "게임 끝내기", null, this::confirmLeaveGame));
        }
        List<String> labels = new ArrayList<>();
        for (GameMenuRow row : rows) {
            labels.add(row.label);
        }

        android.widget.ArrayAdapter<String> adapter =
                new android.widget.ArrayAdapter<String>(this, 0, labels) {
                    @Override
                    public View getView(int position, View convertView, ViewGroup parent) {
                        LinearLayout line = new LinearLayout(MainActivity.this);
                        line.setOrientation(LinearLayout.HORIZONTAL);
                        line.setGravity(android.view.Gravity.CENTER_VERTICAL);
                        line.setPadding(dp(18), dp(12), dp(18), dp(12));
                        TextView icon = new TextView(MainActivity.this);
                        GameMenuRow row = rows.get(position);
                        icon.setText(row.icon);
                        icon.setTextSize(15f);
                        icon.setGravity(android.view.Gravity.CENTER);
                        GradientDrawable box = new GradientDrawable();
                        box.setColor(COLOR_PANEL_2);
                        box.setCornerRadius(dp(8));
                        box.setStroke(Math.max(1, dp(1)), COLOR_HAIR);
                        icon.setBackground(box);
                        LinearLayout.LayoutParams ip = new LinearLayout.LayoutParams(dp(30), dp(30));
                        ip.rightMargin = dp(12);
                        line.addView(icon, ip);
                        TextView label = new TextView(MainActivity.this);
                        label.setText(getItem(position));
                        label.setTextColor(COLOR_TEXT);
                        label.setTextSize(15f);
                        line.addView(label, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
                        String shown = row.value;
                        if (shown != null) {
                            TextView value = new TextView(MainActivity.this);
                            value.setText(shown);
                            boolean off = "꺼짐".equals(shown);
                            value.setTextColor(off ? COLOR_SUBTEXT : COLOR_ACCENT);
                            value.setTextSize(15f);
                            value.setTypeface(Typeface.DEFAULT_BOLD);
                            line.addView(value);
                        }
                        return line;
                    }
                };
        new AlertDialog.Builder(new android.view.ContextThemeWrapper(this, android.R.style.Theme_Material_Dialog_Alert))
                .setTitle(running && currentGameName != null ? currentGameName : "게임")
                .setAdapter(adapter, (dialog, which) -> rows.get(which).action.run())
                .setNegativeButton("닫기", null)
                .show();
    }

    /** A row of the game menu: its icon, label, the value on its right (or null) and what a tap does. */
    private static final class GameMenuRow {
        final String icon;
        final String label;
        final String value;
        final Runnable action;

        GameMenuRow(String icon, String label, String value, Runnable action) {
            this.icon = icon;
            this.label = label;
            this.value = value;
            this.action = action;
        }
    }

    /**
     * Whether the phone has a handset's number pad of its own - a folder phone
     * running Android - rather than only a touch screen.
     */
    private boolean hasPhoneKeypad() {
        return getResources().getConfiguration().keyboard == Configuration.KEYBOARD_12KEY;
    }

    /**
     * Whether the keypad was put away when this title was last played. A
     * title never played puts it away on a phone with a keypad of its own,
     * whose keys play it.
     */
    private boolean gameKeypadHidden(File game) {
        return getSharedPreferences("mini_keypad", MODE_PRIVATE).getBoolean(game.getName(), hasPhoneKeypad());
    }

    /**
     * Puts the on-screen keypad away, leaving the whole screen to the game,
     * or brings it back; kept for the running title.
     */
    private void toggleKeypad() {
        File game = currentGame;
        if (game == null || gameView == null || keypad == null) {
            return;
        }
        releaseKeypad();
        keypadHidden = !keypadHidden;
        getSharedPreferences("mini_keypad", MODE_PRIVATE).edit().putBoolean(game.getName(), keypadHidden).apply();
        buildPlayerContent();
        Toast.makeText(this,
                keypadHidden ? "키패드를 숨겼습니다. ⚙ → 키패드 꺼내기로 되돌립니다." : "키패드를 꺼냈습니다.",
                Toast.LENGTH_SHORT).show();
    }

    /** Asks before leaving the game for the library, the game held still meanwhile. */
    private void confirmLeaveGame() {
        paused = true;
        new AlertDialog.Builder(this)
                .setTitle("종료")
                .setMessage("애플리케이션을 종료하시겠습니까?")
                .setPositiveButton("예", (dialog, which) -> exitGameToLibrary())
                .setNegativeButton("아니요", (dialog, which) -> paused = false)
                .setOnCancelListener(dialog -> paused = false)
                .show();
    }

    /**
     * Back on a phone's own keypad, in a game: a tap is the handset's clear
     * key (취소), which is what that key is on the phone; held, it opens the
     * game menu instead - the way to the menu on a phone that may have no
     * touch screen. Whether the event was taken.
     */
    private boolean phoneBackKey(KeyEvent event) {
        if (event.getKeyCode() != KeyEvent.KEYCODE_BACK || !playerVisible || !hasPhoneKeypad()
                || keyMapVisible || ControlPatch.editing(this)
                || event.getDeviceId() == android.view.KeyCharacterMap.VIRTUAL_KEYBOARD) {
            return false;
        }
        if (event.getAction() == KeyEvent.ACTION_DOWN) {
            if (event.getRepeatCount() == 0) {
                backHeldLong = false;
                if (backHold != null) {
                    wedgeWatch.removeCallbacks(backHold);
                }
                backHold = () -> {
                    backHold = null;
                    backHeldLong = true;
                    showGameMenu();
                };
                wedgeWatch.postDelayed(backHold, BACK_HOLD_MS);
            }
            return true;
        }
        if (event.getAction() == KeyEvent.ACTION_UP) {
            if (backHold != null) {
                wedgeWatch.removeCallbacks(backHold);
                backHold = null;
            }
            if (!backHeldLong && !event.isCanceled()) {
                // A tap: pressed and let go, the release a moment later so a
                // title that reads the key as held still sees it down.
                sendKey(CODE_CLEAR, true);
                wedgeWatch.postDelayed(() -> sendKey(CODE_CLEAR, false), 60);
            }
            backHeldLong = false;
            return true;
        }
        return true;
    }

    /**
     * Whether touches on the game screen reach the running title. Off unless
     * the player turned it on for this title - see {@link #toggleTouch}.
     */
    private volatile boolean touchOn;

    /** Whether touch was on when this title was last played. Kept per title, as the speed is. */
    private boolean gameTouch(File game) {
        return getSharedPreferences("mini_touch", MODE_PRIVATE).getBoolean(game.getName(), false);
    }

    /**
     * Turns touch on the game screen on or off for the running title and keeps
     * the choice for it.
     *
     * <p>Most titles were made for keypad handsets and hear nothing from a
     * touch, so it is off unless asked for: a title made for a touch handset
     * (풀터치폰용) takes taps on its own screen when it is on. A title that asks
     * whether the handset has a touch screen asks once, as it starts, so one
     * that lays itself out for touch only does so after a restart.
     */
    private void toggleTouch() {
        File game = currentGame;
        if (game == null) {
            return;
        }
        boolean enabled = !touchOn;
        if (!enabled && gameView != null) {
            gameView.liftTouch();
        }
        touchOn = enabled;
        getSharedPreferences("mini_touch", MODE_PRIVATE).edit().putBoolean(game.getName(), enabled).apply();
        NativeBridge.nativeSetTouch(enabled ? 1 : 0);
        Toast.makeText(this,
                enabled
                        ? "화면 터치를 켰습니다. 터치를 지원하는 게임은 화면을 눌러 조작할 수 있습니다. (게임에 따라 다시 시작해야 적용됩니다)"
                        : "화면 터치를 껐습니다.",
                Toast.LENGTH_LONG).show();
    }

    /** The screen enlarged with smoothing: the "기본" choice. */
    static final int QUALITY_SMOOTH = 0;
    /** The screen enlarged pixel for pixel, as it always was: "도트". */
    static final int QUALITY_DOT = 1;
    /** The screen doubled through hq2x first, then enlarged with smoothing. */
    static final int QUALITY_HQ2X = 2;

    private static final String[] QUALITY_NAMES = {"기본", "도트", "HQ2X"};
    /** The key, among the per-title ones, for the choice made for every title. */
    private static final String QUALITY_EVERY_GAME = "*";

    /**
     * How the running title's screen is enlarged. Read by the emulator thread,
     * which doubles each frame through hq2x before handing it over when this
     * says to.
     */
    private volatile int screenQuality = QUALITY_DOT;

    /**
     * How `game`'s screen is enlarged: its own choice, else the one made for
     * every title, else 도트 - which is how the screen was always drawn.
     */
    private int gameQuality(File game) {
        SharedPreferences prefs = getSharedPreferences("mini_quality", MODE_PRIVATE);
        int value = prefs.getInt(game.getName(), prefs.getInt(QUALITY_EVERY_GAME, QUALITY_DOT));

        return value >= QUALITY_SMOOTH && value <= QUALITY_HQ2X ? value : QUALITY_DOT;
    }

    /**
     * Keeps `quality` for `game`, or for every title - dropping each one's own
     * choice, so they all follow it.
     */
    private void saveGameQuality(File game, int quality, boolean everyGame) {
        SharedPreferences.Editor editor = getSharedPreferences("mini_quality", MODE_PRIVATE).edit();
        if (everyGame) {
            editor.clear().putInt(QUALITY_EVERY_GAME, quality);
        } else {
            editor.putInt(game.getName(), quality);
        }
        editor.apply();
    }

    /**
     * The quality window from the gear menu: 기본, 도트 and HQ2X, each with a
     * picture of the middle of the screen drawn that way. A choice shows on
     * the screen behind at once; 취소 puts it back.
     */
    private void showQualityDialog() {
        final File game = currentGame;
        if (game == null || gameView == null) {
            return;
        }
        final int saved = screenQuality;
        final int[] chosen = {saved};

        LinearLayout body = new LinearLayout(this);
        body.setOrientation(LinearLayout.VERTICAL);
        body.setPadding(dp(16), 0, dp(16), dp(4));
        body.addView(settingScope("이 게임에만 적용 · 다음에 열어도 유지"));

        Bitmap[] previews = qualityPreviews(gameView.sourceFrame());
        String[] descriptions = {
                "부드럽게 확대해요. 픽셀 경계가 살짝 흐려져요.",
                "픽셀을 그대로 키워요. 또렷하고 각진 옛날 폰 느낌.",
                "계단진 테두리를 매끈하게 다듬어 그려요.",
        };
        final View[] cards = new View[3];
        final Runnable refresh = () -> {
            for (int i = 0; i < cards.length; i++) {
                markOption(cards[i], i == chosen[0]);
            }
        };
        for (int i = 0; i < 3; i++) {
            final int quality = i;
            View card = optionCard(QUALITY_NAMES[i], i == QUALITY_DOT ? "지금 방식" : null, descriptions[i],
                    previews != null ? previews[i] : null, null);
            card.setOnClickListener(v -> {
                chosen[0] = quality;
                showQuality(quality);
                refresh.run();
            });
            cards[i] = card;
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            params.topMargin = i > 0 ? dp(8) : 0;
            body.addView(card, params);
        }

        final android.widget.CheckBox everyGame = new android.widget.CheckBox(this);
        everyGame.setText("모든 게임에 이 화질 쓰기");
        everyGame.setTextColor(COLOR_SUBTEXT);
        everyGame.setTextSize(12.5f);
        everyGame.setButtonTintList(android.content.res.ColorStateList.valueOf(COLOR_ACCENT));
        LinearLayout.LayoutParams everyParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        everyParams.topMargin = dp(8);
        body.addView(everyGame, everyParams);
        refresh.run();

        final boolean[] applied = {false};
        AlertDialog dialog = new AlertDialog.Builder(new android.view.ContextThemeWrapper(this, android.R.style.Theme_Material_Dialog_Alert))
                .setTitle("화질")
                .setView(scrolling(body))
                .setPositiveButton("적용", (d, which) -> {
                    applied[0] = true;
                    saveGameQuality(game, chosen[0], everyGame.isChecked());
                    showQuality(chosen[0]);
                })
                .setNegativeButton("취소", null)
                .create();
        // However the window goes away without 적용, the screen goes back.
        dialog.setOnDismissListener(d -> {
            if (!applied[0]) {
                showQuality(saved);
            }
        });
        dialog.show();
    }

    /** Draws the running title's screen with `quality` from now on. */
    private void showQuality(int quality) {
        screenQuality = quality;
        if (gameView != null) {
            gameView.setQuality(quality);
        }
    }

    /**
     * The middle of `frame`, drawn each of the three ways at the size the
     * quality window shows it, or null when there is no frame yet.
     */
    private Bitmap[] qualityPreviews(short[] frame) {
        if (frame == null || frame.length < 2) {
            return null;
        }
        int width = frame[0] & 0xFFFF;
        int height = frame[1] & 0xFFFF;
        int cropWidth = Math.min(48, width);
        int cropHeight = Math.min(36, height);
        if (cropWidth <= 0 || cropHeight <= 0 || frame.length != width * height + 2) {
            return null;
        }
        int left = (width - cropWidth) / 2;
        int top = (height - cropHeight) / 2;

        short[] crop = new short[cropWidth * cropHeight + 2];
        crop[0] = (short) cropWidth;
        crop[1] = (short) cropHeight;
        for (int y = 0; y < cropHeight; y++) {
            System.arraycopy(frame, 2 + (top + y) * width + left, crop, 2 + y * cropWidth, cropWidth);
        }

        int shownWidth = dp(84);
        int shownHeight = Math.round(shownWidth * cropHeight / (float) cropWidth);
        Bitmap plain = rgb565Bitmap(crop);
        short[] doubled = NativeBridge.nativeHq2x(crop);
        Bitmap smooth = doubled != null ? rgb565Bitmap(doubled) : plain;

        return new Bitmap[] {
                Bitmap.createScaledBitmap(plain, shownWidth, shownHeight, true),
                Bitmap.createScaledBitmap(plain, shownWidth, shownHeight, false),
                Bitmap.createScaledBitmap(smooth, shownWidth, shownHeight, true),
        };
    }

    /** A `{width, height, RGB565...}` frame as a bitmap. */
    private static Bitmap rgb565Bitmap(short[] frame) {
        int width = frame[0] & 0xFFFF;
        int height = frame[1] & 0xFFFF;
        Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.RGB_565);
        ByteBuffer pixels = ByteBuffer.allocateDirect(width * height * 2).order(ByteOrder.nativeOrder());
        pixels.asShortBuffer().put(frame, 2, width * height);
        bitmap.copyPixelsFromBuffer(pixels);
        return bitmap;
    }

    /** The grey "이 게임에만 적용" line under a setting window's title. */
    private TextView settingScope(String text) {
        TextView scope = new TextView(this);
        scope.setText(text);
        scope.setTextSize(12f);
        scope.setTextColor(COLOR_SUBTEXT);
        scope.setPadding(dp(6), 0, 0, dp(10));
        return scope;
    }

    /** `content` in a scroll view, for a window taller than a landscape screen. */
    private ScrollView scrolling(View content) {
        ScrollView scroll = new ScrollView(this);
        scroll.addView(content);
        return scroll;
    }

    /**
     * One choice in a setting window: an optional picture on the left, the
     * name with an optional tag, a line about it, an optional view under that,
     * and a radio mark on the right. {@link #markOption} shows it chosen.
     */
    private View optionCard(String title, String tag, String description, Bitmap picture, View below) {
        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.HORIZONTAL);
        card.setGravity(android.view.Gravity.CENTER_VERTICAL);
        card.setPadding(dp(8), dp(8), dp(10), dp(8));

        if (picture != null) {
            ImageView image = new ImageView(this);
            image.setImageBitmap(picture);
            image.setClipToOutline(true);
            image.setOutlineProvider(new ViewOutlineProvider() {
                @Override
                public void getOutline(View view, Outline outline) {
                    outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(), dp(6));
                }
            });
            LinearLayout.LayoutParams imageParams = new LinearLayout.LayoutParams(picture.getWidth(), picture.getHeight());
            imageParams.rightMargin = dp(11);
            card.addView(image, imageParams);
        }

        LinearLayout text = new LinearLayout(this);
        text.setOrientation(LinearLayout.VERTICAL);
        TextView name = new TextView(this);
        SpannableString label = new SpannableString(tag != null ? title + "  " + tag : title);
        if (tag != null) {
            label.setSpan(new ForegroundColorSpan(COLOR_SUBTEXT), title.length(), label.length(), Spannable.SPAN_EXCLUSIVE_EXCLUSIVE);
            label.setSpan(new android.text.style.RelativeSizeSpan(0.72f), title.length(), label.length(), Spannable.SPAN_EXCLUSIVE_EXCLUSIVE);
        }
        name.setText(label);
        name.setTextSize(15f);
        name.setTypeface(Typeface.DEFAULT_BOLD);
        name.setTextColor(COLOR_TEXT);
        text.addView(name);
        TextView line = new TextView(this);
        line.setText(description);
        line.setTextSize(12f);
        line.setTextColor(COLOR_SUBTEXT);
        line.setLineSpacing(0f, 1.15f);
        LinearLayout.LayoutParams lineParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lineParams.topMargin = dp(2);
        text.addView(line, lineParams);
        if (below != null) {
            LinearLayout.LayoutParams belowParams = new LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            belowParams.topMargin = dp(6);
            text.addView(below, belowParams);
        }
        card.addView(text, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        View radio = new View(this);
        radio.setTag("radio");
        LinearLayout.LayoutParams radioParams = new LinearLayout.LayoutParams(dp(18), dp(18));
        radioParams.leftMargin = dp(10);
        card.addView(radio, radioParams);
        return card;
    }

    /** Shows an {@link #optionCard} chosen or not: its outline and its radio. */
    private void markOption(View card, boolean on) {
        GradientDrawable face = new GradientDrawable();
        face.setCornerRadius(dp(10));
        face.setColor(on ? Color.argb(26, 84, 199, 214) : Color.TRANSPARENT);
        face.setStroke(Math.max(1, dp(1)), on ? COLOR_ACCENT : Color.rgb(58, 61, 71));
        card.setBackground(face);

        View radio = card.findViewWithTag("radio");
        GradientDrawable mark = new GradientDrawable();
        mark.setShape(GradientDrawable.OVAL);
        mark.setColor(on ? COLOR_ACCENT : Color.TRANSPARENT);
        mark.setStroke(dp(2), on ? COLOR_ACCENT : Color.rgb(107, 111, 123));
        radio.setBackground(mark);
    }

    /** The speeds the dialog offers as one-tap chips, in tenths. */
    private static final int[] SPEED_CHIPS = {5, 10, 15, 20, 30, 40};

    /**
     * The speed a title was last played at, 1.0 if it never left real time.
     * Kept per title, by its file name, so a slow title can stay sped up
     * without every other title following it.
     */
    private float gameSpeed(File game) {
        float value = getSharedPreferences("mini_speed", MODE_PRIVATE).getFloat(game.getName(), 1f);

        return Float.isNaN(value) || value <= 0f ? 1f : value;
    }

    private void saveGameSpeed(File game, float value) {
        getSharedPreferences("mini_speed", MODE_PRIVATE).edit().putFloat(game.getName(), value).apply();
    }

    /** 2x, 1.5x, 1.25x - as few digits as the value needs. */
    private static String formatSpeed(float value) {
        if (value == Math.round(value)) {
            return Math.round(value) + "x";
        }
        String text = String.format(java.util.Locale.US, "%.2f", value);
        if (text.endsWith("0")) {
            text = text.substring(0, text.length() - 1);
        }

        return text + "x";
    }

    /**
     * The game-speed window from the gear menu: the speed in large type with
     * a step either side of it, the ruler from 0.1x to 4x in tenths, and chips
     * for the common speeds. Nothing changes until 적용; 1x로 goes straight
     * back to real time.
     */
    private void showSpeedDialog() {
        final File game = currentGame;
        if (game == null) {
            return;
        }
        final float[] chosen = {gameSpeed(game)};

        LinearLayout body = new LinearLayout(this);
        body.setOrientation(LinearLayout.VERTICAL);
        body.setPadding(dp(20), dp(4), dp(20), dp(4));

        final TextView big = new TextView(this);
        big.setTextSize(40f);
        big.setTypeface(Typeface.DEFAULT_BOLD);
        big.setTextColor(COLOR_ACCENT);
        big.setGravity(android.view.Gravity.CENTER);

        final SpeedRuler ruler = new SpeedRuler(this, COLOR_TEXT, COLOR_SUBTEXT, COLOR_ACCENT);

        // − big + : a tenth slower or faster, through the ruler so it slides.
        LinearLayout stepper = new LinearLayout(this);
        stepper.setOrientation(LinearLayout.HORIZONTAL);
        stepper.setGravity(android.view.Gravity.CENTER);
        stepper.addView(speedStepButton("−", "0.1 느리게", () -> ruler.setTenths(ruler.tenths() - 1, true)),
                new LinearLayout.LayoutParams(dp(40), dp(40)));
        LinearLayout.LayoutParams bigParams = new LinearLayout.LayoutParams(dp(130), ViewGroup.LayoutParams.WRAP_CONTENT);
        stepper.addView(big, bigParams);
        stepper.addView(speedStepButton("+", "0.1 빠르게", () -> ruler.setTenths(ruler.tenths() + 1, true)),
                new LinearLayout.LayoutParams(dp(40), dp(40)));
        body.addView(stepper);

        TextView scope = new TextView(this);
        scope.setText("이 게임에만 적용 · 다음에 열어도 유지");
        scope.setTextSize(12.5f);
        scope.setTextColor(COLOR_SUBTEXT);
        scope.setGravity(android.view.Gravity.CENTER);
        LinearLayout.LayoutParams scopeParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        scopeParams.topMargin = dp(4);
        scopeParams.bottomMargin = dp(10);
        body.addView(scope, scopeParams);

        body.addView(ruler, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

        LinearLayout chips = new LinearLayout(this);
        chips.setOrientation(LinearLayout.HORIZONTAL);
        final TextView[] chipViews = new TextView[SPEED_CHIPS.length];

        final Runnable refresh = () -> {
            big.setText(formatSpeed(chosen[0]));
            for (int i = 0; i < chipViews.length; i++) {
                boolean on = Math.abs(SPEED_CHIPS[i] / 10f - chosen[0]) < 0.001f;
                GradientDrawable face = new GradientDrawable();
                face.setCornerRadius(dp(18));
                face.setColor(on ? COLOR_ACCENT : Color.TRANSPARENT);
                face.setStroke(Math.max(1, dp(1)), on ? COLOR_ACCENT : Color.rgb(85, 90, 102));
                chipViews[i].setBackground(face);
                chipViews[i].setTextColor(on ? Color.rgb(11, 42, 47) : COLOR_TEXT);
            }
        };

        for (int i = 0; i < SPEED_CHIPS.length; i++) {
            final int tenths = SPEED_CHIPS[i];
            TextView chip = new TextView(this);
            chip.setText(SpeedRuler.format(tenths));
            chip.setTextSize(13.5f);
            chip.setTypeface(Typeface.DEFAULT_BOLD);
            chip.setGravity(android.view.Gravity.CENTER);
            chip.setPadding(0, dp(8), 0, dp(8));
            chip.setOnClickListener(v -> ruler.setTenths(tenths, true));
            LinearLayout.LayoutParams chipParams = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
            if (i > 0) {
                chipParams.leftMargin = dp(5);
            }
            chips.addView(chip, chipParams);
            chipViews[i] = chip;
        }
        LinearLayout.LayoutParams chipsParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        chipsParams.topMargin = dp(8);
        body.addView(chips, chipsParams);

        TextView note = new TextView(this);
        note.setText("줄자를 옆으로 밀거나 −/+ 로 0.1씩 맞출 수 있어요. 빠르게 하면 게임 시간이 그만큼 빨리 흐르고, 무거운 장면에선 폰 성능만큼만 빨라질 수 있어요. 소리는 원래 속도로 재생됩니다.");
        note.setTextSize(12f);
        note.setTextColor(COLOR_SUBTEXT);
        note.setLineSpacing(0f, 1.2f);
        LinearLayout.LayoutParams noteParams = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        noteParams.topMargin = dp(14);
        body.addView(note, noteParams);

        // A speed saved off the tenths (a quarter, from before the ruler) is
        // shown as it is until the ruler moves.
        ruler.setTenths(Math.round(chosen[0] * 10f), false);
        ruler.setListener(tenths -> {
            chosen[0] = tenths / 10f;
            refresh.run();
        });
        refresh.run();

        new AlertDialog.Builder(new android.view.ContextThemeWrapper(this, android.R.style.Theme_Material_Dialog_Alert))
                .setTitle("게임 속도")
                .setView(body)
                .setPositiveButton("적용", (dialog, which) -> applyGameSpeed(game, chosen[0]))
                .setNegativeButton("취소", null)
                .setNeutralButton("1x로", (dialog, which) -> applyGameSpeed(game, 1f))
                .show();
    }

    /** A round − or + beside the speed. */
    private TextView speedStepButton(String text, String description, Runnable action) {
        TextView button = new TextView(this);
        button.setText(text);
        button.setContentDescription(description);
        button.setTextSize(20f);
        button.setTextColor(COLOR_TEXT);
        button.setGravity(android.view.Gravity.CENTER);
        GradientDrawable face = new GradientDrawable();
        face.setShape(GradientDrawable.OVAL);
        face.setStroke(Math.max(1, dp(1)), Color.rgb(85, 90, 102));
        button.setBackground(face);
        button.setOnClickListener(v -> action.run());
        return button;
    }

    /** Remembers the speed for this title and puts the running one on it. */
    private void applyGameSpeed(File game, float value) {
        saveGameSpeed(game, value);
        if (game.equals(currentGame)) {
            NativeBridge.nativeSetSpeed(value);
        }
    }

    /** Sets the (now optional) status label if one is present. */
    private void setPlayerStatus(String text) {
        if (playerStatus != null) {
            playerStatus.setText(text);
        }
    }

    /** Status line, with the rotate and log buttons in the top-right corner. */
    private View buildTitleBar() {
        LinearLayout bar = new LinearLayout(this);
        bar.setBackgroundColor(COLOR_PANEL);
        bar.setGravity(android.view.Gravity.CENTER_VERTICAL);

        playerStatus = new TextView(this);
        playerStatus.setText(running ? currentGameName : "게임을 시작하는 중...");
        playerStatus.setTextColor(COLOR_TEXT);
        playerStatus.setTextSize(15f);
        playerStatus.setGravity(android.view.Gravity.CENTER_VERTICAL);
        playerStatus.setPadding(dp(14), 0, dp(8), 0);
        playerStatus.setSingleLine(true);
        playerStatus.setEllipsize(android.text.TextUtils.TruncateAt.END);
        bar.addView(playerStatus, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f));

        // Collecting is two presses rather than one save, because what makes a
        // log worth reading is knowing where it starts. Between them every area
        // is captured at trace, which no single default filter can afford over a
        // whole run - and that is what stops each new question needing its own
        // build with its own instrumentation in it.
        collectButton = navyButton("수집");
        collectButton.setOnClickListener(v -> startLogCollect());
        // Long-press for the rest of it. Collecting itself needs nothing set:
        // a window records every area and where the code went. What is left
        // here is the one question a window cannot answer by itself - which
        // address to report writes to - and the filter the always-on capture
        // runs under between windows.
        collectButton.setOnLongClickListener(v -> {
            showDiagnosticsDialog();
            return true;
        });
        LinearLayout.LayoutParams collectParams = new LinearLayout.LayoutParams(dp(48), dp(34));
        collectParams.rightMargin = dp(6);
        bar.addView(collectButton, collectParams);

        stopButton = navyButton("종료");
        stopButton.setOnClickListener(v -> stopLogCollectAndSave());
        LinearLayout.LayoutParams stopParams = new LinearLayout.LayoutParams(dp(48), dp(34));
        stopParams.rightMargin = dp(8);
        bar.addView(stopButton, stopParams);

        showCollectState();

        // The way into keypad customization, per-button hide, per-button
        // rapid-fire, layout save/backup and gamepad mapping - all reachable
        // while a game is running.
        Button controlsButton = navyButton("설정");
        controlsButton.setContentDescription("키패드와 게임패드 설정");
        controlsButton.setOnClickListener(v -> ControlPatch.showSettings(this));
        LinearLayout.LayoutParams controlsParams = new LinearLayout.LayoutParams(dp(48), dp(34));
        controlsParams.rightMargin = dp(6);
        bar.addView(controlsButton, controlsParams);

        rotateButton = navyButton("");
        rotateButton.setOnClickListener(v -> toggleOrientation());
        showRotateState();
        LinearLayout.LayoutParams rotateParams = new LinearLayout.LayoutParams(dp(56), dp(34));
        rotateParams.rightMargin = dp(10);
        bar.addView(rotateButton, rotateParams);

        return bar;
    }

    /**
     * A small pill in the same dark-navy flat style as the keypad, for the
     * title bar's log and rotate buttons.
     */
    private Button navyButton(String label) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextSize(13f);
        button.setAllCaps(false);
        button.setTextColor(Color.rgb(182, 194, 216));
        button.setPadding(0, 0, 0, 0);

        GradientDrawable face = new GradientDrawable(
                GradientDrawable.Orientation.TOP_BOTTOM,
                new int[] {Color.rgb(46, 57, 84), Color.rgb(38, 48, 72)});
        face.setCornerRadius(dp(8));
        face.setStroke(Math.max(1, Math.round(dp(1) * 0.8f)), Color.rgb(24, 32, 52));
        button.setBackground(face);

        return button;
    }

    /**
     * Turns the player, or holds it where it is - whichever the phone has left
     * for it to do.
     *
     * <p>While the phone's auto-rotate is off the player only ever turns
     * because this was pressed, so it is the two-way switch it has always
     * been: it asks for the orientation the player is not in.
     *
     * <p>While auto-rotate is on the phone is already turning the player, and
     * turning it from here would only be undone by the next time the phone
     * moved. So the button stops being about which way and becomes about
     * whether: it holds the player in the orientation it is already in, for
     * playing lying down where the phone's idea of upright is not the
     * player's, and pressed again it hands the player back. It never turns
     * anything.
     *
     * <p>The window turning is what fires onConfigurationChanged, which flips
     * {@code landscapeMode} and relays the player out, so that callback stays
     * the one place the mode changes. Holding the player where it is turns
     * nothing, so the button is redrawn here too.
     */
    private void toggleOrientation() {
        if (autoRotateOn()) {
            orientationPinned = !orientationPinned;
            setRequestedOrientation(orientationPinned ? heldOrientation() : ActivityInfo.SCREEN_ORIENTATION_USER);
        } else {
            // Pinned either way, so the player stays put if auto-rotate is
            // turned on later and the button becomes 해제 rather than 고정.
            orientationPinned = true;
            setRequestedOrientation(landscapeMode
                    ? ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
                    : ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE);
        }

        showRotateState();
    }

    /**
     * The orientation the player is in, asked for as an orientation to keep.
     *
     * <p>Landscape is held as either side up. SENSOR_LANDSCAPE reads the
     * sensor even with the phone's auto-rotate off, so a player held on its
     * side turns over when the phone does - it never leaves landscape, it
     * only stops being upside down. Portrait stays the one way up: most phones
     * do not turn their own screen upside down either.
     */
    private int heldOrientation() {
        return landscapeMode
                ? ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                : ActivityInfo.SCREEN_ORIENTATION_PORTRAIT;
    }

    /** Whether the phone would turn its own screen if it were moved. */
    private boolean autoRotateOn() {
        return Settings.System.getInt(getContentResolver(), Settings.System.ACCELEROMETER_ROTATION, 0) == 1;
    }

    /**
     * What the rotate button says, which is what pressing it would do.
     *
     * <p>With the phone turning the player, that is 고정 to hold it where it is
     * and 해제 to give it back. With the phone not turning it, it is the
     * orientation the press would turn it to.
     */
    private void showRotateState() {
        if (rotateButton == null) {
            return;
        }

        String label = autoRotateOn()
                ? orientationPinned ? "해제" : "고정"
                : landscapeMode ? "세로" : "가로";

        rotateButton.setText(label);
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);

        // The activity is kept alive across rotation (see the manifest's
        // configChanges), so the game thread never stops - only the views move.
        if (playerVisible && gameView != null && keypad != null) {
            landscapeMode = newConfig.orientation == Configuration.ORIENTATION_LANDSCAPE;
            buildPlayerContent();
        }
    }

    private static void detach(View view) {
        if (view != null && view.getParent() instanceof ViewGroup) {
            ((ViewGroup) view.getParent()).removeView(view);
        }
    }

    private void startGame(File game) {
        try (FileInputStream input = new FileInputStream(game);
             ByteArrayOutputStream buffer = new ByteArrayOutputStream()) {
            byte[] chunk = new byte[32768];
            int read;
            while ((read = input.read(chunk)) >= 0) {
                buffer.write(chunk, 0, read);
            }

            File runtimeDir = new File(getFilesDir(), "runtime");
            if (!runtimeDir.exists()) {
                runtimeDir.mkdirs();
            }

            // A previous game's exit (onBackPressed) released the audio pump;
            // re-arm it here so this game produces sound. Idempotent for the
            // first game, where the pump is already running.
            audioOutput.start();

            // Snapshot the actual host handset model once for this emulator
            // instance so legacy PHONEMODEL queries retain their host value.
            String phoneModel = Build.MODEL != null ? Build.MODEL : "";

            // The speed this title was last played at; nativeStart then puts
            // the clock back on the time of day and runs it from there.
            NativeBridge.nativeSetSpeed(gameSpeed(game));
            // Touch is as the player left it for this title: off unless they
            // turned it on for one made for a touch handset.
            touchOn = gameTouch(game);
            NativeBridge.nativeSetTouch(touchOn ? 1 : 0);
            // The screen as the player left it for this title.
            int quality = gameQuality(game);
            screenQuality = quality;
            runOnUiThread(() -> {
                if (gameView != null) {
                    gameView.setQuality(quality);
                }
            });

            String message = NativeBridge.nativeStart(
                    buffer.toByteArray(),
                    runtimeDir.getAbsolutePath(),
                    phoneModel);
            running = NativeBridge.nativeRunning() != 0;

            runOnUiThread(() -> {
                if (running) {
                    setPlayerStatus("게임 초기화 중...");
                } else {
                    setPlayerStatus(message);
                    Toast.makeText(this, message, Toast.LENGTH_LONG).show();
                }
            });
        } catch (Exception e) {
            runOnUiThread(() -> {
                String msg = "실행 실패: " + e.getMessage();
                setPlayerStatus(msg);
                Toast.makeText(this, msg, Toast.LENGTH_LONG).show();
            });
        }
    }

    /**
     * Runs one step and re-arms itself at the interval that step earned.
     *
     * <p>This replaces a fixed-delay schedule, which charged an idle title and a
     * busy one the same pause - see {@link #TICK_BUDGET_MS}. Re-arming happens
     * in a {@code finally} so a step that throws does not silently cancel the
     * loop and leave the game frozen, which is what a periodic schedule does.
     *
     * <p>The budget is compared with a millisecond of slack because the clock
     * this is measured on counts in whole milliseconds, and the native side
     * starts its own deadline a moment after the reading here: a tick that ran
     * the whole budget can come back measuring one less, and charging it the
     * idle delay would give back most of what this is for.
     */
    private void emulatorLoop() {
        try {
            emulatorStep();
        } finally {
            long delay = nextStepDelayMs();
            scheduleEmulatorStep(delay);

            // A key that landed between reading the delay and arming the step
            // would find nothing to pull forward and wait the sleep out. Ask
            // again now that there is something waiting.
            if (delay > 0 && inputSinceStep.get()) {
                pullStepForward();
            }
        }
    }

    /**
     * How long to wait before the next step.
     *
     * <p>A tick that used its whole budget had work left and is re-armed at
     * once. One that came back early went idle, and the emulator is asked how
     * long that idleness lasts: waiting exactly that long lands on the title's
     * timer in one wakeup, where a fixed interval needed several and arrived
     * late. It answers -1 when it cannot say - a title spinning rather than
     * sleeping, or one already stopped - and then the fixed interval stands.
     */
    private long nextStepDelayMs() {
        if (lastTickRanMs + 1 >= TICK_BUDGET_MS) {
            return BUSY_INTERVAL_MS;
        }

        // A key that landed after this step took its input has not been acted on
        // yet, so there is work whatever the emulator thinks.
        if (inputSinceStep.get()) {
            return BUSY_INTERVAL_MS;
        }

        int hint = NativeBridge.nativeSleepHintMs();

        return hint < 0 ? TICK_INTERVAL_MS : Math.min(hint, MAX_IDLE_SLEEP_MS);
    }

    /** Queues the next step, unless the player is closing and the thread is gone. */
    private void scheduleEmulatorStep(long delayMs) {
        try {
            pendingStep = emulatorThread.schedule(this::emulatorLoop, delayMs, TimeUnit.MILLISECONDS);
        } catch (RejectedExecutionException closing) {
            // `shutdownNow` has run; there is nothing left to step.
        }
    }

    /**
     * Presses or releases a handset key, and brings the emulator's next step
     * forward so the sleep before it is not also the key's wait.
     */
    private void sendKey(int code, boolean pressed) {
        NativeBridge.nativeKey(code, pressed ? 1 : 0);

        inputSinceStep.set(true);
        pullStepForward();
    }

    /**
     * Runs the waiting step now, if one is still waiting.
     *
     * <p>A step already running is left alone - that is what `cancel(false)`
     * says - because taking it over mid-tick would be worse than the key waiting
     * for the end of it. {@link #inputSinceStep} is what covers that case: the
     * step that finishes then re-arms at once rather than sleeping.
     */
    private void pullStepForward() {
        ScheduledFuture<?> waiting = pendingStep;
        if (waiting != null && waiting.getDelay(TimeUnit.MILLISECONDS) > 0 && waiting.cancel(false)) {
            scheduleEmulatorStep(0);
        }
    }

    /**
     * One scheduled step: advance the emulator, drain audio, publish a frame.
     * Runs on the emulator thread.
     */
    private void emulatorStep() {
        // Nothing ran, so the step that re-arms this one owes the idle delay.
        lastTickRanMs = 0;

        if (!running || !playerVisible || !foreground || paused) {
            return;
        }

        // The tick drains the input inbox as it starts, so anything waiting now
        // is about to be seen; a key that arrives after this sets it again.
        inputSinceStep.set(false);

        PerformanceTuner.beforeNativeTick();
        tickStartedAt = SystemClock.elapsedRealtime();
        String status = NativeBridge.nativeTick(TICK_BUDGET_MS);
        lastTickRanMs = SystemClock.elapsedRealtime() - tickStartedAt;
        tickStartedAt = 0;
        PerformanceTuner.afterNativeTick();

        for (int i = 0; i < MAX_AUDIO_PER_TICK; i++) {
            byte[] command = NativeBridge.nativePollOutput();
            if (command == null) {
                break;
            }
            audioOutput.handle(command);
        }

        int backlightMode = NativeBridge.nativePollBacklightMode();
        if (backlightMode == 2) {
            runOnUiThread(() ->
                    getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON));
        }

        String phoneCall = NativeBridge.nativePollPhoneCall();
        if (phoneCall != null) {
            runOnUiThread(() -> placePhoneCall(phoneCall));
        }

        String browserUrl = NativeBridge.nativePollBrowserUrl();
        if (browserUrl != null) {
            runOnUiThread(() -> openBrowser(browserUrl));
        }

        if (NativeBridge.nativeRunning() == 0) {
            running = false;

            // A title that ends itself is not a title that broke. Several here
            // do it as part of working normally: 데몬헌터 builds its data on a
            // first run, asks to be started again and exits when the player
            // presses OK, and the run after that goes on to the game. Taking
            // the player back to the library is what the handset did, and it is
            // what makes "start it again" something they can just do. Nothing
            // went wrong, so nothing is saved.
            if (NativeBridge.nativeExitedByTitle() != 0) {
                runOnUiThread(() -> {
                    Toast.makeText(this, "게임이 종료되었습니다.", Toast.LENGTH_SHORT).show();
                    exitGameToLibrary();
                });
                return;
            }

            String error = NativeBridge.nativeLastError();
            runOnUiThread(() -> {
                String msg = error.isEmpty() ? "게임 실행이 중단되었습니다." : "실행 중단: " + error;
                setPlayerStatus(msg);
                Toast.makeText(this, msg, Toast.LENGTH_LONG).show();
            });
            // Already on the emulator thread; save before the log can be lost.
            autoSaveLogOnStop();
            return;
        }

        short[] frame = NativeBridge.nativeFrame();
        if (frame != null && frame.length > 2 && gameView != null) {
            boolean first = !framePainted;
            framePainted = true;
            // Doubled here, on the emulator thread, so the UI thread only ever
            // copies pixels; the frame as the title drew it goes along too, for
            // the screen to redraw from when the quality changes.
            short[] doubled = screenQuality == QUALITY_HQ2X ? NativeBridge.nativeHq2x(frame) : null;
            runOnUiThread(() -> {
                gameView.setFrame(frame, doubled);
                if (first) {
                    setPlayerStatus(currentGameName);
                }
            });
            return;
        }

        // A running game reaches here on every tick it has no new frame for, so
        // once one has been painted the title bar keeps the game's name.
        if (framePainted) {
            return;
        }

        // Nothing painted yet: surface what tick reported, but only
        // occasionally, so a slow boot does not spam the UI thread.
        if (++statusCounter >= STATUS_TICKS) {
            statusCounter = 0;
            runOnUiThread(() -> setPlayerStatus("게임 초기화: " + status));
        }
    }

    /**
     * Host side of WIPI-C MC_phnCallPlace. The LGT WipiPlayer implementation
     * launches ACTION_CALL with exactly a tel: URI.
     */
    private void placePhoneCall(String number) {
        if (checkSelfPermission(Manifest.permission.CALL_PHONE)
                != PackageManager.PERMISSION_GRANTED) {
            pendingPhoneCall = number;
            requestPermissions(
                    new String[]{Manifest.permission.CALL_PHONE},
                    REQUEST_CALL_PHONE);
            return;
        }

        try {
            startActivity(new Intent(Intent.ACTION_CALL, Uri.parse("tel:" + number)));
        } catch (Exception e) {
            Log.e(TAG, "Phone call failed", e);
            Toast.makeText(
                    this,
                    "통화 요청 실패: " + e.getMessage(),
                    Toast.LENGTH_LONG).show();
        }
    }

    /** Host side of LGT MC_sysExecute("WAPBROWSER", argv). */
    private void openBrowser(String url) {
        try {
            startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
        } catch (Exception e) {
            Log.e(TAG, "Browser launch failed", e);
            Toast.makeText(
                    this,
                    "브라우저 실행 실패: " + e.getMessage(),
                    Toast.LENGTH_LONG).show();
        }
    }

    // --- input -----------------------------------------------------------

    // --- helpers ---------------------------------------------------------

    private Button flatButton(String label) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextSize(11.5f);
        button.setTextColor(LIB_GREEN_DEEP);
        button.setAllCaps(false);
        button.setTypeface(Typeface.DEFAULT_BOLD);
        button.setBackground(roundedRect(LIB_GREEN_SOFT, LIB_GREEN_LINE, 1, 15));
        button.setPadding(dp(10), dp(6), dp(10), dp(6));
        button.setMinHeight(0);
        button.setMinimumHeight(0);
        button.setStateListAnimator(null);
        return button;
    }

    private void applyStatusBarInset(View view) {
        view.setOnApplyWindowInsetsListener((v, insets) -> {
            // Reserve the navigation bar's space on the bottom and the sides - in
            // landscape a device can put the bar on the left or the right edge,
            // and without the side padding the number pad would slide under it.
            //
            // Use the *stable* insets there, not the live system-window ones:
            // sticky immersive hides the navigation bar, which collapses the live
            // bottom and side insets to zero while it is hidden, and padding by
            // that let the keypad drop to the very edge of the screen - under
            // where the bar sits - the moment immersive was turned on. The stable
            // insets are the space the bars occupy when shown and do not come and
            // go as immersive hides them, so the keypad keeps its place.
            //
            // The top stays on the live inset so the screen still runs edge to
            // edge under the hidden status bar, which is the point of immersive.
            v.setPadding(
                    insets.getStableInsetLeft(),
                    insets.getSystemWindowInsetTop(),
                    insets.getStableInsetRight(),
                    insets.getStableInsetBottom());
            return insets;
        });
    }

    private String displayName(File file) {
        String name = file.getName();
        int dot = name.lastIndexOf('.');
        return dot > 0 ? name.substring(0, dot) : name;
    }

    /** Stable per-title tile colour for archives without cover art. */
    private int colorForName(String name) {
        int hash = name.hashCode();
        return Color.rgb(
                Math.abs(hash % 130) + 70,
                Math.abs((hash >> 8) % 130) + 70,
                Math.abs((hash >> 16) % 130) + 70);
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    // --- views -----------------------------------------------------------

    /** Draws the emulated LCD, letterboxed into whatever space it is given. */
    private final class GameView extends View {
        // No FILTER_BITMAP_FLAG for 도트: nearest-neighbour scaling keeps the
        // low-res LCD crisp (sharp pixels) instead of the blur bilinear
        // filtering gives. 기본 and HQ2X filter - see setQuality.
        private final Paint paint = new Paint();
        private Bitmap bitmap;
        /**
         * How many bitmap pixels each of the title's takes: 2 while the screen
         * is drawn from hq2x's doubled frame, else 1.
         */
        private int frameScale = 1;
        /**
         * The last frame as the title drew it, so a change of quality can
         * redraw the screen at once rather than at the title's next paint.
         */
        private short[] source;
        private int quality = QUALITY_DOT;
        /**
         * A direct buffer the frame's pixels are copied into before they reach
         * {@link Bitmap#copyPixelsFromBuffer}.
         *
         * Handing that method the array-backed {@code ShortBuffer.wrap(frame, 2,
         * ...)} directly made it reach the backing {@code short[]} through
         * {@code GetPrimitiveArrayCritical} at the wrap's non-zero offset, and
         * the matching release freed a pointer that offset had moved off its
         * allocation - "invalid address passed to free", a native SIGABRT in
         * {@code ReleasePrimitiveArrayCritical} seen on arm64 / Android 7. A
         * direct buffer is read straight from its own memory, so that JNI path
         * is never taken. Reused across frames and grown only when a larger
         * screen needs it.
         */
        private ByteBuffer pixelBuffer;
        /** In landscape the screen is centered at its own aspect; see onMeasure. */
        boolean landscape;

        GameView(MainActivity activity) {
            super(activity);
            setBackgroundColor(COLOR_SCREEN_BEZEL);
        }

        @Override
        protected void onMeasure(int widthSpec, int heightSpec) {
            // Landscape: fill the height but take only the width the screen's own
            // aspect needs, so the controls on either side stay uncovered.
            if (landscape) {
                int h = MeasureSpec.getSize(heightSpec);
                setMeasuredDimension(Math.min(MeasureSpec.getSize(widthSpec), Math.round(h * aspect())), h);
                return;
            }
            super.onMeasure(widthSpec, heightSpec);
        }

        /**
         * The shape of the screen being shown, wide over tall.
         *
         * A handset panel until a frame says otherwise - a title written to be
         * played sideways sends a landscape one (see {@code wie_backend::present}).
         */
        float aspect() {
            return bitmap != null && bitmap.getHeight() > 0 ? (float) bitmap.getWidth() / bitmap.getHeight() : 240f / 320f;
        }

        /**
         * @param frame   {@code {width, height, RGB565 pixels...}} as the title drew it
         * @param doubled the same through hq2x, or null to show it as it is
         */
        void setFrame(short[] frame, short[] doubled) {
            source = frame;
            show(doubled != null ? doubled : frame, doubled != null ? 2 : 1);
        }

        /** The last frame as the title drew it, or null before the first. */
        short[] sourceFrame() {
            return source;
        }

        /** Draws the screen enlarged `quality`'s way from now on, the last frame at once. */
        void setQuality(int quality) {
            if (this.quality == quality) {
                return;
            }
            this.quality = quality;
            paint.setFilterBitmap(quality != QUALITY_DOT);
            if (source != null) {
                short[] doubled = quality == QUALITY_HQ2X ? NativeBridge.nativeHq2x(source) : null;
                show(doubled != null ? doubled : source, doubled != null ? 2 : 1);
            } else {
                invalidate();
            }
        }

        /** Puts `frame`, `scale` bitmap pixels to the title's one, on the screen. */
        private void show(short[] frame, int scale) {
            int width = frame[0] & 0xFFFF;
            int height = frame[1] & 0xFFFF;

            if (width <= 0 || height <= 0 || frame.length != width * height + 2) {
                return;
            }

            frameScale = scale;
            if (bitmap == null || bitmap.getWidth() != width || bitmap.getHeight() != height) {
                bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.RGB_565);

                // How wide this view wants to be is the new shape, and the
                // keypad's side bands are what that leaves - so both have to be
                // worked out again rather than kept from the shape assumed
                // before any frame had arrived.
                requestLayout();
                if (keypad != null) {
                    keypad.setScreenAspect(aspect());
                }
            }

            // Copy the pixels (frame[2..]) into a direct buffer and hand that to
            // copyPixelsFromBuffer, rather than the array-backed wrap - see the
            // pixelBuffer field for why the wrap crashed on some devices. Native
            // order keeps the RGB565 shorts in the byte layout the bitmap stores.
            int needed = width * height * 2;
            if (pixelBuffer == null || pixelBuffer.capacity() < needed) {
                pixelBuffer = ByteBuffer.allocateDirect(needed).order(ByteOrder.nativeOrder());
            }
            pixelBuffer.clear();
            pixelBuffer.asShortBuffer().put(frame, 2, width * height);
            pixelBuffer.limit(needed);
            bitmap.copyPixelsFromBuffer(pixelBuffer);
            invalidate();
        }

        /** The finger on the screen while touch is on, or -1. Only the first is followed. */
        private int touchPointer = -1;
        /** Where that finger last was, in the frame's pixels. */
        private int touchX;
        private int touchY;

        /**
         * A touch on the screen, handed to the title in the frame's own pixels
         * when touch is on (see {@link #toggleTouch}). Only the first finger is
         * followed - the handsets these titles were made for took one.
         *
         * <p>In landscape the keypad lies over this view and takes every touch
         * first, so a finger there reaches the screen through the keypad
         * instead - see {@link KeypadView#onTouchEvent}.
         */
        @Override
        public boolean onTouchEvent(MotionEvent event) {
            if (!touchOn || bitmap == null) {
                return super.onTouchEvent(event);
            }

            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    // A first finger: any one still followed lost its release.
                    liftTouch();
                    press(event.getPointerId(0), event.getX(0), event.getY(0));
                    return true;
                case MotionEvent.ACTION_MOVE: {
                    int index = touchPointer >= 0 ? event.findPointerIndex(touchPointer) : -1;
                    if (index >= 0) {
                        drag(event.getX(index), event.getY(index));
                    }
                    return true;
                }
                case MotionEvent.ACTION_POINTER_UP: {
                    if (event.getPointerId(event.getActionIndex()) == touchPointer) {
                        liftTouch();
                    }
                    return true;
                }
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_CANCEL:
                    liftTouch();
                    return true;
                default:
                    return true;
            }
        }

        /**
         * A finger coming down at a point on this view. It is followed, and the
         * title hears it pressed, when touch is on, no other finger is already
         * followed and the point is on the frame. Whether it was taken.
         */
        boolean press(int pointerId, float viewX, float viewY) {
            if (!touchOn || bitmap == null || touchPointer >= 0 || !toFrame(viewX, viewY)) {
                return false;
            }
            touchPointer = pointerId;
            NativeBridge.nativePointer(0, touchX, touchY);
            return true;
        }

        /** The followed finger moving to a point on this view; the title hears it only if it changes pixel. */
        void drag(float viewX, float viewY) {
            if (touchPointer < 0 || bitmap == null) {
                return;
            }
            int lastX = touchX;
            int lastY = touchY;
            toFrame(viewX, viewY);
            if (touchX != lastX || touchY != lastY) {
                NativeBridge.nativePointer(2, touchX, touchY);
            }
        }

        /** The id of the finger being followed, or -1. */
        int followedPointer() {
            return touchPointer;
        }

        /** Releases the finger being followed, if there is one. */
        void liftTouch() {
            if (touchPointer >= 0) {
                touchPointer = -1;
                NativeBridge.nativePointer(1, touchX, touchY);
            }
        }

        /**
         * Puts a point on this view into the frame's pixels, through the same
         * fit {@link #onDraw} draws with, clamped to the frame's edge so a
         * finger that slides off it is still on it. Whether the point was on the
         * frame at all.
         */
        private boolean toFrame(float viewX, float viewY) {
            // The title's own pixels, however many of the bitmap's each takes.
            int width = bitmap.getWidth() / frameScale;
            int height = bitmap.getHeight() / frameScale;
            float scale = Math.min((float) getWidth() / width, (float) getHeight() / height);
            float left = (getWidth() - width * scale) / 2f;
            float top = (getHeight() - height * scale) / 2f;
            float x = (viewX - left) / scale;
            float y = (viewY - top) / scale;
            boolean inside = x >= 0 && y >= 0 && x < width && y < height;
            touchX = Math.max(0, Math.min(width - 1, (int) x));
            touchY = Math.max(0, Math.min(height - 1, (int) y));
            return inside;
        }

        @Override
        protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);

            if (bitmap == null) {
                return;
            }

            float scale = Math.min((float) getWidth() / bitmap.getWidth(), (float) getHeight() / bitmap.getHeight());
            float left = (getWidth() - bitmap.getWidth() * scale) / 2f;
            float top = (getHeight() - bitmap.getHeight() * scale) / 2f;

            canvas.drawBitmap(bitmap, null,
                    new RectF(left, top, left + bitmap.getWidth() * scale, top + bitmap.getHeight() * scale), paint);
        }
    }

    /**
     * The whole keypad, drawn and handled as one view.
     *
     * <p>It has to be one view to work at all. A grid of {@link Button}s
     * cannot take two fingers: the first view to accept a touch owns the
     * gesture, so a second finger landing on another button is delivered to
     * the first one and that button never hears about it. Diagonal movement,
     * a direction held while a number is tapped, and SEED 2's "press 0 and #"
     * are all impossible that way.
     *
     * <p>Layout is the width split in half - directions on the left, the
     * number pad on the right - under a function row that keeps the same
     * split: the two soft keys over the pad, save and back over the numbers.
     */
    private final class KeypadView extends View {
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint ink = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint subInk = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint edge = new Paint(Paint.ANTI_ALIAS_FLAG);

        private final List<Key> keys = new ArrayList<>();
        private final SparseArray<Key> underFinger = new SparseArray<>();

        /**
         * Landscape splits the keys into two side columns with the screen in
         * the gap between them; portrait keeps the handset stack below it. It
         * stays one view either way so a finger on each side is still tracked.
         */
        boolean landscape;

        /**
         * The shape of the screen the bands make room for, wide over tall. A
         * handset panel until a frame says otherwise; see
         * {@link GameView#aspect}.
         */
        private float screenAspect = 240f / 320f;

        KeypadView(MainActivity activity) {
            super(activity);
            setBackgroundColor(COLOR_KEYPAD_TRAY);

            ink.setTextAlign(Paint.Align.CENTER);
            ink.setTypeface(Typeface.DEFAULT_BOLD);

            // The letters beside a digit are printed, not moulded: lighter
            // weight and smaller, the way a handset silkscreens them.
            subInk.setTextAlign(Paint.Align.CENTER);
            subInk.setTypeface(Typeface.DEFAULT);

            edge.setStyle(Paint.Style.STROKE);
            edge.setStrokeWidth(Math.max(1f, dp(1) * 0.8f));

            // The function row, left to right, grouped over what each pair
            // belongs with: the soft keys sit over the pad they are used
            // alongside, save and back over the numbers.
            //
            // The soft keys are the ones a handset printed nothing on - what
            // they do is whatever the game draws in the corners of its screen
            // above them. Back is the same key a handset marked C, which games
            // use both for their menu and for stepping back out of it.
            keys.add(new Key("L", CODE_SOFT_L, KEY_SOFT));
            keys.add(new Key("R", CODE_SOFT_R, KEY_SOFT));
            keys.add(new Key("저장", 20, KEY_SAVE));
            keys.add(new Key("뒤로가기", CODE_CLEAR, KEY_CLEAR));

            keys.add(new Key("▲", CODE_UP, KEY_DIRECTION));
            keys.add(new Key("◀", CODE_LEFT, KEY_DIRECTION));
            keys.add(new Key("OK", CODE_OK, KEY_PLAIN));
            keys.add(new Key("▶", CODE_RIGHT, KEY_DIRECTION));
            keys.add(new Key("▼", CODE_DOWN, KEY_DIRECTION));

            // The number pad carries what a Korean handset printed beside each
            // digit, because that is what the key looks like and a player
            // reading the pad recognises it faster than a bare column of
            // digits: the 천지인 vowel strokes on 1-3 and the jamo pairs on
            // 4-0, with the Latin run each key writes beside them.
            //
            // The engraving is a promise about what the key types, so it says
            // exactly what the input method writes and nothing else - a key
            // engraved with a jamo it does not write is the bug this pad had.
            // The 된소리 a third press reaches is left off, the way a handset
            // left it off: ㄱㅋ is also where ㄲ lives. # is the space bar while
            // Korean is being typed and types itself otherwise, so it carries
            // the Korean label alone.
            keys.add(new Key("1", 9, KEY_PLAIN, "ㅣ", "@:/"));
            keys.add(new Key("2", 10, KEY_PLAIN, "ㆍ", "ABC"));
            keys.add(new Key("3", 11, KEY_PLAIN, "ㅡ", "DEF"));
            keys.add(new Key("4", 12, KEY_PLAIN, "ㄱㅋ", "GHI"));
            keys.add(new Key("5", 13, KEY_PLAIN, "ㄴㄹ", "JKL"));
            keys.add(new Key("6", 14, KEY_PLAIN, "ㄷㅌ", "MNO"));
            keys.add(new Key("7", 15, KEY_PLAIN, "ㅂㅍ", "PQRS"));
            keys.add(new Key("8", 16, KEY_PLAIN, "ㅅㅎ", "TUV"));
            keys.add(new Key("9", 17, KEY_PLAIN, "ㅈㅊ", "WXYZ"));
            keys.add(new Key("✱", CODE_STAR, KEY_PLAIN));
            keys.add(new Key("0", 8, KEY_PLAIN, "ㅇㅁ", ".,?!"));
            keys.add(new Key("#", CODE_HASH, KEY_PLAIN, "공백", null));
        }

        @Override
        protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
            if (landscape) {
                layoutLandscape(width, height);
                return;
            }

            float pad = dp(5);
            float gap = dp(4);

            float half = (width - 2 * pad - gap) / 2f;
            float leftX = pad;
            float rightX = pad + half + gap;
            float top = pad;
            float usable = height - 2 * pad;

            float topRow = (usable - gap) * KEYPAD_TOP_ROW;
            float below = usable - gap - topRow;
            float padTop = top + topRow + gap;

            // Two function keys over each half, so the pair lines up with the
            // pad it goes with.
            float functionWidth = (half - gap) / 2f;
            place(0, leftX, top, functionWidth, topRow);
            place(1, leftX + functionWidth + gap, top, functionWidth, topRow);
            place(2, rightX, top, functionWidth, topRow);
            place(3, rightX + functionWidth + gap, top, functionWidth, topRow);

            // A three by three grid with only the plus filled in, so each
            // direction is its own key and two of them can be held at once.
            float cellWidth = (half - 2 * gap) / 3f;
            float cellHeight = (below - 2 * gap) / 3f;

            place(4, leftX + cellWidth + gap, padTop, cellWidth, cellHeight);
            place(5, leftX, padTop + cellHeight + gap, cellWidth, cellHeight);
            place(6, leftX + cellWidth + gap, padTop + cellHeight + gap, cellWidth, cellHeight);
            place(7, leftX + 2 * (cellWidth + gap), padTop + cellHeight + gap, cellWidth, cellHeight);
            place(8, leftX + cellWidth + gap, padTop + 2 * (cellHeight + gap), cellWidth, cellHeight);

            float numberWidth = (half - 2 * gap) / 3f;
            float numberHeight = (below - 3 * gap) / 4f;

            for (int index = 0; index < 12; index++) {
                float x = rightX + (index % 3) * (numberWidth + gap);
                float y = padTop + (index / 3) * (numberHeight + gap);

                place(9 + index, x, y, numberWidth, numberHeight);
            }

            ink.setTextSize(Math.min(numberHeight * 0.42f, dp(22)));
        }

        /**
         * Says what shape the screen between the bands is, so they make room
         * for the width it actually takes.
         *
         * A title written to be played with the handset held sideways sends a
         * landscape frame, which is half again as wide as the portrait panel
         * these bands were first written for - sized for the panel, the bands
         * ended up underneath it.
         */
        void setScreenAspect(float aspect) {
            if (aspect <= 0f || Math.abs(aspect - screenAspect) < 0.001f) {
                return;
            }

            screenAspect = aspect;

            // The view's own size has not changed, so onSizeChanged will not
            // fire; the keys have to be placed again from here.
            if (landscape && getWidth() > 0 && getHeight() > 0) {
                layoutLandscape(getWidth(), getHeight());
                invalidate();
            }
        }

        /**
         * Landscape layout: two compact control clusters, one at each edge,
         * with the screen filling the wide gap between them. Left cluster is
         * the soft keys over the direction pad; right is save and back over
         * the number pad. The keys are capped and centered in their side band
         * rather than stretched to fill it, so the screen stays the prominent
         * thing on the display.
         */
        private void layoutLandscape(int width, int height) {
            float pad = dp(10);
            float gap = dp(5);

            // The screen keeps its own aspect in the middle; each side band is
            // whatever is left over, and the keys sit compactly inside. A
            // landscape screen leaves narrower bands than a portrait one, and
            // the keys shrink to fit rather than the band growing over them.
            float centerW = height * screenAspect;
            // The outer margin at one end of the band and a key's own gap at
            // the other, so the cluster never runs up against the screen.
            float sideW = (width - centerW) / 2f - pad - gap;
            // Below what a key column needs at all, take that much anyway and
            // let the keys sit over the edges of the screen - slivers would be
            // worse than a little overlap.
            if (sideW < dp(84)) {
                sideW = Math.min(dp(84), (width - 2 * pad) / 2f - dp(60));
            }
            float leftX = pad;
            float rightX = width - pad - sideW;

            float usable = height - 2 * pad;

            // A hard cap keeps the keys small; the two size limits keep them
            // inside the band's width and inside its height (the taller, right
            // cluster is one function row over four number rows = 4.8 cells).
            float keyCap = dp(54);
            float cell = Math.min(keyCap, Math.min((sideW - 2 * gap) / 3f, (usable - 4 * gap) / 4.8f));
            float funcH = cell * 0.8f;

            float clusterW = cell * 3 + gap * 2;
            float functionWidth = (clusterW - gap) / 2f;
            float leftClusterX = leftX + (sideW - clusterW) / 2f;
            float rightClusterX = rightX + (sideW - clusterW) / 2f;

            float leftHeight = funcH + 3 * cell + 3 * gap;
            float rightHeight = funcH + 4 * cell + 4 * gap;
            float leftTop = pad + (usable - leftHeight) / 2f;
            float rightTop = pad + (usable - rightHeight) / 2f;

            // LEFT cluster: soft keys over the direction pad.
            place(0, leftClusterX, leftTop, functionWidth, funcH);
            place(1, leftClusterX + functionWidth + gap, leftTop, functionWidth, funcH);

            float dx = leftClusterX;
            float dy = leftTop + funcH + gap;
            place(4, dx + cell + gap, dy, cell, cell);
            place(5, dx, dy + cell + gap, cell, cell);
            place(6, dx + cell + gap, dy + cell + gap, cell, cell);
            place(7, dx + 2 * (cell + gap), dy + cell + gap, cell, cell);
            place(8, dx + cell + gap, dy + 2 * (cell + gap), cell, cell);

            // RIGHT cluster: save and back over the number pad.
            place(2, rightClusterX, rightTop, functionWidth, funcH);
            place(3, rightClusterX + functionWidth + gap, rightTop, functionWidth, funcH);

            float nx = rightClusterX;
            float ny = rightTop + funcH + gap;
            for (int index = 0; index < 12; index++) {
                float x = nx + (index % 3) * (cell + gap);
                float y = ny + (index / 3) * (cell + gap);
                place(9 + index, x, y, cell, cell);
            }

            ink.setTextSize(Math.min(cell * 0.42f, dp(20)));
        }

        private void place(int index, float x, float y, float width, float height) {
            Key key = keys.get(index);
            key.bounds.set(x, y, x + width, y + height);
            key.shade();
            ControlPatch.afterPlace(this, index);
        }

        @Override
        protected void onDraw(Canvas canvas) {
            ControlPatch.beforeDraw(this, canvas);
            float radius = dp(8);
            subInk.setTextSize(ink.getTextSize() * 0.42f);

            for (Key key : keys) {
                if (ControlPatch.hidden(this, key)) {
                    continue;
                }
                if (key.down) {
                    fill.setShader(null);
                    fill.setColor(key.pressedColor());
                } else {
                    fill.setShader(key.shader);
                    fill.setColor(Color.WHITE);
                }
                canvas.drawRoundRect(key.bounds, radius, radius, fill);
                fill.setShader(null);

                edge.setColor(key.borderColor());
                canvas.drawRoundRect(key.bounds, radius, radius, edge);

                ink.setColor(key.textColor());
                float was = ink.getTextSize();

                if (key.jamo == null && key.latin == null) {
                    // A label wider than its key is shrunk to fit rather than
                    // clipped, so a word can be used where a digit was.
                    fit(ink, key.label, key.bounds.width() * 0.82f);
                    canvas.drawText(key.label, key.bounds.centerX(), key.bounds.centerY() + ink.getTextSize() * 0.36f, ink);
                    ink.setTextSize(was);
                    continue;
                }

                // Engraved the way a handset prints it: the digit on the left
                // of the key, the letters stacked in the space to its right.
                float centerY = key.bounds.centerY();
                float digitX = key.bounds.left + key.bounds.width() * 0.30f;
                float letterX = key.bounds.left + key.bounds.width() * 0.71f;
                float letterRoom = key.bounds.width() * 0.48f;

                fit(ink, key.label, key.bounds.width() * 0.30f);
                canvas.drawText(key.label, digitX, centerY + ink.getTextSize() * 0.36f, ink);
                ink.setTextSize(was);

                subInk.setColor(key.subTextColor());
                float subWas = subInk.getTextSize();
                if (key.jamo != null && key.latin != null) {
                    fit(subInk, key.jamo, letterRoom);
                    // Two lines straddling the key's middle, so the pair reads
                    // as one block against the digit rather than sitting low.
                    canvas.drawText(key.jamo, letterX, centerY - subWas * 0.22f, subInk);
                    subInk.setTextSize(subWas);

                    fit(subInk, key.latin, letterRoom);
                    canvas.drawText(key.latin, letterX, centerY + subWas * 0.94f, subInk);
                } else {
                    String only = key.jamo != null ? key.jamo : key.latin;
                    fit(subInk, only, letterRoom);
                    canvas.drawText(only, letterX, centerY + subInk.getTextSize() * 0.36f, subInk);
                }
                subInk.setTextSize(subWas);
            }
            ControlPatch.afterDraw(this, canvas);
        }

        /** Shrinks {@code paint} just enough that {@code text} fits {@code room}. */
        private void fit(Paint paint, String text, float room) {
            float measured = paint.measureText(text);
            if (measured > room && measured > 0f) {
                paint.setTextSize(paint.getTextSize() * room / measured);
            }
        }

        @Override
        public boolean onTouchEvent(MotionEvent event) {
            if (ControlPatch.beforeTouch(this, event)) {
                return true;
            }
            // In landscape this view lies over the game screen (see
            // ControlPatch.onPlayerBuilt), so a finger on the screen and on no
            // key is the screen's: it is handed over in the screen's own
            // coordinates and kept away from the keys while it stays down.
            GameView screen = landscape ? gameView : null;
            if (screen != null && event.getActionMasked() == MotionEvent.ACTION_DOWN) {
                // A first finger: any one still on the screen lost its release.
                screen.liftTouch();
            }
            int onScreen = screen != null ? screen.followedPointer() : -1;
            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                case MotionEvent.ACTION_POINTER_DOWN: {
                    int pointer = event.getActionIndex();
                    float x = event.getX(pointer);
                    float y = event.getY(pointer);
                    Key key = keyAt(x, y);
                    if (key == null && screen != null
                            && screen.press(event.getPointerId(pointer), x - screen.getLeft(), y - screen.getTop())) {
                        return true;
                    }
                    underFinger.put(event.getPointerId(pointer), key);
                    break;
                }
                case MotionEvent.ACTION_MOVE: {
                    for (int pointer = 0; pointer < event.getPointerCount(); pointer++) {
                        if (event.getPointerId(pointer) == onScreen) {
                            screen.drag(event.getX(pointer) - screen.getLeft(), event.getY(pointer) - screen.getTop());
                            continue;
                        }
                        underFinger.put(event.getPointerId(pointer), keyAt(event.getX(pointer), event.getY(pointer)));
                    }
                    break;
                }
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_POINTER_UP: {
                    int id = event.getPointerId(event.getActionIndex());
                    if (id == onScreen) {
                        screen.liftTouch();
                        return true;
                    }
                    underFinger.remove(id);
                    break;
                }
                case MotionEvent.ACTION_CANCEL: {
                    if (screen != null) {
                        screen.liftTouch();
                    }
                    underFinger.clear();
                    break;
                }
                default:
                    return true;
            }

            settle();
            return true;
        }

        private Key keyAt(float x, float y) {
            return (Key) ControlPatch.keyAt(this, x, y);
        }

        /**
         * Forgets every finger and sends the resulting key-ups, for when the
         * player is interrupted mid-press and the real ACTION_UP will never
         * arrive.
         */
        void releaseAll() {
            if (underFinger.size() == 0) {
                return;
            }
            underFinger.clear();
            settle();
        }

        /**
         * Sends the difference between what is held now and what was held
         * before, so a finger sliding off a key releases it and two fingers on
         * one key still press it once.
         */
        private void settle() {
            boolean changed = false;

            for (Key key : keys) {
                boolean held = false;
                for (int index = 0; index < underFinger.size(); index++) {
                    if (underFinger.valueAt(index) == key) {
                        held = true;
                        break;
                    }
                }

                if (held == key.down) {
                    continue;
                }

                key.down = held;
                changed = true;
                Log.d(TAG, (held ? "key down: " : "key up: ") + key.code);
                ControlPatch.touchKey(key.code, held ? 1 : 0);
            }

            if (changed) {
                invalidate();
            }
        }
    }

    /** One key of {@link KeypadView}. */
    private static final class Key {
        final String label;
        /** The jamo printed beside the digit, or null for a key without one. */
        final String jamo;
        /** The Latin letters printed beside the digit, or null. */
        final String latin;
        final int code;
        final int style;
        final RectF bounds = new RectF();
        android.graphics.Shader shader;
        boolean down;

        Key(String label, int code, int style) {
            this(label, code, style, null, null);
        }

        Key(String label, int code, int style, String jamo, String latin) {
            this.label = label;
            this.code = code;
            this.style = style;
            this.jamo = jamo;
            this.latin = latin;
        }

        /** Rebuilds the face gradient for the bounds the key was just given. */
        void shade() {
            shader = new android.graphics.LinearGradient(
                    0, bounds.top, 0, bounds.bottom,
                    topColor(), bottomColor(), android.graphics.Shader.TileMode.CLAMP);
        }

        // One face and one outline for every key - numbers, directions, soft
        // keys, save and back alike - because that is how a handset's pad
        // reads: a single milled surface, gold on dark, with the glyph the only
        // thing that ever differs. The top/bottom pair keeps the barest
        // gradient so a face has some depth without looking glossy.
        private int topColor() {
            return COLOR_KEY_FACE_TOP;
        }

        private int bottomColor() {
            return COLOR_KEY_FACE_BOTTOM;
        }

        int borderColor() {
            return COLOR_KEY_EDGE;
        }

        int pressedColor() {
            return COLOR_KEY_PRESSED;
        }

        // The one place the handset itself breaks the gold: the call key is
        // printed green and the end key red, and those two sit exactly where
        // save and back do here. Only the glyph is tinted - the face and the
        // outline stay the same as every other key, as they do on the phone.
        int textColor() {
            switch (style) {
                case KEY_SAVE:
                    return Color.rgb(122, 196, 126);
                case KEY_CLEAR:
                    return Color.rgb(214, 96, 88);
                default:
                    return COLOR_KEY_INK;
            }
        }

        int subTextColor() {
            return COLOR_KEY_INK_SUB;
        }
    }
}
