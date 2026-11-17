package com.jjongjjongs.minimobile;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.Dialog;
import android.content.DialogInterface;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.text.InputFilter;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import com.jjongjjongs.minimobile.ControlData;
import com.jjongjjongs.minimobile.ControlEditor;
import com.jjongjjongs.minimobile.ControlPatch;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Date;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.SortedMap;
import java.util.TreeMap;

final class ControlDialogs {
    static final int EXPORT_LAYOUT = 6211;
    static final int IMPORT_LAYOUT = 6212;
    static final int PICK_KEYPAD_PHOTO = 6213;

    /** Takes the picture 내 사진's 사진 고르기 brought back, while its dialog is open. */
    java.util.function.Consumer<android.graphics.Bitmap> keypadPhotoPicked;
    boolean captureArmed;
    AlertDialog captureDialog;
    String pendingBackup;
    final ControlPatch.Session s;
    // Two palettes, one active. Dark is the default (in-game menus); the
    // library's gamepad mapping switches to light via palette(true).
    final ControlStyle darkStyle;
    ControlStyle lightStyle;
    ControlStyle style;
    final Handler main = new Handler(Looper.getMainLooper());

    /* renamed from: com.jjongjjongs.minimobile.ControlDialogs$22, reason: invalid class name */
    class AnonymousClass22 implements Runnable {
        private final /* synthetic */ String val$backup;
        private final /* synthetic */ int val$request;
        private final /* synthetic */ Uri val$uri;

        AnonymousClass22(int i, String str, Uri uri) {
            this.val$request = i;
            this.val$backup = str;
            this.val$uri = uri;
        }

        @Override // java.lang.Runnable
        public void run() {
            try {
                if (this.val$request == ControlDialogs.EXPORT_LAYOUT) {
                    String payload = this.val$backup != null
                            ? this.val$backup
                            : ControlDialogs.this.s.data.encode();
                    java.io.OutputStream out = ControlDialogs.this.s.a.getContentResolver()
                            .openOutputStream(this.val$uri, "wt");
                    if (out == null) {
                        throw new Exception("파일을 열 수 없습니다.");
                    }
                    try {
                        out.write(payload.getBytes("UTF-8"));
                        out.flush();
                    } finally {
                        out.close();
                    }
                    ControlDialogs.this.main.post(new Runnable() {
                        @Override
                        public void run() {
                            ControlDialogs.this.s.fileBusy = false;
                            ControlDialogs.this.s.syncPause();
                            ControlPatch.toast(ControlDialogs.this.s.a, "배치를 파일로 저장했습니다.");
                        }
                    });
                } else {
                    java.io.InputStream in = ControlDialogs.this.s.a.getContentResolver()
                            .openInputStream(this.val$uri);
                    if (in == null) {
                        throw new Exception("파일을 열 수 없습니다.");
                    }
                    String text;
                    try {
                        java.io.ByteArrayOutputStream buf = new java.io.ByteArrayOutputStream();
                        byte[] chunk = new byte[8192];
                        while (true) {
                            int n = in.read(chunk);
                            if (n == -1) {
                                break;
                            }
                            if (buf.size() + n > 1048576) {
                                throw new Exception("백업 파일은 1 MB 이하여야 합니다.");
                            }
                            buf.write(chunk, 0, n);
                        }
                        text = new String(buf.toByteArray(), "UTF-8");
                    } finally {
                        in.close();
                    }
                    final ControlData decoded = ControlData.decode(text);
                    ControlDialogs.this.main.post(new Runnable() {
                        @Override
                        public void run() {
                            ControlPatch.input.releaseAll();
                            ControlDialogs.this.s.data.applyActive(decoded);
                            ControlDialogs.this.s.data.slots.clear();
                            ControlDialogs.this.s.data.slots.putAll(decoded.slots);
                            ControlDialogs.this.s.saveLayout();
                            ControlDialogs.this.s.editor.refresh();
                            ControlDialogs.this.s.fileBusy = false;
                            ControlDialogs.this.s.syncPause();
                            ControlPatch.toast(ControlDialogs.this.s.a, "배치를 파일에서 복원했습니다.");
                        }
                    });
                }
            } catch (final Exception e) {
                ControlDialogs.this.main.post(new Runnable() {
                    @Override
                    public void run() {
                        ControlDialogs.this.s.fileBusy = false;
                        ControlDialogs.this.s.syncPause();
                        ControlDialogs.this.error(e.getMessage() == null ? "파일 처리 실패" : e.getMessage());
                    }
                });
            }
        }
    }

    static final class PadDialog extends AlertDialog {
        final Activity activity;

        PadDialog(Activity activity, int themeId) {
            super(activity, themeId);
            this.activity = activity;
        }

        @Override // android.app.Dialog, android.view.Window.Callback
        public boolean dispatchGenericMotionEvent(MotionEvent motionEvent) {
            if (ControlPatch.onGamepadMotion(this.activity, motionEvent)) {
                return true;
            }
            return super.dispatchGenericMotionEvent(motionEvent);
        }

        @Override // android.app.Dialog, android.view.Window.Callback
        public boolean dispatchKeyEvent(KeyEvent keyEvent) {
            if (ControlPatch.onGamepadKey(this.activity, keyEvent)) {
                return true;
            }
            return super.dispatchKeyEvent(keyEvent);
        }
    }

    ControlDialogs(ControlPatch.Session session) {
        this.s = session;
        this.darkStyle = new ControlStyle(this.s.a, false);
        this.lightStyle = new ControlStyle(this.s.a, true);
        this.style = this.darkStyle;
    }

    /** Picks the palette for the dialogs opened next: the list's for the
     * library's gamepad mapping - made afresh, as the list may have turned
     * dark or light since - and dark for everything in-game. */
    void palette(boolean lightPalette) {
        if (lightPalette) {
            this.lightStyle = new ControlStyle(this.s.a, true);
        }
        this.style = lightPalette ? this.lightStyle : this.darkStyle;
    }

    static String physicalName(int i) {
        // A phone's own keys by what is printed on them.
        switch (i) {
            case KeyEvent.KEYCODE_STAR:
                return "✱";
            case KeyEvent.KEYCODE_POUND:
                return "#";
            case KeyEvent.KEYCODE_CALL:
                return "통화";
            case KeyEvent.KEYCODE_SOFT_LEFT:
                return "왼쪽소프트";
            case KeyEvent.KEYCODE_SOFT_RIGHT:
                return "오른쪽소프트";
            case KeyEvent.KEYCODE_MENU:
                return "메뉴";
            case KeyEvent.KEYCODE_DEL:
                return "지우기";
            case KeyEvent.KEYCODE_DPAD_CENTER:
                return "가운데";
            default:
                return KeyEvent.keyCodeToString(i).replace("KEYCODE_BUTTON_", "").replace("KEYCODE_", "");
        }
    }

    boolean activityResult(int i, int i2, Intent intent) {
        if (i == PICK_KEYPAD_PHOTO) {
            keypadPhotoResult(i2, intent);
            return true;
        }
        if (i != EXPORT_LAYOUT && i != IMPORT_LAYOUT) {
            return false;
        }
        if (i2 != -1 || intent == null || intent.getData() == null) {
            this.s.fileBusy = false;
            this.pendingBackup = null;
            this.s.syncPause();
            return true;
        }
        Uri data = intent.getData();
        String str = this.pendingBackup;
        this.pendingBackup = null;
        new Thread(new AnonymousClass22(i, str, data), "Mini layout backup").start();
        return true;
    }

    AlertDialog.Builder builder(String str) {
        return new AlertDialog.Builder(this.style.context, this.style.themeId).setTitle(str);
    }

    /**
     * A phone's own key pressed - or a pad's - while the key table is asking
     * for one. Whether it was taken.
     */
    boolean captureKey(KeyEvent keyEvent) {
        if (this.keyLayout.captureKey < 0) {
            return false;
        }
        if (keyEvent.getAction() == 0 && keyEvent.getRepeatCount() == 0) {
            if (padOnly(keyEvent.getKeyCode())) {
                this.keyLayout.captureNote.setText("패드 버튼은 게임패드 버튼 배치에서 바꿉니다.");
            } else {
                putKey(this.keyLayout, keyEvent.getKeyCode());
            }
        }
        return true;
    }

    /** The left stick and the triggers, while the pad layout is asking for a button. */
    boolean captureMotion(MotionEvent motionEvent, float f, float f2, float f3, float f4) {
        int i;
        if (this.padLayout.captureKey < 0) {
            return false;
        }
        if (!this.captureArmed) {
            if (this.s.axesNeutral) {
                this.captureArmed = true;
            }
            return true;
        }
        if (f < -0.65f) {
            i = KeyEvent.KEYCODE_DPAD_LEFT;
        } else if (f > 0.65f) {
            i = KeyEvent.KEYCODE_DPAD_RIGHT;
        } else if (f2 < -0.65f) {
            i = KeyEvent.KEYCODE_DPAD_UP;
        } else if (f2 > 0.65f) {
            i = KeyEvent.KEYCODE_DPAD_DOWN;
        } else if (f3 > 0.65f) {
            i = KeyEvent.KEYCODE_BUTTON_L2;
        } else if (f4 > 0.65f) {
            i = KeyEvent.KEYCODE_BUTTON_R2;
        } else {
            return true;
        }
        putKey(this.padLayout, i);
        return true;
    }

    LinearLayout column() {
        return this.style.column();
    }

    void editNumbers() {
        DialogInterface.OnClickListener onClickListener = null;
        if (this.s.editor.selected < 0) {
            CharSequence[] charSequenceArr = new String[21];
            for (int i = 0; i < 21; i++) {
                charSequenceArr[i] = ControlData.NAMES[ControlData.ORDER[i]];
            }
            track(builder("수정할 버튼 선택").setItems(charSequenceArr, new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.19
                @Override // android.content.DialogInterface.OnClickListener
                public void onClick(DialogInterface dialogInterface, int i2) {
                    ControlDialogs.this.s.editor.select(ControlData.ORDER[i2]);
                    ControlDialogs.this.editNumbers();
                }
            }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
            return;
        }
        final int i2 = this.s.editor.selected;
        ControlEditor.KeyRef keyRef = this.s.editor.byCode[i2];
        if (keyRef == null) {
            return;
        }
        final float f = this.s.a.getResources().getDisplayMetrics().density;
        LinearLayout column = column();
        final EditText[] editTextArr = new EditText[4];
        column.addView(this.style.hint("적용하면 " + this.s.editor.layout().gridDp + "dp 격자에 자동으로 맞춰집니다."));
        String[] strArr = {"왼쪽 X (dp)", "위쪽 Y (dp)", "너비 (dp)", "높이 (dp)"};
        float[] fArr = {keyRef.bounds.left, keyRef.bounds.top, keyRef.bounds.width(), keyRef.bounds.height()};
        int i3 = 0;
        while (i3 < 4) {
            TextView text = this.style.text(strArr[i3], 13.0f, this.style.MUTED);
            text.setPadding(0, this.style.dp(10.0f), 0, this.style.dp(6.0f));
            column.addView(text);
            editTextArr[i3] = this.style.input();
            editTextArr[i3].setSingleLine(true);
            editTextArr[i3].setInputType(8194);
            editTextArr[i3].setText(String.format(Locale.US, "%.1f", Float.valueOf(fArr[i3] / f)));
            column.addView(editTextArr[i3]);
            i3++;
            onClickListener = null;
        }
        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        final AlertDialog create = builder(String.valueOf(ControlData.NAMES[i2]) + " · 위치와 크기").setView(scrollView).setPositiveButton("적용", onClickListener).setNeutralButton("이 버튼 기본값", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.20
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                ControlDialogs.this.s.editor.layout().rect[i2] = null;
                ControlDialogs.this.s.editor.layout().hidden[i2] = false;
                ControlDialogs.this.s.saveLayout();
                ControlDialogs.this.s.editor.refresh();
            }
        }).setNegativeButton("취소", onClickListener).create();
        track(create);
        create.getButton(-1).setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.21
            @Override // android.view.View.OnClickListener
            public void onClick(View view) {
                float[] fArr2 = new float[4];
                for (int i4 = 0; i4 < 4; i4++) {
                    try {
                        fArr2[i4] = Float.parseFloat(editTextArr[i4].getText().toString()) * f;
                        if (Float.isNaN(fArr2[i4]) || Float.isInfinite(fArr2[i4]) || fArr2[i4] < 0.0f || (i4 >= 2 && fArr2[i4] <= 0.0f)) {
                            throw new Exception();
                        }
                    } catch (Exception e) {
                        editTextArr[i4].setError("올바른 숫자를 입력해 주세요.");
                        return;
                    }
                }
                ControlDialogs.this.s.editor.setRect(i2, fArr2[0], fArr2[1], fArr2[2], fArr2[3]);
                ControlDialogs.this.s.saveLayout();
                create.dismiss();
            }
        });
    }

    void error(String str) {
        track(builder("작업을 완료하지 못했습니다").setMessage(str).setPositiveButton("확인", (DialogInterface.OnClickListener) null).create());
    }

    void gridSettings() {
        final ControlData.Layout layout = this.s.editor.layout();
        final int[] iArr = {layout.gridDp};
        int length = ControlGrid.SPACINGS.length;
        String[] strArr = new String[length];
        int i = 0;
        for (int i2 = 0; i2 < length; i2++) {
            int i3 = ControlGrid.SPACINGS[i2];
            strArr[i2] = String.valueOf(i3) + "dp" + (i3 == 8 ? " · 기본" : "");
            if (i3 == iArr[0]) {
                i = i2;
            }
        }
        track(builder(String.valueOf(this.s.editor.landscape() ? "가로" : "세로") + " · 자동맞춤 격자").setSingleChoiceItems(strArr, i, new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.16
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                iArr[0] = ControlGrid.SPACINGS[i4];
            }
        }).setPositiveButton("적용", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.17
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                if (ControlDialogs.this.s.editor.layout() != layout) {
                    ControlPatch.toast(ControlDialogs.this.s.a, "화면 방향이 바뀌었습니다. 격자 설정을 다시 열어 주세요.");
                    return;
                }
                layout.gridDp = iArr[0];
                ControlDialogs.this.s.saveLayout();
                ControlDialogs.this.s.editor.refresh();
            }
        }).setNeutralButton("모두 맞춤", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.18
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                if (ControlDialogs.this.s.editor.layout() != layout) {
                    ControlPatch.toast(ControlDialogs.this.s.a, "화면 방향이 바뀌었습니다. 격자 설정을 다시 열어 주세요.");
                    return;
                }
                layout.gridDp = iArr[0];
                ControlDialogs.this.s.editor.snapAll();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    void loadSlots(final boolean z) {
        ControlPatch.Session session = this.s;
        final ArrayList arrayList = new ArrayList((z ? session.padSlots : session.data.slots).keySet());
        if (arrayList.isEmpty()) {
            ControlPatch.toast(this.s.a, "저장된 " + (z ? "매핑" : "배치") + "가 없습니다.");
        } else {
            final int[] iArr = new int[1];
            track(builder(z ? "게임패드 프리셋" : "저장된 키패드 배치").setSingleChoiceItems((CharSequence[]) arrayList.toArray(new String[0]), 0, new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.13
                @Override // android.content.DialogInterface.OnClickListener
                public void onClick(DialogInterface dialogInterface, int i) {
                    iArr[0] = i;
                }
            }).setPositiveButton("불러오기", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.14
                @Override // android.content.DialogInterface.OnClickListener
                public void onClick(DialogInterface dialogInterface, int i) {
                    String str = (String) arrayList.get(iArr[0]);
                    ControlPatch.input.releaseAll();
                    if (z) {
                        ControlDialogs.this.s.restore(ControlDialogs.this.s.padSlots.get(str));
                        ControlDialogs.this.s.savePads();
                        ControlDialogs.this.refreshPadRows();
                    } else {
                        ControlDialogs.this.s.data.applyActive(ControlDialogs.this.s.data.slots.get(str));
                        ControlDialogs.this.s.saveLayout();
                        ControlDialogs.this.s.editor.refresh();
                    }
                    ControlPatch.toast(ControlDialogs.this.s.a, "‘" + str + "’ 적용 완료");
                }
            }).setNeutralButton("삭제", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.15
                @Override // android.content.DialogInterface.OnClickListener
                public void onClick(DialogInterface dialogInterface, int i) {
                    final String str = (String) arrayList.get(iArr[0]);
                    ControlDialogs controlDialogs = ControlDialogs.this;
                    AlertDialog.Builder message = ControlDialogs.this.builder("저장 항목 삭제").setMessage("‘" + str + "’을 삭제할까요? 현재 적용된 설정은 유지됩니다.");
                    final boolean z2 = z;
                    controlDialogs.track(message.setPositiveButton("삭제", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.15.1
                        @Override // android.content.DialogInterface.OnClickListener
                        public void onClick(DialogInterface dialogInterface2, int i2) {
                            if (z2) {
                                ControlDialogs.this.s.padSlots.remove(str);
                                ControlDialogs.this.s.savePads();
                            } else {
                                ControlDialogs.this.s.data.slots.remove(str);
                                ControlDialogs.this.s.saveLayout();
                            }
                        }
                    }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
                }
            }).setNegativeButton("닫기", (DialogInterface.OnClickListener) null).create());
        }
    }

    /**
     * 조작 설정, opened in-game from the game menu, the game paused under it.
     *
     * <p>At its top a picture of the keypad as it is now, with what it is
     * named under it. The keypad's look is chosen right here, each choice
     * drawn on that picture and on the keypad at once: the design from its
     * four small pictures (내 사진 then offers its own editor), the
     * arrangement for this title, and 펼친 화면 for a Fold or tablet. The rest
     * - moving and sizing keys, hiding them, 연사, saved layouts and the
     * gamepad - is one list, each row saying what it is for.
     */
    void mainMenu() {
        final MainActivity player = this.s.a instanceof MainActivity ? (MainActivity) this.s.a : null;
        LinearLayout column = column();
        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        final AlertDialog create = builder("조작 설정").setView(scrollView).setPositiveButton("게임으로 돌아가기", (DialogInterface.OnClickListener) null).create();
        column.addView(this.style.hint("설정을 바꾸는 동안 게임은 일시정지돼요."));

        if (player != null) {
            final int[] theme = {player.keypadThemeId()};
            final boolean[] numbers = {player.keypadNumbers()};

            // The keypad as it is, and its name.
            LinearLayout card = new LinearLayout(this.style.context);
            card.setOrientation(LinearLayout.VERTICAL);
            card.setBackground(this.style.rounded(this.style.BG, this.style.LINE, 1, 16));
            card.setPadding(this.style.dp(1), this.style.dp(1), this.style.dp(1), this.style.dp(1));
            final View preview = player.keypadPreview(theme[0], numbers[0]);
            preview.setClipToOutline(true);
            preview.setOutlineProvider(new android.view.ViewOutlineProvider() {
                @Override
                public void getOutline(View view, android.graphics.Outline outline) {
                    outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(), ControlDialogs.this.style.dp(15));
                }
            });
            card.addView(preview, new LinearLayout.LayoutParams(-1, this.style.dp(136)));
            LinearLayout caption = new LinearLayout(this.style.context);
            caption.setPadding(this.style.dp(12), this.style.dp(8), this.style.dp(12), this.style.dp(8));
            caption.addView(this.style.text("지금 키패드", 12f, this.style.MUTED), new LinearLayout.LayoutParams(0, -2, 1f));
            final TextView summary = this.style.text("", 12f, this.style.GREEN);
            summary.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
            caption.addView(summary);
            card.addView(caption);
            column.addView(card);

            // The design, from its four pictures; 내 사진 adds its editor.
            column.addView(settingsLabel("키패드 모양 · 모든 게임"));
            LinearLayout designs = new LinearLayout(this.style.context);
            final List<View> swatches = new ArrayList<>();
            final List<TextView> names = new ArrayList<>();
            final Button photoEdit = this.style.button("내 사진 편집 · 사진, 위치, 입히는 방식  ›", false);
            photoEdit.setOnClickListener(v -> {
                create.dismiss();
                keypadDesign();
            });
            final Runnable refresh = () -> {
                summary.setText(KeypadTheme.NAMES[theme[0]] + " · " + (numbers[0] ? "숫자 크게" : "기본 배치"));
                player.showKeypadPreview(preview, theme[0], numbers[0]);
                for (int index = 0; index < swatches.size(); index++) {
                    boolean on = index == theme[0];
                    swatches.get(index).setForeground(this.style.rounded(0, on ? this.style.GREEN : this.style.LINE, on ? 2 : 1, 12));
                    names.get(index).setTextColor(on ? this.style.GREEN : this.style.INK);
                    names.get(index).setTypeface(on ? android.graphics.Typeface.DEFAULT_BOLD : android.graphics.Typeface.DEFAULT);
                }
                photoEdit.setVisibility(theme[0] == KeypadTheme.PHOTO ? View.VISIBLE : View.GONE);
            };
            for (int id = 0; id < KeypadTheme.NAMES.length; id++) {
                final int which = id;
                LinearLayout option = new LinearLayout(this.style.context);
                option.setOrientation(LinearLayout.VERTICAL);
                option.setGravity(android.view.Gravity.CENTER_HORIZONTAL);
                option.setClickable(true);
                option.setFocusable(true);
                option.setContentDescription("키패드 디자인 " + KeypadTheme.NAMES[which]);
                View picture = player.keypadPreview(which, false);
                picture.setClipToOutline(true);
                picture.setOutlineProvider(new android.view.ViewOutlineProvider() {
                    @Override
                    public void getOutline(View view, android.graphics.Outline outline) {
                        outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(), ControlDialogs.this.style.dp(12));
                    }
                });
                option.addView(picture, new LinearLayout.LayoutParams(-1, this.style.dp(52)));
                TextView name = this.style.text(KeypadTheme.NAMES[which], 12f, this.style.INK);
                name.setGravity(android.view.Gravity.CENTER);
                name.setSingleLine(true);
                name.setPadding(0, this.style.dp(6), 0, 0);
                option.addView(name, new LinearLayout.LayoutParams(-1, -2));
                option.setOnClickListener(v -> {
                    theme[0] = which;
                    player.saveKeypadTheme(which);
                    refresh.run();
                });
                LinearLayout.LayoutParams optionParams = new LinearLayout.LayoutParams(0, -2, 1f);
                if (id > 0) {
                    optionParams.leftMargin = this.style.dp(8);
                }
                designs.addView(option, optionParams);
                swatches.add(picture);
                names.add(name);
            }
            column.addView(designs);
            LinearLayout.LayoutParams editParams = new LinearLayout.LayoutParams(-1, -2);
            editParams.topMargin = this.style.dp(10);
            column.addView(photoEdit, editParams);

            // The arrangement, this title only; 펼친 화면, every title.
            column.addView(segmentRow("배치", "이 게임에만", new String[]{"기본", "숫자 크게"}, numbers[0] ? 1 : 0, index -> {
                numbers[0] = index == 1;
                player.saveKeypadNumbers(numbers[0]);
                refresh.run();
            }));
            column.addView(segmentRow("펼친 화면", "폴드·태블릿에서", new String[]{"가로 배치", "세로 배치"},
                    player.unfoldedSideLayout() ? 0 : 1, index -> player.saveUnfoldedSideLayout(index == 0)));
            refresh.run();
        }

        // Everything else, one list.
        column.addView(settingsLabel("키와 게임패드"));
        LinearLayout list = new LinearLayout(this.style.context);
        list.setOrientation(LinearLayout.VERTICAL);
        list.setBackground(this.style.rounded(this.style.BG, this.style.LINE, 1, 16));
        String pad = connectedPadName();
        Object[][] rows = {
                {LineIcon.Kind.MOVE, "위치·크기 편집", "키를 끌어 옮기고 크기 바꾸기", (Runnable) () -> this.s.editor.start()},
                {LineIcon.Kind.EYE, "버튼 표시·숨김", "안 쓰는 키 감추기", (Runnable) this::visibility},
                {LineIcon.Kind.BOLT, "연사", "누르고 있으면 반복 입력", (Runnable) this::rapidSettings},
                {LineIcon.Kind.SAVE, "배치 저장·백업", "저장 · 불러오기 · 파일 백업 · 초기화", (Runnable) this::layoutStorageMenu},
                {LineIcon.Kind.GAMEPAD, "게임패드 버튼", "없음".equals(pad) ? "연결된 패드 없음" : "연결됨 · " + pad, (Runnable) this::padMenu},
        };
        for (int index = 0; index < rows.length; index++) {
            final Runnable action = (Runnable) rows[index][3];
            View row = settingsRow((LineIcon.Kind) rows[index][0], (String) rows[index][1], (String) rows[index][2], index < rows.length - 1);
            row.setOnClickListener(v -> {
                create.dismiss();
                action.run();
            });
            list.addView(row);
        }
        column.addView(list);
        track(create);
        if (player != null) {
            player.centreOverGameScreen(create);
        }
    }

    /** A small heading over a group of 조작 설정. */
    private TextView settingsLabel(String text) {
        TextView label = this.style.text(text, 12f, this.style.MUTED);
        label.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        label.setPadding(this.style.dp(2), this.style.dp(16), 0, this.style.dp(8));
        return label;
    }

    /**
     * A named choice of two, as a segmented switch: the name and where it
     * applies over the two halves, the chosen one raised in the accent.
     */
    private View segmentRow(String name, String scope, String[] labels, int chosen, final java.util.function.IntConsumer picked) {
        LinearLayout box = new LinearLayout(this.style.context);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(0, this.style.dp(16), 0, 0);
        LinearLayout head = new LinearLayout(this.style.context);
        head.setPadding(this.style.dp(2), 0, this.style.dp(2), this.style.dp(8));
        TextView title = this.style.text(name, 13.5f, this.style.INK);
        title.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        head.addView(title, new LinearLayout.LayoutParams(0, -2, 1f));
        head.addView(this.style.text(scope, 11.5f, this.style.MUTED));
        box.addView(head);

        LinearLayout track = new LinearLayout(this.style.context);
        track.setPadding(this.style.dp(4), this.style.dp(4), this.style.dp(4), this.style.dp(4));
        track.setBackground(this.style.rounded(this.style.SOFT, this.style.SOFT, 0, 12));
        final List<TextView> halves = new ArrayList<>();
        for (int index = 0; index < labels.length; index++) {
            final int which = index;
            TextView half = this.style.text(labels[index], 13f, this.style.MUTED);
            half.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
            half.setGravity(android.view.Gravity.CENTER);
            half.setClickable(true);
            half.setFocusable(true);
            half.setOnClickListener(v -> {
                markSegment(halves, which);
                picked.accept(which);
            });
            LinearLayout.LayoutParams halfParams = new LinearLayout.LayoutParams(0, this.style.dp(40), 1f);
            if (index > 0) {
                halfParams.leftMargin = this.style.dp(4);
            }
            track.addView(half, halfParams);
            halves.add(half);
        }
        markSegment(halves, chosen);
        box.addView(track);
        return box;
    }

    private void markSegment(List<TextView> halves, int chosen) {
        for (int index = 0; index < halves.size(); index++) {
            boolean on = index == chosen;
            halves.get(index).setBackground(on ? this.style.rounded(this.style.BG, this.style.BG, 0, 9) : null);
            halves.get(index).setTextColor(on ? this.style.GREEN : this.style.MUTED);
        }
    }

    /** A row of 조작 설정's list: its icon, its name over what it is for, and a chevron. */
    private View settingsRow(LineIcon.Kind icon, String name, String detail, boolean ruled) {
        LinearLayout row = new LinearLayout(this.style.context);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(this.style.dp(14), this.style.dp(9), this.style.dp(14), this.style.dp(9));
        row.setMinimumHeight(this.style.dp(54));
        row.setClickable(true);
        row.setFocusable(true);
        if (ruled) {
            android.graphics.drawable.GradientDrawable rule = new android.graphics.drawable.GradientDrawable();
            rule.setColor(this.style.SOFT);
            android.graphics.drawable.InsetDrawable line = new android.graphics.drawable.InsetDrawable(rule, 0, 0, 0, 0);
            android.graphics.drawable.LayerDrawable under = new android.graphics.drawable.LayerDrawable(new android.graphics.drawable.Drawable[]{line});
            under.setLayerInset(0, this.style.dp(48), 0, 0, 0);
            under.setLayerGravity(0, android.view.Gravity.BOTTOM);
            under.setLayerHeight(0, Math.max(1, this.style.dp(1)));
            row.setBackground(under);
        }
        android.widget.ImageView glyph = new android.widget.ImageView(this.style.context);
        glyph.setImageDrawable(new LineIcon(icon, this.style.MUTED, 1.8f));
        LinearLayout.LayoutParams glyphParams = new LinearLayout.LayoutParams(this.style.dp(20), this.style.dp(20));
        glyphParams.rightMargin = this.style.dp(14);
        row.addView(glyph, glyphParams);
        LinearLayout words = new LinearLayout(this.style.context);
        words.setOrientation(LinearLayout.VERTICAL);
        words.addView(this.style.text(name, 14f, this.style.INK));
        TextView under = this.style.text(detail, 11.5f, this.style.MUTED);
        under.setSingleLine(true);
        under.setEllipsize(android.text.TextUtils.TruncateAt.END);
        words.addView(under);
        row.addView(words, new LinearLayout.LayoutParams(0, -2, 1f));
        row.addView(this.style.text("›", 18f, this.style.MUTED));
        return row;
    }


    /**
     * 배치 저장·백업·초기화: the layout saved on this device and loaded or
     * deleted, backed up to a file and restored from one, or put back to the
     * standard - one row of 조작 설정 opening onto all five.
     */
    void layoutStorageMenu() {
        final String[] titles = {"현재 배치 저장", "저장된 배치 불러오기·삭제", "배치 파일로 백업", "배치 파일에서 복원", "현재 배치 초기화"};
        final String[] icons = {"💾", "📂", "📤", "📥", "↺"};
        int saved = this.s.data.slots.size();
        final String[] details = {"이름을 붙여 저장해 둡니다", saved == 0 ? "저장한 배치 없음" : "저장한 배치 " + saved + "개", "다른 기기로 옮길 때", null, "처음 배치로 되돌립니다"};
        String current = this.s.editor.landscape() ? "가로 화면" : this.s.editor.numbers() ? "세로 · 숫자 크게" : "세로 · 기본";

        LinearLayout column = column();
        column.addView(this.style.hint("지금 배치(" + current + ")를 저장하거나 되돌립니다."));
        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        final AlertDialog create = builder("배치 저장·백업·초기화").setView(scrollView)
                .setNegativeButton("뒤로", new DialogInterface.OnClickListener() {
                    @Override
                    public void onClick(DialogInterface dialogInterface, int i) {
                        ControlDialogs.this.mainMenu();
                    }
                })
                .setPositiveButton("게임으로 돌아가기", (DialogInterface.OnClickListener) null).create();
        LinearLayout section = null;
        for (int i = 0; i < titles.length; i++) {
            final int idx = i;
            if (idx == 0) {
                section = this.style.section(column, "이 기기에");
            } else if (idx == 2) {
                section = this.style.section(column, "파일로");
            } else if (idx == 4) {
                section = this.style.section(column, "되돌리기");
            }
            this.style.menuRow(section, icons[idx], titles[idx], details[idx], new View.OnClickListener() {
                @Override
                public void onClick(View view) {
                    switch (idx) {
                        case 0:
                            ControlDialogs.this.saveName(false);
                            break;
                        case 1:
                            ControlDialogs.this.loadSlots(false);
                            break;
                        case 2:
                            ControlDialogs.this.pickFile(true);
                            break;
                        case 3:
                            ControlDialogs.this.pickFile(false);
                            break;
                        case 4:
                            ControlDialogs.this.resetLayout();
                            break;
                    }
                    create.dismiss();
                }
            });
        }
        track(create);
    }

    /**
     * 키패드 디자인: the designs as small pictures of the keypad itself.
     * Picking one shows it on the real keypad behind the dialog at once;
     * 적용 keeps it for every title, and anything else puts the old one back.
     * 내 사진 opens its own settings under the list - for upright and on its
     * side apart: the pictures, how they are placed and laid on - each shown
     * at once too.
     */
    void keypadDesign() {
        if (!(this.s.a instanceof MainActivity)) {
            return;
        }
        final MainActivity player = (MainActivity) this.s.a;
        final int saved = player.keypadThemeId();
        final int[] chosen = {saved};
        final boolean[] applied = {false};
        final KeypadPhoto keptPhoto = player.keypadPhoto();
        final KeypadPhoto photo = keptPhoto.copy();
        player.showKeypadPhoto(photo);

        LinearLayout column = column();
        column.addView(this.style.hint("모든 게임에 함께 적용됩니다. 키 위치·크기는 그대로예요."));
        final List<LinearLayout> rows = new ArrayList<>();
        final List<View> pictures = new ArrayList<>();
        final View[] photoPanel = new View[1];
        for (int id = 0; id < KeypadTheme.NAMES.length; id++) {
            final int theme = id;
            View picture = player.keypadPreview(theme, false);
            pictures.add(picture);
            LinearLayout row = optionRow(picture, 132, 84, KeypadTheme.NAMES[theme], KeypadTheme.DESCRIPTIONS[theme]);
            row.setOnClickListener(v -> {
                chosen[0] = theme;
                markOption(rows, theme);
                player.showKeypadTheme(theme);
                photoPanel[0].setVisibility(theme == KeypadTheme.PHOTO ? View.VISIBLE : View.GONE);
            });
            rows.add(row);
            column.addView(row);
        }
        markOption(rows, saved);

        final Runnable[] follow = new Runnable[1];
        photoPanel[0] = keypadPhotoPanel(player, photo, pictures, follow);
        photoPanel[0].setVisibility(saved == KeypadTheme.PHOTO ? View.VISIBLE : View.GONE);
        column.addView(photoPanel[0]);

        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        AlertDialog dialog = builder("키패드 디자인").setView(scrollView)
                .setPositiveButton("적용", (d, which) -> {
                    applied[0] = true;
                    player.saveKeypadTheme(chosen[0]);
                    if (chosen[0] == KeypadTheme.PHOTO) {
                        player.saveKeypadPhoto(photo);
                        ControlPatch.toast(this.s.a, photo.face(player.keypadLandscape()).hasPicture()
                                ? "키패드 디자인: 내 사진"
                                : "이 화면 방향은 사진을 고르기 전까지 다크 글래스로 보여요");
                    } else {
                        KeypadPhoto.discardPending(this.s.a);
                        player.showKeypadPhoto(keptPhoto);
                        ControlPatch.toast(this.s.a, "키패드 디자인: " + KeypadTheme.NAMES[chosen[0]]);
                    }
                })
                .setNegativeButton("취소", null)
                .setNeutralButton("꾹 눌러 보기", null).create();
        // A turn, by the tabs or by hand, lays the player out again; the
        // dialog follows it over the screen, and the tabs follow the turn.
        final Runnable refit = () -> {
            player.fitOverGameScreen(dialog, true);
            follow[0].run();
        };
        player.whenRelaidOut(refit);
        track(dialog, () -> {
            player.stopWhenRelaidOut(refit);
            this.keypadPhotoPicked = null;
            if (!applied[0]) {
                KeypadPhoto.discardPending(this.s.a);
                player.showKeypadTheme(saved);
                player.showKeypadPhoto(keptPhoto);
            }
        });
        player.fitOverGameScreen(dialog, true);

        // Held down, the dialog goes clear and the whole keypad shows as the
        // changes have left it; the touch is taken, so it never closes it.
        Button peek = dialog.getButton(AlertDialog.BUTTON_NEUTRAL);
        if (peek != null && dialog.getWindow() != null) {
            final View window = dialog.getWindow().getDecorView();
            peek.setOnTouchListener((v, event) -> {
                int action = event.getActionMasked();
                if (action == android.view.MotionEvent.ACTION_DOWN) {
                    window.setAlpha(0f);
                } else if (action == android.view.MotionEvent.ACTION_UP || action == android.view.MotionEvent.ACTION_CANCEL) {
                    window.setAlpha(1f);
                }
                return true;
            });
        }
    }

    /**
     * 내 사진's settings, under the designs. Upright and on its side each have
     * their own, picked by the tabs at the top: one picture across the keypad
     * or one for each half, each picture chosen, turned, placed and zoomed on
     * its own, and the way they are laid on with four sliders. Every change is
     * drawn at once on the keypad behind the dialog and on the design
     * pictures; the way up the phone is not held in is seen when it is.
     */
    private View keypadPhotoPanel(final MainActivity player, final KeypadPhoto photo, final List<View> pictures, final Runnable[] follow) {
        final LinearLayout panel = new LinearLayout(this.style.context);
        panel.setOrientation(LinearLayout.VERTICAL);
        panel.setPadding(this.style.dp(12), this.style.dp(10), this.style.dp(12), this.style.dp(8));
        panel.setBackground(this.style.rounded(this.style.SOFT, this.style.GREEN, 1, 14));
        LinearLayout.LayoutParams panelParams = new LinearLayout.LayoutParams(-1, -2);
        panelParams.topMargin = this.style.dp(10);
        panel.setLayoutParams(panelParams);

        final boolean[] side = {player.keypadLandscape()};
        final int[] slot = {photo.face(side[0]).split ? KeypadPhoto.LEFT : KeypadPhoto.WHOLE};
        final LinearLayout body = new LinearLayout(this.style.context);
        body.setOrientation(LinearLayout.VERTICAL);
        final Runnable[] render = new Runnable[1];

        final Runnable redraw = () -> {
            player.showKeypadPhoto(photo);
            for (View picture : pictures) {
                picture.invalidate();
            }
        };

        panel.addView(body);

        // The tabs turn the player too, so the face being set is the one on
        // show; the turn lays it out again and follow catches the tabs up.
        follow[0] = () -> {
            boolean now = player.keypadLandscape();
            if (now != side[0]) {
                side[0] = now;
                slot[0] = photo.face(now).split ? KeypadPhoto.LEFT : KeypadPhoto.WHOLE;
            }
            render[0].run();
        };

        render[0] = () -> {
            body.removeAllViews();
            final KeypadPhoto.Face face = photo.face(side[0]);
            final KeypadPhoto.Slot picked = face.slots[slot[0]];

            body.addView(chipRow(new String[]{"세로 화면", "가로 화면"}, side[0] ? 1 : 0, index -> {
                side[0] = index == 1;
                slot[0] = photo.face(side[0]).split ? KeypadPhoto.LEFT : KeypadPhoto.WHOLE;
                player.turnPlayer(side[0]);
                render[0].run();
            }));

            if (side[0] != player.keypadLandscape()) {
                TextView note = this.style.text(side[0] ? "가로 화면 설정은 폰을 눕혀 게임할 때 보여요." : "세로 화면 설정은 폰을 세워 게임할 때 보여요.",
                        11.5f, this.style.MUTED);
                note.setPadding(0, this.style.dp(6), 0, 0);
                body.addView(note);
            }

            body.addView(panelLabel(side[0] ? "사진 (게임 화면 양옆에 깔려요)" : "사진"));
            body.addView(chipRow(new String[]{"한 장", "왼쪽·오른쪽 따로"}, face.split ? 1 : 0, index -> {
                face.split = index == 1;
                face.changed();
                slot[0] = face.split ? KeypadPhoto.LEFT : KeypadPhoto.WHOLE;
                redraw.run();
                render[0].run();
            }));
            if (face.split) {
                body.addView(chipRow(new String[]{"왼쪽 (방향키 쪽)", "오른쪽 (숫자키 쪽)"}, slot[0] == KeypadPhoto.RIGHT ? 1 : 0, index -> {
                    slot[0] = index == 1 ? KeypadPhoto.RIGHT : KeypadPhoto.LEFT;
                    render[0].run();
                }));
            }

            LinearLayout actions = new LinearLayout(this.style.context);
            actions.setGravity(android.view.Gravity.CENTER_VERTICAL);
            actions.setPadding(0, this.style.dp(8), 0, 0);
            Button pick = this.style.button(picked.picture == null ? "사진 고르기" : "사진 바꾸기", true);
            final boolean pickSide = side[0];
            final int pickSlot = slot[0];
            pick.setOnClickListener(v -> {
                this.keypadPhotoPicked = bitmap -> {
                    KeypadPhoto.Face target = photo.face(pickSide);
                    target.slots[pickSlot].setPicture(bitmap);
                    target.changed();
                    redraw.run();
                    render[0].run();
                };
                pickKeypadPhoto(pickSide, pickSlot);
            });
            actions.addView(pick, new LinearLayout.LayoutParams(0, -2, 1.4f));
            if (picked.picture != null) {
                Button turn = this.style.button("↻ 회전", false);
                turn.setOnClickListener(v -> {
                    picked.turns = (picked.turns + 1) % 4;
                    face.changed();
                    redraw.run();
                });
                LinearLayout.LayoutParams turnParams = new LinearLayout.LayoutParams(0, -2, 1f);
                turnParams.leftMargin = this.style.dp(6);
                actions.addView(turn, turnParams);
                Button remove = this.style.button("빼기", false);
                this.style.quiet(remove);
                remove.setOnClickListener(v -> {
                    picked.clear();
                    face.changed();
                    redraw.run();
                    render[0].run();
                });
                LinearLayout.LayoutParams removeParams = new LinearLayout.LayoutParams(0, -2, 0.8f);
                removeParams.leftMargin = this.style.dp(6);
                actions.addView(remove, removeParams);
            }
            body.addView(actions);

            if (picked.picture != null) {
                body.addView(photoSlider("가로 위치", 100, picked.focusX, v -> v <= 10 ? "왼쪽" : v >= 90 ? "오른쪽" : v + "%", v -> {
                    picked.focusX = v;
                    face.changed();
                    redraw.run();
                }));
                body.addView(photoSlider("세로 위치", 100, picked.focusY, v -> v <= 10 ? "위" : v >= 90 ? "아래" : v + "%", v -> {
                    picked.focusY = v;
                    face.changed();
                    redraw.run();
                }));
                body.addView(photoSlider("확대", 200, picked.zoom - 100, v -> (100 + v) + "%", v -> {
                    picked.zoom = 100 + v;
                    face.changed();
                    redraw.run();
                }));
            } else {
                TextView none = this.style.text(face.split ? "이쪽에 사진이 없으면 디자인 바탕이 그대로 보여요." : "사진을 고르기 전까지는 다크 글래스로 보여요.",
                        11.5f, this.style.MUTED);
                none.setPadding(0, this.style.dp(6), 0, 0);
                body.addView(none);
            }

            body.addView(panelLabel("입히는 방식"));
            body.addView(chipRow(KeypadPhoto.MODES, face.mode, index -> {
                face.mode = index;
                redraw.run();
            }));
            body.addView(photoSlider("어둡게", 80, face.dim, v -> v + "%", v -> {
                face.dim = v;
                redraw.run();
            }));
            body.addView(photoSlider("흐림", 20, face.blur, String::valueOf, v -> {
                face.blur = v;
                redraw.run();
            }));
            body.addView(photoSlider("키 투명도 (유리 키·색 뽑기)", 100, face.glass, v -> v + "%", v -> {
                face.glass = v;
                redraw.run();
            }));
        };
        render[0].run();
        return panel;
    }

    private TextView panelLabel(String text) {
        TextView label = this.style.text(text, 12f, this.style.MUTED);
        label.setPadding(0, this.style.dp(12), 0, this.style.dp(4));
        return label;
    }

    /** Choices as chips two to a row, the chosen one outlined; {@code picked} hears a tap. */
    private View chipRow(String[] labels, int chosen, final java.util.function.IntConsumer picked) {
        LinearLayout box = new LinearLayout(this.style.context);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(0, this.style.dp(4), 0, 0);
        final List<TextView> chips = new ArrayList<>();
        LinearLayout row = null;
        for (int index = 0; index < labels.length; index++) {
            if (index % 2 == 0) {
                row = new LinearLayout(this.style.context);
                box.addView(row, new LinearLayout.LayoutParams(-1, -2));
            }
            final int which = index;
            TextView chip = this.style.text(labels[index], 12.5f, this.style.INK);
            chip.setGravity(android.view.Gravity.CENTER);
            chip.setPadding(this.style.dp(8), this.style.dp(9), this.style.dp(8), this.style.dp(9));
            chip.setOnClickListener(v -> {
                markChips(chips, which);
                picked.accept(which);
            });
            LinearLayout.LayoutParams chipParams = new LinearLayout.LayoutParams(0, -2, 1f);
            chipParams.setMargins(index % 2 == 0 ? 0 : this.style.dp(3), this.style.dp(3), index % 2 == 0 ? this.style.dp(3) : 0, this.style.dp(3));
            row.addView(chip, chipParams);
            chips.add(chip);
        }
        markChips(chips, chosen);
        return box;
    }

    private void markChips(List<TextView> chips, int chosen) {
        for (int index = 0; index < chips.size(); index++) {
            boolean on = index == chosen;
            chips.get(index).setBackground(this.style.rounded(on ? this.style.BG : this.style.SOFT, on ? this.style.GREEN : this.style.LINE, on ? 2 : 1, 10));
            chips.get(index).setTextColor(on ? this.style.GREEN : this.style.INK);
        }
    }

    /** A named slider from 0 to {@code max}, its value shown beside the name as it moves. */
    private View photoSlider(String name, int max, int value, final java.util.function.IntFunction<String> shown,
            final java.util.function.IntConsumer changed) {
        LinearLayout box = new LinearLayout(this.style.context);
        box.setOrientation(LinearLayout.VERTICAL);
        LinearLayout labelRow = new LinearLayout(this.style.context);
        labelRow.setPadding(0, this.style.dp(10), 0, 0);
        labelRow.addView(this.style.text(name, 12f, this.style.MUTED), new LinearLayout.LayoutParams(0, -2, 1f));
        final TextView amount = this.style.text(shown.apply(value), 12.5f, this.style.DEEP);
        amount.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        labelRow.addView(amount);
        box.addView(labelRow);

        android.widget.SeekBar bar = new android.widget.SeekBar(this.style.context);
        bar.setMax(max);
        bar.setProgress(value);
        bar.setOnSeekBarChangeListener(new android.widget.SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(android.widget.SeekBar seekBar, int progress, boolean fromUser) {
                amount.setText(shown.apply(progress));
                changed.accept(progress);
            }

            @Override
            public void onStartTrackingTouch(android.widget.SeekBar seekBar) {
            }

            @Override
            public void onStopTrackingTouch(android.widget.SeekBar seekBar) {
            }
        });
        box.addView(bar, new LinearLayout.LayoutParams(-1, -2));
        return box;
    }

    /** Which face and picture 사진 고르기 is choosing for. */
    private boolean keypadPhotoSide;
    private int keypadPhotoSlot;

    /** Opens the system's picture picker for 내 사진; {@link #keypadPhotoResult} takes what it brings back. */
    private void pickKeypadPhoto(boolean onItsSide, int slot) {
        this.keypadPhotoSide = onItsSide;
        this.keypadPhotoSlot = slot;
        try {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("image/*");
            this.s.fileBusy = true;
            this.s.syncPause();
            this.s.a.startActivityForResult(intent, PICK_KEYPAD_PHOTO);
        } catch (Exception e) {
            error("사진 선택기를 열지 못했습니다: " + e.getMessage());
            this.s.fileBusy = false;
            this.s.syncPause();
        }
    }

    private void keypadPhotoResult(int result, Intent intent) {
        this.s.fileBusy = false;
        this.s.syncPause();
        final java.util.function.Consumer<android.graphics.Bitmap> picked = this.keypadPhotoPicked;
        if (result != -1 || intent == null || intent.getData() == null || picked == null) {
            return;
        }
        final Uri uri = intent.getData();
        final Activity activity = this.s.a;
        final boolean onItsSide = this.keypadPhotoSide;
        final int slot = this.keypadPhotoSlot;
        new Thread(() -> {
            try {
                final android.graphics.Bitmap bitmap = KeypadPhoto.importPicture(activity, uri, onItsSide, slot);
                activity.runOnUiThread(() -> {
                    // The dialog may have closed while the picture was read.
                    if (this.keypadPhotoPicked == picked) {
                        picked.accept(bitmap);
                    }
                });
            } catch (Exception e) {
                activity.runOnUiThread(() -> error("사진을 읽지 못했습니다: " + e.getMessage()));
            }
        }, "Mini keypad photo").start();
    }

    /**
     * 키패드 배치: the ordinary keypad or 숫자 크게, which gives the number pad
     * the room - for rhythm games, played on the numbers. Kept per title, so
     * it can be on for one game and off for the rest.
     */
    void keypadArrangement() {
        if (!(this.s.a instanceof MainActivity)) {
            return;
        }
        final MainActivity player = (MainActivity) this.s.a;
        final boolean saved = player.keypadNumbers();
        final boolean[] chosen = {saved};
        int theme = player.keypadThemeId();

        LinearLayout column = column();
        column.addView(this.style.hint("리듬게임처럼 숫자키를 많이 쓰는 게임엔 ‘숫자 크게’가 편해요. 이 게임에만 적용되고, 가로 화면에서는 기본 배치로 보여요."));
        final List<LinearLayout> rows = new ArrayList<>();
        String[] names = {"기본", "숫자 크게"};
        String[] descriptions = {"방향키와 숫자판을 반씩. 대부분의 게임에 맞아요.", "숫자판을 가운데 크게. 나머지 키는 위에 한 줄로."};
        for (int option = 0; option < 2; option++) {
            final int index = option;
            LinearLayout row = optionRow(player.keypadPreview(theme, option == 1), 96, 104, names[option], descriptions[option]);
            row.setOnClickListener(v -> {
                chosen[0] = index == 1;
                markOption(rows, index);
            });
            rows.add(row);
            column.addView(row);
        }
        markOption(rows, saved ? 1 : 0);

        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        track(builder("키패드 배치").setView(scrollView)
                .setPositiveButton("적용", (d, which) -> {
                    if (chosen[0] != saved) {
                        player.saveKeypadNumbers(chosen[0]);
                        ControlPatch.toast(this.s.a, "키패드 배치: " + (chosen[0] ? "숫자 크게" : "기본"));
                    }
                })
                .setNegativeButton("취소", null).create());
    }

    /** One choice in a picker: a picture, a name over a line saying what it is, and a radio mark. */
    LinearLayout optionRow(View picture, int widthDp, int heightDp, String name, String description) {
        LinearLayout row = new LinearLayout(this.style.context);
        row.setGravity(android.view.Gravity.CENTER_VERTICAL);
        row.setPadding(this.style.dp(9), this.style.dp(9), this.style.dp(12), this.style.dp(9));
        LinearLayout.LayoutParams rowParams = new LinearLayout.LayoutParams(-1, -2);
        rowParams.topMargin = this.style.dp(8);
        row.setLayoutParams(rowParams);
        row.setClickable(true);

        picture.setClipToOutline(true);
        picture.setOutlineProvider(new android.view.ViewOutlineProvider() {
            @Override
            public void getOutline(View view, android.graphics.Outline outline) {
                outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(), ControlDialogs.this.style.dp(9));
            }
        });
        row.addView(picture, new LinearLayout.LayoutParams(this.style.dp(widthDp), this.style.dp(heightDp)));

        LinearLayout text = new LinearLayout(this.style.context);
        text.setOrientation(LinearLayout.VERTICAL);
        text.setPadding(this.style.dp(12), 0, this.style.dp(8), 0);
        TextView title = this.style.text(name, 14.5f, this.style.INK);
        title.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        text.addView(title);
        TextView line = this.style.text(description, 12f, this.style.MUTED);
        line.setPadding(0, this.style.dp(3), 0, 0);
        text.addView(line);
        row.addView(text, new LinearLayout.LayoutParams(0, -2, 1f));

        TextView radio = new TextView(this.style.context);
        radio.setTag("radio");
        row.addView(radio, new LinearLayout.LayoutParams(this.style.dp(20), this.style.dp(20)));
        return row;
    }

    /** Outlines the chosen row and fills its radio mark. */
    void markOption(List<LinearLayout> rows, int chosen) {
        for (int index = 0; index < rows.size(); index++) {
            boolean on = index == chosen;
            LinearLayout row = rows.get(index);
            row.setBackground(this.style.rounded(on ? this.style.SOFT : this.style.BG, on ? this.style.GREEN : this.style.LINE, on ? 2 : 1, 14));
            View radio = row.findViewWithTag("radio");
            android.graphics.drawable.GradientDrawable mark = new android.graphics.drawable.GradientDrawable();
            mark.setShape(android.graphics.drawable.GradientDrawable.OVAL);
            mark.setColor(on ? this.style.BG : 0);
            mark.setStroke(this.style.dp(on ? 6 : 2), on ? this.style.GREEN : this.style.MUTED);
            radio.setBackground(mark);
        }
    }

    EditText nameField() {
        EditText input = this.style.input();
        input.setSingleLine(true);
        input.setHint("배치 이름");
        input.setFilters(new InputFilter[]{new InputFilter.LengthFilter(40)});
        return input;
    }

    /**
     * One of the two layouts drawn as the handset's keypad - the gamepad's
     * buttons, or a folder phone's and a keyboard's own keys - and what goes
     * with it while it is up.
     */
    static final class KeyLayout {
        final boolean physical;
        KeypadPickerView keypad;
        Dialog dialog;
        TextView[] tabs;
        TextView status;
        TextView strip;
        /** The layer shown: SELECT held, or not. The pad's only. */
        boolean layer;
        /** The handset key being asked for, -1 for none. */
        int captureKey = -1;
        boolean captureSelect;
        Dialog captureDialog;
        TextView captureNote;

        KeyLayout(boolean physical) {
            this.physical = physical;
        }
    }

    final KeyLayout padLayout = new KeyLayout(false);
    final KeyLayout keyLayout = new KeyLayout(true);

    /** The gamepad's buttons on the handset's keypad. See {@link #keypadMenu}. */
    void padMenu() {
        keypadMenu(this.padLayout);
    }

    /** A folder phone's and a keyboard's own keys on the handset's keypad. See {@link #keypadMenu}. */
    void keyMenu() {
        keypadMenu(this.keyLayout);
    }

    /**
     * A layout the way the keyboard's is on the PC builds: the handset's
     * keypad, each key with what presses it - for the pad, in one layer,
     * plain or with SELECT held - and a key picked asks for the button or key
     * to put on it. One already on another key moves over; one already on
     * this key comes off.
     */
    void keypadMenu(final KeyLayout layout) {
        LinearLayout column = column();

        LinearLayout header = new LinearLayout(this.style.context);
        header.setGravity(16);
        header.setPadding(0, this.style.dp(2.0f), 0, this.style.dp(4.0f));
        TextView title = this.style.text(layout.physical ? "폴더폰 키패드·키보드 키 매핑" : "게임패드 버튼 배치", layout.physical ? 16.0f : 18.0f, this.style.INK);
        title.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        header.addView(title, new LinearLayout.LayoutParams(0, -2, 1.0f));
        header.addView(headerButton("프리셋", new Runnable() {
            @Override
            public void run() {
                ControlDialogs.this.padPresets();
            }
        }));
        header.addView(headerButton("↺ 기본값", new Runnable() {
            @Override
            public void run() {
                if (layout.physical) {
                    ControlDialogs.this.confirmPhoneReset();
                } else {
                    ControlDialogs.this.confirmPadReset();
                }
            }
        }));
        column.addView(header);
        column.addView(this.style.hint(layout.physical
                ? "폰 키를 고른 뒤 쓸 폴더폰 키나 키보드 키를 누르세요. 다른 키에 있던 키는 옮겨 오고, 이 키에 이미 있는 키를 누르면 빠집니다. 패드 버튼은 게임패드 버튼 배치에서 바꿉니다."
                : "폰 키를 고른 뒤 쓸 패드 버튼을 누르세요. 다른 키에 있던 버튼은 옮겨 오고, 이 키에 이미 있는 버튼을 누르면 빠집니다. 패드로는 방향키로 고르고 A, SELECT로 칸을 바꿉니다.\n연결된 패드: " + connectedPadName()));

        if (!layout.physical) {
            // The layer shown: a tab each.
            LinearLayout tabs = new LinearLayout(this.style.context);
            tabs.setPadding(0, this.style.dp(6.0f), 0, this.style.dp(6.0f));
            layout.tabs = new TextView[2];
            for (int i = 0; i < 2; i++) {
                final boolean layer = i == 1;
                TextView tab = this.style.text(layer ? "SELECT 누른 채" : "그냥 누를 때", 13.0f, this.style.INK);
                tab.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
                tab.setPadding(this.style.dp(14.0f), this.style.dp(6.0f), this.style.dp(14.0f), this.style.dp(6.0f));
                tab.setOnClickListener(new View.OnClickListener() {
                    @Override
                    public void onClick(View view) {
                        layout.layer = layer;
                        ControlDialogs.this.refreshPadRows();
                    }
                });
                LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-2, -2);
                params.rightMargin = this.style.dp(6.0f);
                tabs.addView(tab, params);
                layout.tabs[i] = tab;
            }
            column.addView(tabs);
        } else {
            layout.layer = false;
        }

        layout.keypad = new KeypadPickerView(this.style, 4, keypadNotes(layout), new KeypadPickerView.Listener() {
            @Override
            public void focus(int key) {
                layout.status.setText("");
                ControlDialogs.this.refreshStrip(layout);
            }

            @Override
            public void choose(int key) {
                ControlDialogs.this.captureFor(layout, key);
            }
        });
        LinearLayout.LayoutParams keypadParams = new LinearLayout.LayoutParams(-1, -2);
        keypadParams.topMargin = this.style.dp(layout.physical ? 8.0f : 0.0f);
        column.addView(layout.keypad, keypadParams);

        layout.status = this.style.text("", 12.5f, this.style.DEEP);
        layout.status.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        layout.status.setGravity(17);
        layout.status.setPadding(0, this.style.dp(6.0f), 0, 0);
        column.addView(layout.status, new LinearLayout.LayoutParams(-1, -2));
        layout.strip = this.style.text("", 14.0f, this.style.INK);
        layout.strip.setPadding(this.style.dp(12.0f), this.style.dp(10.0f), this.style.dp(12.0f), this.style.dp(10.0f));
        layout.strip.setBackground(this.style.rounded(this.style.SOFTER, this.style.LINE, 1, 12));
        LinearLayout.LayoutParams stripParams = new LinearLayout.LayoutParams(-1, -2);
        stripParams.topMargin = this.style.dp(6.0f);
        column.addView(layout.strip, stripParams);

        if (!layout.physical) {
            LinearLayout more = this.style.section(column, "물리 키");
            this.style.menuRow(more, "⌨", "폴더폰 키패드·키보드 키 매핑", new View.OnClickListener() {
                @Override
                public void onClick(View view) {
                    ControlDialogs.this.keyMenu();
                }
            });
        }

        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        PadDialog dialog = new PadDialog(this.s.a, this.style.themeId);
        dialog.setView(scrollView);
        dialog.setButton(-2, "닫기", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i3) {
            }
        });
        layout.dialog = dialog;
        refreshPadRows();
        track(dialog, new Runnable() {
            @Override
            public void run() {
                layout.keypad = null;
                layout.dialog = null;
                layout.tabs = null;
            }
        });
    }

    /**
     * Whether a key code is one only a gamepad sends - a face, shoulder or
     * menu button - rather than one a phone's own D-pad or keypad sends too.
     */
    static boolean padOnly(int code) {
        return code >= KeyEvent.KEYCODE_BUTTON_A && code <= KeyEvent.KEYCODE_BUTTON_MODE;
    }

    /** By handset key, what presses it in the layer a layout shows. */
    String[] keypadNotes(KeyLayout layout) {
        String[] notes = new String[22];
        for (int key = 0; key < notes.length; key++) {
            notes[key] = keysOn(layout, key, layout.layer);
        }
        return notes;
    }

    /** What presses `key` on a layout in one layer, by name, or "" for nothing. */
    String keysOn(KeyLayout layout, int key, boolean withSelect) {
        StringBuilder names = new StringBuilder();
        if (layout.physical) {
            for (Map.Entry<Integer, Integer> entry : this.s.mapping.entrySet()) {
                if (entry.getValue() == key && !padOnly(entry.getKey())) {
                    if (names.length() > 0) {
                        names.append(" · ");
                    }
                    names.append(shortPadLabel(entry.getKey()));
                }
            }
            return names.toString();
        }
        Map<Integer, Integer> layer = withSelect ? this.s.selectMapping : this.s.mapping;
        for (int button = 0; button < ControlPads.PAD_CODES.length; button++) {
            Integer on = layer.get(ControlPads.PAD_CODES[button]);
            if (on != null && on == key) {
                if (names.length() > 0) {
                    names.append(" · ");
                }
                names.append(ControlPads.PAD_NAMES[button]);
            }
        }
        return names.toString();
    }

    /** The key picked, and what is on it - in each layer, for the pad. */
    void refreshStrip(KeyLayout layout) {
        if (layout.keypad == null || layout.strip == null) {
            return;
        }
        int key = layout.keypad.picked();
        String plain = keysOn(layout, key, false);
        if (layout.physical) {
            layout.strip.setText(KeypadPickerView.label(key) + "  →  " + (plain.isEmpty() ? "없음" : plain));
            return;
        }
        String select = keysOn(layout, key, true);
        layout.strip.setText(KeypadPickerView.label(key) + "  →  그냥: " + (plain.isEmpty() ? "없음" : plain) + "   SELECT+: " + (select.isEmpty() ? "없음" : select));
    }

    /**
     * Asks for the button or key to put on `key` in the layer a layout shows:
     * pressed, it goes on (or comes off, if it was there). 비우기 takes
     * everything off the key in that layer; 취소 leaves it.
     */
    void captureFor(final KeyLayout layout, final int key) {
        final boolean withSelect = layout.layer;
        final String where = layout.physical ? "" : " (" + (withSelect ? "SELECT 누른 채" : "그냥 누를 때") + ")";
        LinearLayout column = column();
        TextView title = this.style.text(KeypadPickerView.label(key) + where, 18.0f, this.style.INK);
        title.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        column.addView(title);
        String now = keysOn(layout, key, withSelect);
        column.addView(this.style.hint("지금: " + (now.isEmpty() ? "없음" : now)));
        TextView ask = this.style.text(layout.physical ? "쓸 키를 누르세요" : "쓸 버튼을 누르세요", 20.0f, this.style.INK);
        ask.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        ask.setGravity(17);
        ask.setPadding(0, this.style.dp(18.0f), 0, this.style.dp(4.0f));
        column.addView(ask, new LinearLayout.LayoutParams(-1, -2));
        layout.captureNote = this.style.text("…", 13.0f, this.style.DEEP);
        layout.captureNote.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        layout.captureNote.setGravity(17);
        column.addView(layout.captureNote, new LinearLayout.LayoutParams(-1, -2));
        TextView rule = this.style.hint(layout.physical
                ? "다른 키에 있던 키는 이 키로 옮겨 오고, 이미 이 키에 있는 키를 누르면 빠집니다."
                : "다른 키에 있던 버튼은 이 키로 옮겨 오고, 이미 이 키에 있는 버튼을 누르면 빠집니다.");
        rule.setGravity(17);
        column.addView(rule, new LinearLayout.LayoutParams(-1, -2));

        PadDialog dialog = new PadDialog(this.s.a, this.style.themeId);
        dialog.setView(column);
        dialog.setButton(-2, "취소", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i) {
            }
        });
        dialog.setButton(-3, "비우기", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i) {
                ControlDialogs.this.clearKey(layout, key, withSelect);
            }
        });
        if (layout.physical) {
            // A key that is neither a pad's nor a phone's - a keyboard's
            // letters - never reaches a game, so it is not one to put on a key.
            dialog.setOnKeyListener(new DialogInterface.OnKeyListener() {
                @Override
                public boolean onKey(DialogInterface dialogInterface, int code, KeyEvent keyEvent) {
                    if (code == KeyEvent.KEYCODE_BACK || keyEvent.getAction() != 0) {
                        return false;
                    }
                    if (layout.captureNote != null) {
                        layout.captureNote.setText("이 키는 쓸 수 없습니다.");
                    }
                    return true;
                }
            });
        }
        layout.captureKey = key;
        layout.captureSelect = withSelect;
        layout.captureDialog = dialog;
        this.captureArmed = this.s.axesNeutral;
        ControlPatch.input.releaseAll();
        track(dialog, new Runnable() {
            @Override
            public void run() {
                layout.captureKey = -1;
                layout.captureDialog = null;
                layout.captureNote = null;
            }
        });
    }

    /** Takes everything off `key` on a layout in one layer. */
    void clearKey(KeyLayout layout, int key, boolean withSelect) {
        Map<Integer, Integer> map = withSelect ? this.s.selectMapping : this.s.mapping;
        java.util.Iterator<Map.Entry<Integer, Integer>> entries = map.entrySet().iterator();
        while (entries.hasNext()) {
            Map.Entry<Integer, Integer> entry = entries.next();
            int code = entry.getKey();
            boolean ours = layout.physical ? !padOnly(code) : ControlPads.isPadCode(code);
            if (entry.getValue() == key && ours) {
                entries.remove();
            }
        }
        this.s.savePads();
        ControlPatch.input.releaseAll();
        layout.status.setText(KeypadPickerView.label(key) + ": " + (layout.physical ? "" : (withSelect ? "SELECT 누른 채 " : "그냥 누를 때 ")) + "모두 뺐습니다.");
        refreshPadRows();
    }

    /** Puts `code` on the key a layout is asking for, or takes it off. */
    void putKey(KeyLayout layout, int code) {
        int key = layout.captureKey;
        if (key < 0) {
            return;
        }
        String name;
        if (layout.physical) {
            name = shortPadLabel(code);
        } else {
            if (code == KeyEvent.KEYCODE_DPAD_CENTER) {
                code = KeyEvent.KEYCODE_BUTTON_A;
            }
            name = null;
            for (int button = 0; button < ControlPads.PAD_CODES.length; button++) {
                if (ControlPads.PAD_CODES[button] == code) {
                    name = ControlPads.PAD_NAMES[button];
                }
            }
            if (name == null) {
                if (layout.captureNote != null) {
                    layout.captureNote.setText("이 버튼은 쓸 수 없습니다.");
                }
                return;
            }
        }
        Map<Integer, Integer> map = layout.captureSelect ? this.s.selectMapping : this.s.mapping;
        // The D-pad's centre goes with A, as a pad may send either.
        int[] codes = !layout.physical && code == KeyEvent.KEYCODE_BUTTON_A ? new int[]{code, KeyEvent.KEYCODE_DPAD_CENTER} : new int[]{code};
        Integer before = map.get(code);
        String outcome;
        if (before != null && before == key) {
            for (int each : codes) {
                map.remove(each);
            }
            outcome = name + ": '" + KeypadPickerView.label(key) + "'에서 뺐습니다.";
        } else {
            for (int each : codes) {
                map.put(each, key);
            }
            outcome = before != null && before >= 0
                    ? name + ": '" + KeypadPickerView.label(before) + "'에서 옮겨 왔습니다."
                    : name + ": '" + KeypadPickerView.label(key) + "'에 붙였습니다.";
        }
        this.s.savePads();
        ControlPatch.input.releaseAll();
        if (layout.status != null) {
            layout.status.setText(outcome);
        }
        if (layout.captureDialog != null) {
            layout.captureDialog.dismiss();
        }
        refreshPadRows();
    }

    /** Puts the pad's buttons, both layers, back to the defaults, after asking. */
    void confirmPadReset() {
        track(builder("↺ 기본값으로 되돌리기").setMessage("버튼 배치를 처음 설정으로 되돌릴까요?\n\n패드 버튼의 두 칸만 바뀌어요. 폴더폰 키패드·키보드 키 매핑은 그대로예요.").setPositiveButton("되돌리기", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i) {
                ControlPads.resetPadButtons(ControlDialogs.this.s.mapping);
                ControlDialogs.this.s.selectMapping.clear();
                ControlDialogs.this.s.selectMapping.putAll(ControlPads.selectDefaults());
                ControlDialogs.this.s.savePads();
                ControlDialogs.this.refreshPadRows();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    /**
     * A pad button pressed while a keypad layout is up - the key table's, if
     * it is over the pad's. Asked for a pad button, any but SELECT (the layer
     * key) and HOME goes on the key; the key table takes no pad buttons. On
     * the keypad, the D-pad and A pick a key, SELECT turns to the other layer
     * and B closes it. Whether the event was a layout's.
     */
    boolean layoutKey(KeyEvent keyEvent) {
        int code = keyEvent.getKeyCode();
        boolean down = keyEvent.getAction() == 0 && keyEvent.getRepeatCount() == 0;
        if (this.keyLayout.captureKey >= 0) {
            if (down) {
                if (padOnly(code)) {
                    this.keyLayout.captureNote.setText("패드 버튼은 게임패드 버튼 배치에서 바꿉니다.");
                } else {
                    putKey(this.keyLayout, code);
                }
            }
            return true;
        }
        if (this.padLayout.captureKey >= 0) {
            if (down) {
                if (code == ControlPads.SELECT) {
                    this.padLayout.captureNote.setText("SELECT는 칸 전환이라 쓸 수 없습니다.");
                } else if (code == KeyEvent.KEYCODE_BUTTON_START && this.padLayout.captureSelect) {
                    this.padLayout.captureNote.setText("SELECT+START는 쓸 수 없습니다.");
                } else {
                    putKey(this.padLayout, code);
                }
            }
            return true;
        }
        KeyLayout layout = this.keyLayout.keypad != null ? this.keyLayout : this.padLayout;
        if (layout.keypad == null) {
            return false;
        }
        if (code != ControlPads.SELECT && !ControlPads.isPadCode(code)) {
            return false;
        }
        if (keyEvent.getAction() != 0 || (keyEvent.getRepeatCount() != 0 && !isDirection(code))) {
            return true;
        }
        if (code == ControlPads.SELECT) {
            if (!layout.physical) {
                layout.layer = !layout.layer;
                refreshPadRows();
            }
        } else if (code == KeyEvent.KEYCODE_BUTTON_B && layout.dialog != null) {
            layout.dialog.dismiss();
        } else {
            layout.keypad.padKey(code);
        }
        return true;
    }

    static boolean isDirection(int code) {
        return code == KeyEvent.KEYCODE_DPAD_UP || code == KeyEvent.KEYCODE_DPAD_DOWN
                || code == KeyEvent.KEYCODE_DPAD_LEFT || code == KeyEvent.KEYCODE_DPAD_RIGHT;
    }

    /** Puts a phone's own keys back where they start, leaving the pad's buttons, after asking. */
    void confirmPhoneReset() {
        track(builder("↺ 기본값으로 되돌리기").setMessage("폴더폰 키패드·키보드 키 매핑을 처음 설정으로 되돌릴까요?\n\n게임패드 버튼 배치는 그대로예요.").setPositiveButton("되돌리기", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i) {
                java.util.Iterator<Integer> codes = ControlDialogs.this.s.mapping.keySet().iterator();
                while (codes.hasNext()) {
                    int code = codes.next();
                    if (!padOnly(code) && !ControlPads.isPadCode(code)) {
                        codes.remove();
                    }
                }
                for (Map.Entry<Integer, Integer> entry : ControlPads.phoneDefaults().entrySet()) {
                    if (!ControlPads.isPadCode(entry.getKey())) {
                        ControlDialogs.this.s.mapping.put(entry.getKey(), entry.getValue());
                    }
                }
                ControlDialogs.this.s.savePads();
                ControlDialogs.this.refreshPadRows();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    /** A small secondary button for the pad menu header. */
    Button headerButton(String text, final Runnable action) {
        Button button = this.style.button(text, false);
        button.setTextSize(12.5f);
        button.setMinHeight(this.style.dp(34.0f));
        button.setMinimumHeight(this.style.dp(34.0f));
        button.setPadding(this.style.dp(12.0f), this.style.dp(4.0f), this.style.dp(12.0f), this.style.dp(4.0f));
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(-2, -2);
        params.leftMargin = this.style.dp(6.0f);
        button.setLayoutParams(params);
        button.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View view) {
                action.run();
            }
        });
        return button;
    }

    /** Name of the first connected gamepad, or 없음. */
    String connectedPadName() {
        try {
            for (int id : android.view.InputDevice.getDeviceIds()) {
                android.view.InputDevice device = android.view.InputDevice.getDevice(id);
                if (device == null) {
                    continue;
                }
                int src = device.getSources();
                boolean pad = (src & android.view.InputDevice.SOURCE_GAMEPAD) == android.view.InputDevice.SOURCE_GAMEPAD
                        || (src & android.view.InputDevice.SOURCE_JOYSTICK) == android.view.InputDevice.SOURCE_JOYSTICK;
                if (pad) {
                    String name = device.getName();
                    if (name != null && name.length() > 0) {
                        return name;
                    }
                }
            }
        } catch (Exception e) {
            // fall through
        }
        return "없음";
    }

    void padPresets() {
        track(builder("게임패드 프리셋").setItems(new String[]{"현재 매핑 저장", "저장한 매핑 불러오기·삭제"}, new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.27
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i) {
                if (i == 0) {
                    ControlDialogs.this.saveName(true);
                } else {
                    ControlDialogs.this.loadSlots(true);
                }
            }
        }).setNegativeButton("닫기", (DialogInterface.OnClickListener) null).create());
    }

    void pickFile(boolean z) {
        try {
            Intent intent = new Intent(z ? "android.intent.action.CREATE_DOCUMENT" : "android.intent.action.OPEN_DOCUMENT");
            intent.addCategory("android.intent.category.OPENABLE");
            intent.setType(z ? "application/json" : "*/*");
            if (z) {
                this.pendingBackup = this.s.data.encode();
                intent.putExtra("android.intent.extra.TITLE", "MiniMobile_keypad_" + new SimpleDateFormat("yyyyMMdd_HHmmss", Locale.US).format(new Date()) + ".json");
            }
            this.s.fileBusy = true;
            this.s.syncPause();
            this.s.a.startActivityForResult(intent, z ? EXPORT_LAYOUT : IMPORT_LAYOUT);
        } catch (Exception e) {
            error("파일 선택기를 열지 못했습니다: " + e.getMessage());
            this.s.fileBusy = false;
            this.s.syncPause();
        }
    }

    void rapidSettings() {
        final ControlData.RapidSettings copy = this.s.data.rapid.copy();
        final boolean[] state = new boolean[21];
        for (int c = 0; c < 21; c++) {
            state[c] = copy.enabled[c];
        }
        final View[] chipViews = new View[21];
        final int[] period = {copy.periodMs};

        LinearLayout column = column();
        column.addView(this.style.hint("누르고 있는 동안 자동으로 반복됩니다."));
        addKeyGroups(column, state, chipViews);
        column.addView(buildSpeedSlider(period));
        ScrollView scroll = new ScrollView(this.style.context);
        scroll.addView(column);

        track(builder("연사 ON/OFF").setView(scroll).setPositiveButton("적용", new DialogInterface.OnClickListener() {
            @Override
            public void onClick(DialogInterface dialogInterface, int i) {
                ControlPatch.input.releaseAll();
                for (int c = 0; c < 21; c++) {
                    copy.enabled[c] = state[c];
                }
                copy.periodMs = period[0];
                ControlDialogs.this.s.data.rapid = copy.copy();
                ControlDialogs.this.s.saveLayout();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    /** The three grouped rows of toggle chips shared by 표시·숨김 and 연사. */
    void addKeyGroups(LinearLayout column, boolean[] state, View[] chipViews) {
        addKeyGroup(column, "방향·확인", new int[]{0, 1, 2, 3, 4}, 3, true, state, chipViews);
        addKeyGroup(column, "기능", new int[]{5, 6, 7, 20}, 2, true, state, chipViews);
        addKeyGroup(column, "숫자·기호", new int[]{9, 10, 11, 12, 13, 14, 15, 16, 17, 8, 18, 19}, 5, false, state, chipViews);
    }

    void addKeyGroup(LinearLayout column, String title, int[] codes, int cols, boolean nameShown, boolean[] state, View[] chipViews) {
        TextView label = this.style.text(title, 11.5f, this.style.GREEN);
        label.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        label.setPadding(this.style.dp(2.0f), this.style.dp(12.0f), 0, this.style.dp(7.0f));
        column.addView(label);

        LinearLayout box = new LinearLayout(this.style.context);
        box.setOrientation(1);
        for (int start = 0; start < codes.length; start += cols) {
            LinearLayout rowLine = new LinearLayout(this.style.context);
            rowLine.setOrientation(0);
            for (int c = 0; c < cols; c++) {
                LinearLayout.LayoutParams cellParams = new LinearLayout.LayoutParams(0, -2, 1.0f);
                cellParams.leftMargin = c == 0 ? 0 : this.style.dp(6.0f);
                cellParams.bottomMargin = this.style.dp(6.0f);
                int index = start + c;
                if (index >= codes.length) {
                    rowLine.addView(new View(this.style.context), cellParams);
                    continue;
                }
                rowLine.addView(buildToggleChip(codes[index], nameShown, state, chipViews), cellParams);
            }
            box.addView(rowLine, new LinearLayout.LayoutParams(-1, -2));
        }
        column.addView(box);
    }

    View buildToggleChip(final int code, boolean nameShown, final boolean[] state, final View[] chipViews) {
        final LinearLayout chip = new LinearLayout(this.style.context);
        chip.setGravity(16);
        chip.setPadding(this.style.dp(8.0f), this.style.dp(7.0f), this.style.dp(9.0f), this.style.dp(7.0f));
        chip.setMinimumHeight(this.style.dp(40.0f));

        TextView badge = this.style.text(ControlData.BADGES[code], 12.0f, this.style.DEEP);
        badge.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        badge.setGravity(17);
        badge.setBackground(this.style.rounded(this.style.light ? this.style.SOFTER : android.graphics.Color.rgb(43, 46, 55), this.style.SOFT_LINE, 1, 7));
        LinearLayout.LayoutParams badgeParams = new LinearLayout.LayoutParams(this.style.dp(24.0f), this.style.dp(24.0f));
        badgeParams.rightMargin = this.style.dp(7.0f);
        chip.addView(badge, badgeParams);

        if (nameShown && ControlData.SHORT[code].length() > 0) {
            TextView name = this.style.text(ControlData.SHORT[code], 13.0f, this.style.INK);
            chip.addView(name, new LinearLayout.LayoutParams(0, -2, 1.0f));
        } else {
            chip.addView(new View(this.style.context), new LinearLayout.LayoutParams(0, -2, 1.0f));
        }

        TextView check = this.style.text("✓", 12.0f, this.style.DEEP);
        check.setPadding(this.style.dp(2.0f), 0, 0, 0);
        chip.addView(check);
        chip.setTag(check);

        chipViews[code] = chip;
        styleToggleChip(chip, state[code]);
        chip.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View view) {
                state[code] = !state[code];
                styleToggleChip(chip, state[code]);
            }
        });
        return chip;
    }

    void styleToggleChip(LinearLayout chip, boolean on) {
        int fill;
        int line;
        if (on) {
            fill = this.style.light ? this.style.SOFT : android.graphics.Color.rgb(23, 50, 58);
            line = this.style.GREEN;
        } else {
            fill = this.style.light ? this.style.BG : android.graphics.Color.rgb(35, 38, 46);
            line = this.style.SOFT_LINE;
        }
        chip.setBackground(this.style.rounded(fill, line, 1, 11));
        Object tag = chip.getTag();
        if (tag instanceof View) {
            ((View) tag).setVisibility(on ? View.VISIBLE : View.INVISIBLE);
        }
    }

    void refreshToggleChips(View[] chipViews, boolean[] state) {
        for (int c = 0; c < chipViews.length; c++) {
            if (chipViews[c] instanceof LinearLayout) {
                styleToggleChip((LinearLayout) chipViews[c], state[c]);
            }
        }
    }

    /** The inline rapid-fire speed slider (2·3·5·8·10/s) for the 연사 dialog. */
    View buildSpeedSlider(final int[] period) {
        LinearLayout box = new LinearLayout(this.style.context);
        box.setOrientation(1);
        box.setPadding(this.style.dp(12.0f), this.style.dp(12.0f), this.style.dp(12.0f), this.style.dp(12.0f));
        box.setBackground(this.style.rounded(this.style.light ? this.style.SOFTER : android.graphics.Color.rgb(35, 38, 46), this.style.SOFT_LINE, 1, 12));
        LinearLayout.LayoutParams boxParams = new LinearLayout.LayoutParams(-1, -2);
        boxParams.topMargin = this.style.dp(14.0f);
        box.setLayoutParams(boxParams);

        LinearLayout labelRow = new LinearLayout(this.style.context);
        labelRow.setGravity(16);
        labelRow.addView(this.style.text("연사 속도", 12.0f, this.style.MUTED), new LinearLayout.LayoutParams(0, -2, 1.0f));
        final TextView value = this.style.text("초당 " + ControlRapid.rate(period[0]) + "회", 13.0f, this.style.DEEP);
        value.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        labelRow.addView(value);
        box.addView(labelRow);

        int index = 2;
        for (int k = 0; k < ControlRapid.PERIODS_MS.length; k++) {
            if (ControlRapid.PERIODS_MS[k] == period[0]) {
                index = k;
            }
        }
        android.widget.SeekBar bar = new android.widget.SeekBar(this.style.context);
        bar.setMax(ControlRapid.PERIODS_MS.length - 1);
        bar.setProgress(index);
        bar.setOnSeekBarChangeListener(new android.widget.SeekBar.OnSeekBarChangeListener() {
            @Override
            public void onProgressChanged(android.widget.SeekBar seekBar, int progress, boolean fromUser) {
                period[0] = ControlRapid.PERIODS_MS[progress];
                value.setText("초당 " + ControlRapid.RATES[progress] + "회");
            }

            @Override
            public void onStartTrackingTouch(android.widget.SeekBar seekBar) {
            }

            @Override
            public void onStopTrackingTouch(android.widget.SeekBar seekBar) {
            }
        });
        box.addView(bar, new LinearLayout.LayoutParams(-1, -2));

        LinearLayout ticks = new LinearLayout(this.style.context);
        for (int k = 0; k < ControlRapid.RATES.length; k++) {
            TextView tick = this.style.text(String.valueOf(ControlRapid.RATES[k]), 10.5f, k == index ? this.style.DEEP : this.style.MUTED);
            tick.setGravity(17);
            ticks.addView(tick, new LinearLayout.LayoutParams(0, -2, 1.0f));
        }
        box.addView(ticks);
        return box;
    }

    void rapidSpeed(final ControlData.RapidSettings rapidSettings, final AlertDialog alertDialog) {
        int length = ControlRapid.PERIODS_MS.length;
        String[] strArr = new String[length];
        final int[] iArr = {rapidSettings.periodMs};
        int i = 0;
        for (int i2 = 0; i2 < length; i2++) {
            int i3 = ControlRapid.PERIODS_MS[i2];
            strArr[i2] = "초당 " + ControlRapid.RATES[i2] + "회" + (i3 == 200 ? " · 기본" : "");
            if (i3 == rapidSettings.periodMs) {
                i = i2;
            }
        }
        track(builder("연사 속도 · 모든 연사 버튼에 적용").setSingleChoiceItems(strArr, i, new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.6
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                iArr[0] = ControlRapid.PERIODS_MS[i4];
            }
        }).setPositiveButton("선택", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.7
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i4) {
                rapidSettings.periodMs = iArr[0];
                alertDialog.setTitle(ControlDialogs.this.rapidTitle(rapidSettings));
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    String rapidTitle(ControlData.RapidSettings rapidSettings) {
        return "누르는 동안 연사 · 초당 " + ControlRapid.rate(rapidSettings.periodMs) + "회";
    }

    /** Short chip label for a pad code (D-pad glyphs, else the physical name). */
    static String shortPadLabel(int code) {
        switch (code) {
            case KeyEvent.KEYCODE_DPAD_UP:
                return "D▲";
            case KeyEvent.KEYCODE_DPAD_DOWN:
                return "D▼";
            case KeyEvent.KEYCODE_DPAD_LEFT:
                return "D◀";
            case KeyEvent.KEYCODE_DPAD_RIGHT:
                return "D▶";
            default:
                return physicalName(code);
        }
    }

    /** Redraws whichever keypad layouts are up, after their mapping changed. */
    void refreshPadRows() {
        for (KeyLayout layout : new KeyLayout[]{this.padLayout, this.keyLayout}) {
            if (layout.keypad != null) {
                layout.keypad.setNotes(keypadNotes(layout));
                refreshStrip(layout);
            }
            if (layout.tabs != null) {
                for (int i = 0; i < layout.tabs.length; i++) {
                    boolean on = (i == 1) == layout.layer;
                    layout.tabs[i].setTextColor(on ? (this.style.light ? android.graphics.Color.WHITE : this.style.BG) : this.style.MUTED);
                    layout.tabs[i].setBackground(this.style.rounded(on ? this.style.GREEN : this.style.SOFTER, this.style.LINE, on ? 0 : 1, 16));
                }
            }
        }
    }

    void resetLayout() {
        final boolean landscape = this.s.editor.landscape();
        final boolean numbers = this.s.editor.numbers();
        track(builder("현재 배치 초기화").setMessage(String.valueOf(landscape ? "가로" : "세로") + " 버튼의 위치·크기·숨김과 격자 간격을 기본값으로 되돌립니다. 저장된 배치는 유지됩니다.").setPositiveButton("초기화", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.11
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i) {
                if (landscape) {
                    ControlDialogs.this.s.data.landscape = new ControlData.Layout();
                } else if (numbers) {
                    ControlDialogs.this.s.data.numbers = new ControlData.Layout();
                } else {
                    ControlDialogs.this.s.data.portrait = new ControlData.Layout();
                }
                ControlDialogs.this.s.saveLayout();
                ControlDialogs.this.s.editor.refresh();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }

    void saveName(final boolean z) {
        final EditText nameField = nameField();
        nameField.setHint(z ? "매핑 이름" : "배치 이름");
        LinearLayout column = column();
        column.addView(nameField);
        column.addView(this.style.hint(z ? "현재 게임패드 매핑을 저장합니다." : "세로·가로 배치, 버튼 숨김·격자·연사 설정을 함께 저장합니다."));
        final AlertDialog create = builder(z ? "게임패드 프리셋 저장" : "키패드 배치 저장").setView(column).setPositiveButton("저장", (DialogInterface.OnClickListener) null).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create();
        track(create);
        create.getButton(-1).setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.12
            @Override // android.view.View.OnClickListener
            public void onClick(View view) {
                EditText editText;
                String str;
                final String name = ControlData.name(nameField.getText().toString());
                if (name.length() != 0) {
                    boolean containsKey = (z ? ControlDialogs.this.s.padSlots : ControlDialogs.this.s.data.slots).containsKey(name);
                    if (!containsKey) {
                        if ((z ? ControlDialogs.this.s.padSlots : ControlDialogs.this.s.data.slots).size() >= 32) {
                            editText = nameField;
                            str = "최대 32개까지 저장할 수 있습니다.";
                        }
                    }
                    final boolean z2 = z;
                    final AlertDialog alertDialog = create;
                    final Runnable runnable = new Runnable() { // from class: com.jjongjjongs.minimobile.ControlDialogs.12.1
                        @Override // java.lang.Runnable
                        public void run() {
                            if (z2) {
                                ControlDialogs.this.s.padSlots.put(name, ControlDialogs.this.s.snapshot());
                                ControlDialogs.this.s.savePads();
                            } else {
                                ControlDialogs.this.s.data.slots.put(name, ControlDialogs.this.s.data.activeCopy());
                                ControlDialogs.this.s.saveLayout();
                            }
                            ControlPatch.toast(ControlDialogs.this.s.a, "‘" + name + "’ 저장 완료");
                            alertDialog.dismiss();
                        }
                    };
                    if (containsKey) {
                        ControlDialogs.this.track(ControlDialogs.this.builder("같은 이름이 있습니다").setMessage("‘" + name + "’을 현재 설정으로 바꿀까요?").setPositiveButton("덮어쓰기", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.12.2
                            @Override // android.content.DialogInterface.OnClickListener
                            public void onClick(DialogInterface dialogInterface, int i) {
                                runnable.run();
                            }
                        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
                        return;
                    } else {
                        runnable.run();
                        return;
                    }
                }
                editText = nameField;
                str = "이름을 입력해 주세요.";
                editText.setError(str);
            }
        });
    }

    void track(Dialog dialog) {
        track(dialog, null);
    }

    void track(final Dialog dialog, final Runnable runnable) {
        this.s.dialogs.add(dialog);
        this.s.syncPause();
        dialog.setOnDismissListener(new DialogInterface.OnDismissListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.1
            @Override // android.content.DialogInterface.OnDismissListener
            public void onDismiss(DialogInterface dialogInterface) {
                ControlDialogs.this.s.dialogs.remove(dialog);
                if (runnable != null) {
                    runnable.run();
                }
                ControlDialogs.this.s.syncPause();
            }
        });
        dialog.show();
        if (dialog instanceof AlertDialog) {
            this.style.decorate((AlertDialog) dialog);
        }
    }

    void visibility() {
        final ControlData.Layout layout = this.s.editor.layout();
        final boolean[] state = new boolean[21];
        for (int c = 0; c < 21; c++) {
            state[c] = !layout.hidden[c];
        }
        final View[] chipViews = new View[21];

        LinearLayout column = column();
        column.addView(this.style.hint("끈 버튼은 키패드에서 사라집니다."));
        addKeyGroups(column, state, chipViews);
        ScrollView scroll = new ScrollView(this.style.context);
        scroll.addView(column);

        final AlertDialog dialog = builder(String.valueOf(this.s.editor.landscape() ? "가로" : "세로") + " · 표시할 버튼")
                .setView(scroll)
                .setPositiveButton("적용", new DialogInterface.OnClickListener() {
                    @Override
                    public void onClick(DialogInterface dialogInterface, int i) {
                        ControlPatch.input.releaseAll();
                        for (int c = 0; c < 21; c++) {
                            layout.hidden[c] = !state[c];
                        }
                        ControlDialogs.this.s.saveLayout();
                        ControlDialogs.this.s.editor.refresh();
                    }
                })
                .setNeutralButton("모두 표시", (DialogInterface.OnClickListener) null)
                .setNegativeButton("취소", (DialogInterface.OnClickListener) null)
                .create();
        track(dialog);
        // 모두 표시: check everything without closing the dialog.
        dialog.getButton(-3).setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View view) {
                for (int c = 0; c < 21; c++) {
                    state[c] = true;
                }
                refreshToggleChips(chipViews, state);
            }
        });
    }
}
