package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.Dialog;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.hardware.input.InputManager;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.Toast;
import com.jjongjjongs.minimobile.ControlEditor;
import com.jjongjjongs.minimobile.ControlInput;
import com.jjongjjongs.minimobile.ControlRapid;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import org.json.JSONObject;

public final class ControlPatch {
    static final String TAG = "MiniControls";
    private static Session activeSession;
    private static final Map<Activity, Session> sessions = new HashMap();
    private static final Handler rapidHandler = new Handler(Looper.getMainLooper());
    static final ControlRapid rapid = new ControlRapid(new ControlInput.Sink() { // from class: com.jjongjjongs.minimobile.ControlPatch.1
        @Override // com.jjongjjongs.minimobile.ControlInput.Sink
        public void key(int i, boolean z) {
            // Route through the activity's own key path (`sendKey`) so a keypad
            // or rapid-fire press wakes the idle-aware runner the way a direct
            // press does, rather than only reaching `NativeBridge.nativeKey`.
            Session session = ControlPatch.activeSession;
            if (session != null && session.a != null) {
                ControlPatch.sendKey(session.a, i, z);
            } else {
                NativeBridge.nativeKey(i, z ? 1 : 0);
            }
        }
    }, new ControlRapid.Scheduler() { // from class: com.jjongjjongs.minimobile.ControlPatch.2
        @Override // com.jjongjjongs.minimobile.ControlRapid.Scheduler
        public boolean allowed() {
            Session session = ControlPatch.activeSession;
            return (session == null || session.leaving || session.blocked() || session.a.isFinishing() || !ControlPatch.flag(session.a, "foreground") || !ControlPatch.flag(session.a, "playerVisible") || !ControlPatch.flag(session.a, "running") || ControlPatch.flag(session.a, "paused")) ? false : true;
        }

        @Override // com.jjongjjongs.minimobile.ControlRapid.Scheduler
        public void cancel(Runnable runnable) {
            ControlPatch.rapidHandler.removeCallbacks(runnable);
        }

        @Override // com.jjongjjongs.minimobile.ControlRapid.Scheduler
        public long now() {
            return SystemClock.uptimeMillis();
        }

        @Override // com.jjongjjongs.minimobile.ControlRapid.Scheduler
        public void post(Runnable runnable, long j) {
            ControlPatch.rapidHandler.postDelayed(runnable, j);
        }
    });
    static final ControlInput input = new ControlInput(rapid);
    static final Map<String, Field> fields = new HashMap();
    static final Map<String, Method> methods = new HashMap();

    static final class Session {
        final Activity a;
        ControlData data;
        final InputManager.InputDeviceListener deviceListener;
        final ControlEditor editor;
        boolean fileBusy;
        final InputManager inputManager;
        boolean leaving;
        boolean ownsPause;
        final SharedPreferences prefs;
        boolean previousPause;
        final ControlDialogs ui;
        final Set<Dialog> dialogs = new HashSet();
        boolean axesNeutral = true;
        final TreeMap<Integer, Integer> mapping = new TreeMap<>();
        final LinkedHashMap<String, TreeMap<Integer, Integer>> padSlots = new LinkedHashMap<>();

        Session(Activity activity) {
            this.a = activity;
            this.prefs = this.a.getSharedPreferences("mini_controls_v1", 0);
            String string = this.prefs.getString("keypad", null);
            try {
                this.data = string == null ? new ControlData() : ControlData.decode(string);
            } catch (Exception e) {
                this.data = new ControlData();
                Log.e(ControlPatch.TAG, "invalid saved layout", e);
                ControlPatch.toast(this.a, "저장된 배치를 읽지 못해 기본 배치를 표시합니다. 백업에서 복원할 수 있습니다.");
            }
            this.editor = new ControlEditor(this);
            this.ui = new ControlDialogs(this);
            readPads();
            this.inputManager = (InputManager) this.a.getSystemService("input");
            this.deviceListener = new InputManager.InputDeviceListener() { // from class: com.jjongjjongs.minimobile.ControlPatch.Session.1
                @Override // android.hardware.input.InputManager.InputDeviceListener
                public void onInputDeviceAdded(int i) {
                }

                @Override // android.hardware.input.InputManager.InputDeviceListener
                public void onInputDeviceChanged(int i) {
                    ControlPatch.input.releasePrefix("pad:" + i + ":");
                }

                @Override // android.hardware.input.InputManager.InputDeviceListener
                public void onInputDeviceRemoved(int i) {
                    ControlPatch.input.releasePrefix("pad:" + i + ":");
                }
            };
            if (this.inputManager != null) {
                this.inputManager.registerInputDeviceListener(this.deviceListener, new Handler(Looper.getMainLooper()));
            }
        }

        static JSONObject mapJson(Map<Integer, Integer> map) throws Exception {
            JSONObject jSONObject = new JSONObject();
            for (Map.Entry<Integer, Integer> entry : map.entrySet()) {
                jSONObject.put(String.valueOf(entry.getKey()), entry.getValue().intValue());
            }
            return jSONObject;
        }

        static TreeMap<Integer, Integer> parseMap(JSONObject jSONObject) throws Exception {
            TreeMap<Integer, Integer> treeMap = new TreeMap<>();
            if (jSONObject.length() > 256) {
                throw new Exception("too many keys");
            }
            Iterator<String> keys = jSONObject.keys();
            while (keys.hasNext()) {
                String next = keys.next();
                int parseInt = Integer.parseInt(next);
                int i = jSONObject.getInt(next);
                if (parseInt < 0 || parseInt > 2048 || i < 0 || i >= 21) {
                    throw new Exception("bad mapping");
                }
                treeMap.put(Integer.valueOf(parseInt), Integer.valueOf(i));
            }
            return treeMap;
        }

        void applyRapid() {
            if (ControlPatch.activeSession == this) {
                ControlPatch.rapid.configure(this.data.rapid.enabled, this.data.rapid.periodMs);
            }
        }

        void axis(String str, int i, boolean z) {
            Integer num = this.mapping.get(Integer.valueOf(i));
            ControlPatch.input.set(str, num == null ? -1 : num.intValue(), z);
        }

        boolean blocked() {
            return this.editor.editing || !this.dialogs.isEmpty() || this.fileBusy;
        }

        TreeMap<Integer, Integer> legacyMap(SharedPreferences sharedPreferences, String str) {
            int[] iArr = (int[]) ControlPads.LEGACY_TARGETS.clone();
            for (int i = 0; i < iArr.length; i++) {
                iArr[i] = sharedPreferences.getInt(String.valueOf(str) + ControlPads.LEGACY_LABELS[i], iArr[i]);
            }
            return ControlPads.fromLegacy(iArr);
        }

        void readPads() {
            try {
                String string = this.prefs.getString("pad_current", null);
                int i = 0;
                if (string != null) {
                    this.mapping.putAll(parseMap(new JSONObject(string)));
                } else {
                    SharedPreferences sharedPreferences = this.a.getSharedPreferences("pad_mapping", 0);
                    this.mapping.putAll(legacyMap(sharedPreferences, "pad."));
                    int min = Math.min(8, sharedPreferences.getInt("preset.count", 0));
                    int i2 = 0;
                    while (i2 < min) {
                        int i3 = i2 + 1;
                        String name = ControlData.name(sharedPreferences.getString("preset." + i2 + ".name", "프리셋 " + i3));
                        if (name.length() == 0) {
                            name = "프리셋 " + i3;
                        }
                        while (this.padSlots.containsKey(name)) {
                            name = String.valueOf(name) + "_";
                        }
                        this.padSlots.put(name, legacyMap(sharedPreferences, "preset." + i2 + ".pad."));
                        i2 = i3;
                    }
                }
                String string2 = this.prefs.getString("pad_slots", null);
                if (string2 != null) {
                    JSONObject jSONObject = new JSONObject(string2);
                    Iterator<String> keys = jSONObject.keys();
                    while (keys.hasNext()) {
                        String next = keys.next();
                        this.padSlots.put(next, parseMap(jSONObject.getJSONObject(next)));
                    }
                }
                if (string != null) {
                    i = this.prefs.getInt("pad_default_revision", 0);
                }
                ControlPads.upgradeDefault(this.mapping, i);
                if (string == null || i < 1) {
                    savePads();
                }
            } catch (Exception e) {
                this.mapping.clear();
                this.mapping.putAll(ControlPads.defaults());
                Log.e(ControlPatch.TAG, "read mapping", e);
            }
        }

        void saveLayout() {
            try {
                this.prefs.edit().putString("keypad", this.data.encode()).apply();
                applyRapid();
            } catch (Exception e) {
                Log.e(ControlPatch.TAG, "save layout", e);
                ControlPatch.toast(this.a, "배치 저장 실패: " + e.getMessage());
            }
        }

        void savePads() {
            ControlPatch.input.releasePrefix("pad:");
            try {
                JSONObject jSONObject = new JSONObject();
                for (Map.Entry<String, TreeMap<Integer, Integer>> entry : this.padSlots.entrySet()) {
                    jSONObject.put(entry.getKey(), mapJson(entry.getValue()));
                }
                this.prefs.edit().putString("pad_current", mapJson(this.mapping).toString()).putString("pad_slots", jSONObject.toString()).putInt("pad_default_revision", 1).apply();
            } catch (Exception e) {
                Log.e(ControlPatch.TAG, "save mapping", e);
                ControlPatch.toast(this.a, "매핑 저장 실패");
            }
        }

        void syncPause() {
            Object field;
            String str;
            if (this.leaving) {
                return;
            }
            if (blocked() && ControlPatch.flag(this.a, "playerVisible")) {
                if (!this.ownsPause) {
                    this.previousPause = ControlPatch.flag(this.a, "paused");
                    this.ownsPause = true;
                }
                ControlPatch.set(this.a, "paused", true);
                ControlPatch.call(this.a, "releaseKeypad");
                ControlPatch.input.releaseAll();
                ControlPatch.rapid.stop();
                field = ControlPatch.field(this.a, "audioOutput");
                if (field == null) {
                    return;
                } else {
                    str = "pause";
                }
            } else {
                if (!this.ownsPause) {
                    return;
                }
                this.ownsPause = false;
                ControlPatch.set(this.a, "paused", Boolean.valueOf(this.previousPause));
                if (this.previousPause || !ControlPatch.flag(this.a, "foreground") || (field = ControlPatch.field(this.a, "audioOutput")) == null) {
                    return;
                } else {
                    str = "resume";
                }
            }
            ControlPatch.call(field, str);
        }
    }

    public static void afterDraw(View view, Canvas canvas) {
        of(view).editor.draw(canvas);
    }

    public static void afterPlace(View view, int i) {
        of(view).editor.afterPlace(view, i);
    }

    public static void beforeDraw(View view, Canvas canvas) {
        of(view).editor.drawGrid(canvas);
    }

    public static boolean beforeTouch(View view, MotionEvent motionEvent) {
        Session of = of(view);
        return of.editor.editing ? of.editor.touch(motionEvent) : of.blocked();
    }

    static Object call(Object obj, String str) {
        try {
            String str2 = String.valueOf(obj.getClass().getName()) + "#" + str;
            Method method = methods.get(str2);
            if (method == null) {
                method = obj.getClass().getDeclaredMethod(str, new Class[0]);
                method.setAccessible(true);
                methods.put(str2, method);
            }
            return method.invoke(obj, new Object[0]);
        } catch (Exception e) {
            throw new IllegalStateException(str, e);
        }
    }

    /** Presses a handset key through the activity's own `sendKey(int, boolean)`. */
    static void sendKey(Object obj, int code, boolean pressed) {
        try {
            String str2 = String.valueOf(obj.getClass().getName()) + "#sendKey(IZ)";
            Method method = methods.get(str2);
            if (method == null) {
                method = obj.getClass().getDeclaredMethod("sendKey", Integer.TYPE, Boolean.TYPE);
                method.setAccessible(true);
                methods.put(str2, method);
            }
            method.invoke(obj, Integer.valueOf(code), Boolean.valueOf(pressed));
        } catch (Exception e) {
            NativeBridge.nativeKey(code, pressed ? 1 : 0);
        }
    }

    public static void decorateTitle(final Activity activity, View view) {
        if (view instanceof LinearLayout) {
            Button button = new Button(activity);
            button.setText("설정");
            button.setTextSize(12.0f);
            button.setPadding(0, 0, 0, 0);
            ControlStyle.playerButton(button);
            button.setMinWidth(0);
            button.setMinimumWidth(0);
            button.setContentDescription("키패드와 게임패드 설정");
            button.setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlPatch.3
                @Override // android.view.View.OnClickListener
                public void onClick(View view2) {
                    ControlPatch.showSettings(activity);
                }
            });
            LinearLayout linearLayout = (LinearLayout) view;
            linearLayout.addView(button, 1, new LinearLayout.LayoutParams(dp(activity, 56.0f), dp(activity, 34.0f)));
            Button button2 = (Button) field(activity, "collectButton");
            Button button3 = (Button) field(activity, "stopButton");
            Button button4 = (Button) field(activity, "rotateButton");
            if (button3 != null && button3.getParent() == linearLayout) {
                linearLayout.removeView(button3);
            }
            if (button2 != null) {
                button2.setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlPatch.4
                    @Override // android.view.View.OnClickListener
                    public void onClick(View view2) {
                        ControlPatch.call(activity, NativeBridge.nativeLogCollecting() != 0 ? "stopLogCollectAndSave" : "startLogCollect");
                    }
                });
            }
            Button[] buttonArr = {button, button2, button4};
            int i = 0;
            while (i < 3) {
                Button button5 = buttonArr[i];
                if (button5 != null) {
                    button5.setTextSize(12.0f);
                    button5.setMinWidth(0);
                    button5.setMinimumWidth(0);
                    button5.setMinHeight(0);
                    button5.setMinimumHeight(0);
                    button5.setPadding(0, 0, 0, 0);
                    LinearLayout.LayoutParams layoutParams = new LinearLayout.LayoutParams(dp(activity, 56.0f), dp(activity, 34.0f));
                    layoutParams.rightMargin = dp(activity, i == 2 ? 10 : 6);
                    button5.setLayoutParams(layoutParams);
                }
                i++;
            }
            syncLogButton(activity);
        }
    }

    static int dp(Context context, float f) {
        return Math.round(f * context.getResources().getDisplayMetrics().density);
    }

    static Object field(Object obj, String str) {
        try {
            String str2 = String.valueOf(obj.getClass().getName()) + "#" + str;
            Field field = fields.get(str2);
            if (field == null) {
                field = obj.getClass().getDeclaredField(str);
                field.setAccessible(true);
                fields.put(str2, field);
            }
            return field.get(obj);
        } catch (Exception e) {
            throw new IllegalStateException(str, e);
        }
    }

    static boolean flag(Object obj, String str) {
        return Boolean.TRUE.equals(field(obj, str));
    }

    public static boolean hidden(View view, Object obj) {
        return of(view).editor.hidden(obj);
    }

    static boolean isPad(InputEvent inputEvent) {
        int source = inputEvent.getSource();
        InputDevice device = inputEvent.getDevice();
        if (device != null) {
            source |= device.getSources();
        }
        return (source & 1025) == 1025 || (source & 16777232) == 16777232;
    }

    public static Object keyAt(View view, float f, float f2) {
        ControlEditor controlEditor = of(view).editor;
        controlEditor.bind(view);
        ControlEditor.KeyRef gameHit = controlEditor.gameHit(f, f2);
        if (gameHit == null) {
            return null;
        }
        return gameHit.object;
    }

    static Session of(Activity activity) {
        Session session = sessions.get(activity);
        if (session != null) {
            return session;
        }
        Session session2 = new Session(activity);
        sessions.put(activity, session2);
        return session2;
    }

    static Session of(View view) {
        return of((Activity) view.getContext());
    }

    public static boolean onActivityResult(Activity activity, int i, int i2, Intent intent) {
        return of(activity).ui.activityResult(i, i2, intent);
    }

    public static boolean onBack(Activity activity) {
        Session session = sessions.get(activity);
        if (session == null || !session.editor.editing) {
            return false;
        }
        session.editor.finish(true);
        return true;
    }

    public static void onDestroy(Activity activity) {
        onLeave(activity);
        Session remove = sessions.remove(activity);
        if (remove == null || remove.inputManager == null) {
            return;
        }
        remove.inputManager.unregisterInputDeviceListener(remove.deviceListener);
    }

    public static void onFocus(Activity activity, boolean z) {
        if (z) {
            return;
        }
        input.releaseAll();
        rapid.stop();
    }

    public static boolean onGamepadKey(Activity activity, KeyEvent keyEvent) {
        if (!isPad(keyEvent)) {
            return false;
        }
        Session of = of(activity);
        if (of.ui.captureKey(keyEvent) || of.blocked()) {
            return true;
        }
        if (!flag(activity, "playerVisible")) {
            return false;
        }
        int action = keyEvent.getAction();
        if (action == 0 && keyEvent.getRepeatCount() != 0) {
            return true;
        }
        if (action == 0 || action == 1) {
            Integer num = of.mapping.get(Integer.valueOf(keyEvent.getKeyCode()));
            input.set("pad:" + keyEvent.getDeviceId() + ":key:" + keyEvent.getKeyCode(), num == null ? -1 : num.intValue(), action == 0);
        }
        return true;
    }

    public static boolean onGamepadMotion(Activity activity, MotionEvent motionEvent) {
        if (!isPad(motionEvent)) {
            return false;
        }
        Session of = of(activity);
        float axisValue = motionEvent.getAxisValue(15);
        float axisValue2 = motionEvent.getAxisValue(16);
        float axisValue3 = motionEvent.getAxisValue(0);
        float axisValue4 = motionEvent.getAxisValue(1);
        float max = Math.max(motionEvent.getAxisValue(17), motionEvent.getAxisValue(23));
        float max2 = Math.max(motionEvent.getAxisValue(18), motionEvent.getAxisValue(22));
        of.axesNeutral = Math.abs(axisValue) < 0.35f && Math.abs(axisValue2) < 0.35f && Math.abs(axisValue3) < 0.35f && Math.abs(axisValue4) < 0.35f && max < 0.35f && max2 < 0.35f;
        if (of.ui.captureMotion(motionEvent, axisValue, axisValue2, max, max2) || of.blocked()) {
            return true;
        }
        if (!flag(activity, "playerVisible")) {
            return false;
        }
        if (motionEvent.getActionMasked() == 3) {
            input.releasePrefix("pad:" + motionEvent.getDeviceId() + ":");
            return true;
        }
        String str = "pad:" + motionEvent.getDeviceId() + ":axis:";
        of.axis(String.valueOf(str) + "hatL", 21, axisValue < -0.5f);
        of.axis(String.valueOf(str) + "hatR", 22, axisValue > 0.5f);
        of.axis(String.valueOf(str) + "hatU", 19, axisValue2 < -0.5f);
        of.axis(String.valueOf(str) + "hatD", 20, axisValue2 > 0.5f);
        input.set(String.valueOf(str) + "stickL", 2, axisValue3 < -0.5f);
        input.set(String.valueOf(str) + "stickR", 3, axisValue3 > 0.5f);
        input.set(String.valueOf(str) + "stickU", 0, axisValue4 < -0.5f);
        input.set(String.valueOf(str) + "stickD", 1, axisValue4 > 0.5f);
        of.axis(String.valueOf(str) + "lt", 104, max > 0.5f);
        of.axis(String.valueOf(str) + "rt", 105, max2 > 0.5f);
        return true;
    }

    public static void onLeave(Activity activity) {
        Session session = sessions.get(activity);
        if (session == null) {
            return;
        }
        session.leaving = true;
        session.editor.finish(true);
        session.fileBusy = false;
        Iterator it = new ArrayList(session.dialogs).iterator();
        while (it.hasNext()) {
            ((Dialog) it.next()).dismiss();
        }
        input.releaseAll();
        rapid.stop();
        if (activeSession == session) {
            activeSession = null;
        }
        session.ownsPause = false;
    }

    public static void onPause(Activity activity) {
        input.releaseAll();
        rapid.stop();
    }

    public static void onPlayerBuilt(Activity activity) {
        Session of = of(activity);
        of.leaving = false;
        input.releaseAll();
        rapid.stop();
        activeSession = of;
        of.applyRapid();
        call(activity, "releaseKeypad");
        of.editor.dragging = false;
        of.editor.pointer = -1;
        View view = (View) field(activity, "keypad");
        if (view != null) {
            of.editor.bind(view);
            if (flag(view, "landscape")) {
                view.setBackgroundColor(0);
                view.bringToFront();
            } else {
                view.setBackgroundColor(((Integer) field(activity, "COLOR_KEYPAD_TRAY")).intValue());
            }
            if (of.editor.editing) {
                of.editor.attachToolbar();
            }
        }
        of.syncPause();
    }

    public static void onResume(Activity activity) {
        Session session = sessions.get(activity);
        if (session != null) {
            session.syncPause();
        }
    }

    static void set(Object obj, String str, Object obj2) {
        field(obj, str);
        try {
            fields.get(String.valueOf(obj.getClass().getName()) + "#" + str).set(obj, obj2);
        } catch (Exception e) {
            throw new IllegalStateException(str, e);
        }
    }

    public static void showPadMapping(Activity activity) {
        of(activity).ui.padMenu();
    }

    public static void showSettings(Activity activity) {
        of(activity).ui.mainMenu();
    }

    public static void syncLogButton(Activity activity) {
        Button button = (Button) field(activity, "collectButton");
        if (button == null) {
            return;
        }
        boolean z = NativeBridge.nativeLogCollecting() != 0;
        button.setText(z ? "종료" : "수집");
        button.setAlpha(1.0f);
        button.setContentDescription(z ? "로그 수집 종료 및 파일 저장" : "로그 수집 시작");
    }

    static void toast(Context context, String str) {
        Toast.makeText(context, str, 0).show();
    }

    public static void touchKey(int i, int i2) {
        input.set("touch:" + i, i, i2 != 0);
    }
}
