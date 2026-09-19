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
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.util.Log;
import android.util.SparseArray;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewOutlineProvider;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import android.view.WindowManager;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.Arrays;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;
import java.util.Collections;
import java.util.Enumeration;
import java.util.Locale;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import java.nio.ShortBuffer;

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
     * A CPU-bound title runs the whole budget every tick, so the run/idle ratio
     * is {@code BUDGET / (BUDGET + INTERVAL)}: the old 20/16 starved the emulator
     * to 56% of real time and, because MIPS is measured over the wall clock, made
     * a title that the JIT can drive at ~70 MIPS look like ~40 and feel slow. A
     * short interval lifts the duty cycle to ~83% without pegging a menu: the
     * native tick now returns the instant the emulator reports idle (every task
     * asleep), so the interval becomes a real sleep whenever there is no work.
     */
    private static final int TICK_BUDGET_MS = 20;
    private static final int TICK_INTERVAL_MS = 4;

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
    private boolean playerVisible;
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

        gamesDir = new File(getFilesDir(), "games");
        if (!gamesDir.exists()) {
            gamesDir.mkdirs();
        }

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

        emulatorThread.scheduleWithFixedDelay(this::emulatorStep, 0, TICK_INTERVAL_MS, TimeUnit.MILLISECONDS);
    }

    @Override
    protected void onResume() {
        super.onResume();
        foreground = true;
        // The AudioTracks are paused, not released, when we leave the
        // foreground, so playback picks up where it left off on return.
        audioOutput.resume();
    }

    @Override
    protected void onPause() {
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
        super.onWindowFocusChanged(hasFocus);
        // A notification shade or dialog can take focus without pausing us,
        // and swallows the touch release the same way. Drop held keys as soon
        // as focus is lost.
        if (!hasFocus) {
            releaseKeypad();
        }
    }

    /** Releases every key the keypad currently holds, if a keypad is shown. */
    private void releaseKeypad() {
        KeypadView view = keypad;
        if (view != null) {
            view.releaseAll();
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
        NativeBridge.nativeKey(code, down ? 1 : 0);
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

    @Override
    protected void onDestroy() {
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
        if (keyMapVisible) {
            // The screen keeps its own working copy, so back is the same
            // question its own arrow asks.
            leaveKeyMap(editingMapping);
            return;
        }

        if (!playerVisible) {
            super.onBackPressed();
            return;
        }

        // In a game, back asks before leaving. The game is frozen while the
        // dialog is up: 예 quits to the library, 아니요 (or dismissing the dialog
        // with back / an outside tap) resumes it in place.
        paused = true;
        new AlertDialog.Builder(this)
                .setTitle("종료")
                .setMessage("애플리케이션을 종료하시겠습니까?")
                .setPositiveButton("예", (dialog, which) -> exitGameToLibrary())
                .setNegativeButton("아니요", (dialog, which) -> paused = false)
                .setOnCancelListener(dialog -> paused = false)
                .show();
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

        // The way into the key mapping, at the end of the name row where there
        // was nothing but space.
        View padEntry = new PadGlyphView(this);
        padEntry.setBackground(roundedRect(LIB_GREEN_SOFT, LIB_GREEN_LINE, 1, 12));
        padEntry.setContentDescription("게임패드 키매핑");
        padEntry.setOnClickListener(v -> showKeyMap());
        nameRow.addView(padEntry, new LinearLayout.LayoutParams(dp(40), dp(40)));

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

        // Section header: title on the left, live count on the right.
        File[] games = gamesDir.listFiles(File::isFile);
        int count = games == null ? 0 : games.length;
        LinearLayout sect = new LinearLayout(this);
        sect.setOrientation(LinearLayout.HORIZONTAL);
        sect.setGravity(android.view.Gravity.CENTER_VERTICAL);
        sect.setPadding(dp(2), dp(16), dp(2), dp(8));
        TextView sectTitle = new TextView(this);
        sectTitle.setText("게임 목록");
        sectTitle.setTextSize(13f);
        sectTitle.setTypeface(Typeface.DEFAULT_BOLD);
        sectTitle.setTextColor(LIB_INK);
        sect.addView(sectTitle, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        TextView sectCount = new TextView(this);
        sectCount.setText(count + "개");
        sectCount.setTextSize(12f);
        sectCount.setTextColor(LIB_MUTED);
        sect.addView(sectCount);
        content.addView(sect);

        // One rounded surface with hairline dividers between rows.
        content.addView(buildGameList(games));

        ScrollView scroll = new ScrollView(this);
        scroll.setVerticalScrollBarEnabled(false);
        scroll.addView(content);
        root.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

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
        title.setPadding(dp(12), 0, 0, 0);
        titleRow.addView(title);
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
            padMapping.copyFrom(working);
            padMapping.save(this);
            Toast.makeText(this, "키매핑을 저장했습니다.", Toast.LENGTH_SHORT).show();
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

        // Cover: the archive's own icon if it carries one, otherwise a colour
        // tile with the title's first character.
        Bitmap bitmap = readArchiveIcon(game);
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

        TextView tag = new TextView(this);
        tag.setText(archiveTag(game));
        tag.setTextSize(10f);
        tag.setTypeface(Typeface.DEFAULT_BOLD);
        tag.setTextColor(LIB_GREEN_DEEP);
        tag.setBackground(roundedRect(LIB_GREEN_SOFT, 0, 0, 6));
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

        // A round, soft-green play affordance on the right.
        TextView play = new TextView(this);
        play.setText("▶");
        play.setTextSize(11f);
        play.setTextColor(LIB_GREEN);
        play.setGravity(android.view.Gravity.CENTER);
        play.setBackground(circle(LIB_GREEN_SOFT));
        row.addView(play, new LinearLayout.LayoutParams(dp(29), dp(29)));

        row.setOnClickListener(v -> showPlayer(game));
        row.setOnLongClickListener(v -> {
            showGameMenu(game);
            return true;
        });

        return row;
    }

    /** Uppercase archive extension used as the row's small tag chip. */
    private String archiveTag(File game) {
        String fileName = game.getName();
        int dot = fileName.lastIndexOf('.');
        String ext = dot >= 0 ? fileName.substring(dot + 1) : "";
        return ext.isEmpty() ? "게임" : ext.toUpperCase(java.util.Locale.ROOT);
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

    /** What a long press offers: get the saves out, or drop the game. */
    private void showGameMenu(File game) {
        new AlertDialog.Builder(this)
                .setTitle(displayName(game))
                .setItems(new CharSequence[]{"세이브 파일 꺼내기", "세이브 불러오기", "목록에서 삭제"}, (dialog, which) -> {
                    if (which == 0) {
                        exportSaves(game);
                    } else if (which == 1) {
                        importSaves(game);
                    } else {
                        confirmDelete(game);
                    }
                })
                .setNegativeButton("취소", null)
                .show();
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

                    Toast.makeText(this, "다운로드 폴더에 저장: " + result.name + " (" + result.files + "개)", Toast.LENGTH_LONG).show();
                });
            } catch (Exception e) {
                runOnUiThread(() -> Toast.makeText(this, "꺼내기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
            }
        });
    }

    /**
     * Restores saved data from a previously exported save zip, overwriting the
     * app's private save folder. The zip's own {@code db/<id>}/{@code fs/<id>}
     * paths route each file to the game it belongs to, so this works from any
     * title's menu; the file can sit in Downloads or any other folder.
     */
    private void importSaves(File game) {
        new AlertDialog.Builder(this)
                .setTitle(displayName(game))
                .setMessage("세이브 파일(.zip)을 골라 지금 저장된 내용에 덮어씁니다.\n덮어쓴 뒤에는 되돌릴 수 없습니다.")
                .setNegativeButton("취소", null)
                .setPositiveButton("파일 선택", (dialog, which) -> openSavePicker())
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
            String header = "wie " + NativeBridge.nativeVersion() + "\n"
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

        try (ZipFile zip = new ZipFile(game)) {
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
        } catch (Exception e) {
            // A corrupt or unreadable archive still gets a placeholder tile.
        }

        return null;
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
        if (name.endsWith(".icon") || name.contains("icon")) {
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

            if (!looksLikeImage(header, headerRead)) {
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
            Bitmap bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.length);
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
        startActivityForResult(intent, PICK_GAME);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);

        if (resultCode != RESULT_OK || data == null) {
            return;
        }
        if (requestCode != PICK_GAME && requestCode != PICK_SAVE) {
            return;
        }

        Uri uri = data.getData();
        if (uri == null) {
            return;
        }

        if (requestCode == PICK_SAVE) {
            Toast.makeText(this, "세이브를 불러오는 중...", Toast.LENGTH_SHORT).show();
            importSaveNow(uri);
            return;
        }

        Toast.makeText(this, "게임을 가져오는 중...", Toast.LENGTH_SHORT).show();
        emulatorThread.execute(() -> importGame(uri));
    }

    private void importGame(Uri uri) {
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

            runOnUiThread(() -> {
                Toast.makeText(this, "가져오기 완료", Toast.LENGTH_SHORT).show();
                showLibrary();
            });
        } catch (Exception e) {
            target.delete();
            runOnUiThread(() -> Toast.makeText(this, "가져오기 실패: " + e.getMessage(), Toast.LENGTH_LONG).show());
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
        framePainted = false;
        // Whichever way the phone is being held: the player opens the way the
        // window already is, not the way the last one was.
        landscapeMode = getResources().getConfiguration().orientation == Configuration.ORIENTATION_LANDSCAPE;
        orientationPinned = false;
        // The player is a dark device again, so restore light status-bar icons.
        setLightStatusBar(false);

        // Persistent views, kept across rotations so the last frame and any
        // held keys survive a re-layout instead of being torn down.
        gameView = new GameView(this);
        keypad = new KeypadView(this);

        // The phone decides, the way it decides for everything else:
        // SCREEN_ORIENTATION_USER follows the sensor while the phone's
        // auto-rotate is on and holds the orientation the user locked while it
        // is off. The title-bar toggle then only has to say what the phone is
        // not already saying - see `toggleOrientation`.
        setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_USER);
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
        gameView.landscape = landscapeMode;
        keypad.landscape = landscapeMode;
        keypad.requestLayout();

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(COLOR_BG);

        root.addView(buildTitleBar(),
                new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(landscapeMode ? 40 : 46)));

        if (landscapeMode) {
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
            arena.addView(gameView, screenParams);
            root.addView(arena, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        } else {
            root.addView(gameView, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, GAME_WEIGHT));
            root.addView(keypad, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, KEYPAD_WEIGHT));
        }

        applyStatusBarInset(root);
        setContentView(root);
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
                    : ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE);
        }

        showRotateState();
    }

    /** The orientation the player is in, asked for as an orientation to keep. */
    private int heldOrientation() {
        return landscapeMode
                ? ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
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

            String message = NativeBridge.nativeStart(
                    buffer.toByteArray(),
                    runtimeDir.getAbsolutePath(),
                    phoneModel);
            running = NativeBridge.nativeRunning() != 0;

            runOnUiThread(() -> playerStatus.setText(running ? "게임 초기화 중..." : message));
        } catch (Exception e) {
            runOnUiThread(() -> playerStatus.setText("실행 실패: " + e.getMessage()));
        }
    }

    /**
     * One scheduled step: advance the emulator, drain audio, publish a frame.
     * Runs on the emulator thread.
     */
    private void emulatorStep() {
        if (!running || !playerVisible || !foreground || paused) {
            return;
        }

        PerformanceTuner.beforeNativeTick();
        tickStartedAt = SystemClock.elapsedRealtime();
        String status = NativeBridge.nativeTick(TICK_BUDGET_MS);
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
            runOnUiThread(() -> playerStatus.setText(error.isEmpty() ? "게임 실행이 중단되었습니다." : "실행 중단: " + error));
            // Already on the emulator thread; save before the log can be lost.
            autoSaveLogOnStop();
            return;
        }

        short[] frame = NativeBridge.nativeFrame();
        if (frame != null && frame.length > 2 && gameView != null) {
            boolean first = !framePainted;
            framePainted = true;
            runOnUiThread(() -> {
                gameView.setFrame(frame);
                if (first) {
                    playerStatus.setText(currentGameName);
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
            runOnUiThread(() -> playerStatus.setText("게임 초기화: " + status));
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
            // Reserve all four system-bar insets, not just top and bottom: in
            // landscape a device can put its navigation bar on the left or the
            // right edge, and without the side padding the number pad would
            // slide under it.
            v.setPadding(
                    insets.getSystemWindowInsetLeft(),
                    insets.getSystemWindowInsetTop(),
                    insets.getSystemWindowInsetRight(),
                    insets.getSystemWindowInsetBottom());
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
        // No FILTER_BITMAP_FLAG: nearest-neighbour scaling keeps the low-res LCD
        // crisp (sharp pixels) instead of the blur bilinear filtering gives.
        private final Paint paint = new Paint();
        private Bitmap bitmap;
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

        /** @param frame {@code {width, height, RGB565 pixels...}} */
        void setFrame(short[] frame) {
            int width = frame[0] & 0xFFFF;
            int height = frame[1] & 0xFFFF;

            if (width <= 0 || height <= 0 || frame.length != width * height + 2) {
                return;
            }

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

            bitmap.copyPixelsFromBuffer(ShortBuffer.wrap(frame, 2, width * height));
            invalidate();
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
        }

        @Override
        protected void onDraw(Canvas canvas) {
            float radius = dp(8);
            subInk.setTextSize(ink.getTextSize() * 0.42f);

            for (Key key : keys) {
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
            switch (event.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                case MotionEvent.ACTION_POINTER_DOWN: {
                    int pointer = event.getActionIndex();
                    underFinger.put(event.getPointerId(pointer), keyAt(event.getX(pointer), event.getY(pointer)));
                    break;
                }
                case MotionEvent.ACTION_MOVE: {
                    for (int pointer = 0; pointer < event.getPointerCount(); pointer++) {
                        underFinger.put(event.getPointerId(pointer), keyAt(event.getX(pointer), event.getY(pointer)));
                    }
                    break;
                }
                case MotionEvent.ACTION_UP:
                case MotionEvent.ACTION_POINTER_UP: {
                    underFinger.remove(event.getPointerId(event.getActionIndex()));
                    break;
                }
                case MotionEvent.ACTION_CANCEL: {
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
            for (Key key : keys) {
                if (key.bounds.contains(x, y)) {
                    return key;
                }
            }
            return null;
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
                NativeBridge.nativeKey(key.code, held ? 1 : 0);
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
