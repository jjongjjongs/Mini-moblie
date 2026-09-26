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
import java.util.Locale;
import java.util.Map;
import java.util.SortedMap;
import java.util.TreeMap;

final class ControlDialogs {
    static final int EXPORT_LAYOUT = 6211;
    static final int IMPORT_LAYOUT = 6212;
    boolean captureArmed;
    AlertDialog captureDialog;
    Button[] padRows;
    String pendingBackup;
    final ControlPatch.Session s;
    final ControlStyle style;
    final Handler main = new Handler(Looper.getMainLooper());
    int captureTarget = -1;

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

        /* JADX WARN: Code restructure failed: missing block: B:36:0x00a9, code lost:
        
            throw new java.lang.Exception("백업 파일은 1 MB 이하여야 합니다.");
         */
        @Override // java.lang.Runnable
        /*
            Code decompiled incorrectly, please refer to instructions dump.
            To view partially-correct add '--show-bad-code' argument
        */
        public void run() {
            /*
                r7 = this;
                int r0 = r7.val$request     // Catch: java.lang.Exception -> Lb5
                r1 = 6211(0x1843, float:8.703E-42)
                java.lang.String r2 = "UTF-8"
                java.lang.String r3 = "파일을 열 수 없습니다."
                if (r0 != r1) goto L55
                java.lang.String r0 = r7.val$backup     // Catch: java.lang.Exception -> Lb5
                if (r0 != 0) goto L19
                com.jjongjjongs.minimobile.ControlDialogs r0 = com.jjongjjongs.minimobile.ControlDialogs.this     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlPatch$Session r0 = r0.s     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlData r0 = r0.data     // Catch: java.lang.Exception -> Lb5
                java.lang.String r0 = r0.encode()     // Catch: java.lang.Exception -> Lb5
                goto L1b
            L19:
                java.lang.String r0 = r7.val$backup     // Catch: java.lang.Exception -> Lb5
            L1b:
                com.jjongjjongs.minimobile.ControlDialogs r1 = com.jjongjjongs.minimobile.ControlDialogs.this     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlPatch$Session r1 = r1.s     // Catch: java.lang.Exception -> Lb5
                android.app.Activity r1 = r1.a     // Catch: java.lang.Exception -> Lb5
                android.content.ContentResolver r1 = r1.getContentResolver()     // Catch: java.lang.Exception -> Lb5
                android.net.Uri r4 = r7.val$uri     // Catch: java.lang.Exception -> Lb5
                java.lang.String r5 = "wt"
                java.io.OutputStream r1 = r1.openOutputStream(r4, r5)     // Catch: java.lang.Exception -> Lb5
                if (r1 == 0) goto L4f
                byte[] r0 = r0.getBytes(r2)     // Catch: java.lang.Throwable -> L4a
                r1.write(r0)     // Catch: java.lang.Throwable -> L4a
                r1.flush()     // Catch: java.lang.Throwable -> L4a
                r1.close()     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlDialogs r0 = com.jjongjjongs.minimobile.ControlDialogs.this     // Catch: java.lang.Exception -> Lb5
                android.os.Handler r0 = r0.main     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlDialogs$22$1 r1 = new com.jjongjjongs.minimobile.ControlDialogs$22$1     // Catch: java.lang.Exception -> Lb5
                r1.<init>()     // Catch: java.lang.Exception -> Lb5
                r0.post(r1)     // Catch: java.lang.Exception -> Lb5
                goto Lc2
            L4a:
                r0 = move-exception
                r1.close()     // Catch: java.lang.Exception -> Lb5
                throw r0     // Catch: java.lang.Exception -> Lb5
            L4f:
                java.lang.Exception r0 = new java.lang.Exception     // Catch: java.lang.Exception -> Lb5
                r0.<init>(r3)     // Catch: java.lang.Exception -> Lb5
                throw r0     // Catch: java.lang.Exception -> Lb5
            L55:
                com.jjongjjongs.minimobile.ControlDialogs r0 = com.jjongjjongs.minimobile.ControlDialogs.this     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlPatch$Session r0 = r0.s     // Catch: java.lang.Exception -> Lb5
                android.app.Activity r0 = r0.a     // Catch: java.lang.Exception -> Lb5
                android.content.ContentResolver r0 = r0.getContentResolver()     // Catch: java.lang.Exception -> Lb5
                android.net.Uri r1 = r7.val$uri     // Catch: java.lang.Exception -> Lb5
                java.io.InputStream r0 = r0.openInputStream(r1)     // Catch: java.lang.Exception -> Lb5
                if (r0 == 0) goto Laf
                java.io.ByteArrayOutputStream r1 = new java.io.ByteArrayOutputStream     // Catch: java.lang.Exception -> Lb5
                r1.<init>()     // Catch: java.lang.Exception -> Lb5
                r3 = 8192(0x2000, float:1.148E-41)
                byte[] r3 = new byte[r3]     // Catch: java.lang.Throwable -> Laa
            L70:
                int r4 = r0.read(r3)     // Catch: java.lang.Throwable -> Laa
                r5 = -1
                if (r4 != r5) goto L94
                r0.close()     // Catch: java.lang.Exception -> Lb5
                java.lang.String r0 = new java.lang.String     // Catch: java.lang.Exception -> Lb5
                byte[] r1 = r1.toByteArray()     // Catch: java.lang.Exception -> Lb5
                r0.<init>(r1, r2)     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlData r0 = com.jjongjjongs.minimobile.ControlData.decode(r0)     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlDialogs r1 = com.jjongjjongs.minimobile.ControlDialogs.this     // Catch: java.lang.Exception -> Lb5
                android.os.Handler r1 = r1.main     // Catch: java.lang.Exception -> Lb5
                com.jjongjjongs.minimobile.ControlDialogs$22$2 r2 = new com.jjongjjongs.minimobile.ControlDialogs$22$2     // Catch: java.lang.Exception -> Lb5
                r2.<init>()     // Catch: java.lang.Exception -> Lb5
                r1.post(r2)     // Catch: java.lang.Exception -> Lb5
                goto Lc2
            L94:
                int r5 = r1.size()     // Catch: java.lang.Throwable -> Laa
                int r5 = r5 + r4
                r6 = 1048576(0x100000, float:1.469368E-39)
                if (r5 > r6) goto La2
                r5 = 0
                r1.write(r3, r5, r4)     // Catch: java.lang.Throwable -> Laa
                goto L70
            La2:
                java.lang.Exception r1 = new java.lang.Exception     // Catch: java.lang.Throwable -> Laa
                java.lang.String r2 = "백업 파일은 1 MB 이하여야 합니다."
                r1.<init>(r2)     // Catch: java.lang.Throwable -> Laa
                throw r1     // Catch: java.lang.Throwable -> Laa
            Laa:
                r1 = move-exception
                r0.close()     // Catch: java.lang.Exception -> Lb5
                throw r1     // Catch: java.lang.Exception -> Lb5
            Laf:
                java.lang.Exception r0 = new java.lang.Exception     // Catch: java.lang.Exception -> Lb5
                r0.<init>(r3)     // Catch: java.lang.Exception -> Lb5
                throw r0     // Catch: java.lang.Exception -> Lb5
            Lb5:
                r0 = move-exception
                com.jjongjjongs.minimobile.ControlDialogs r1 = com.jjongjjongs.minimobile.ControlDialogs.this
                android.os.Handler r1 = r1.main
                com.jjongjjongs.minimobile.ControlDialogs$22$3 r2 = new com.jjongjjongs.minimobile.ControlDialogs$22$3
                r2.<init>()
                r1.post(r2)
            Lc2:
                return
            */
            throw new UnsupportedOperationException("Method not decompiled: com.jjongjjongs.minimobile.ControlDialogs.AnonymousClass22.run():void");
        }
    }

    static final class PadDialog extends AlertDialog {
        final Activity activity;

        PadDialog(Activity activity) {
            super(activity, ControlStyle.theme(activity));
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
        this.style = new ControlStyle(this.s.a);
    }

    static String physicalName(int i) {
        return KeyEvent.keyCodeToString(i).replace("KEYCODE_BUTTON_", "").replace("KEYCODE_", "");
    }

    boolean activityResult(int i, int i2, Intent intent) {
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

    void assign(int i) {
        if (this.captureTarget < 0) {
            return;
        }
        int i2 = this.captureTarget;
        this.s.mapping.put(Integer.valueOf(i), Integer.valueOf(i2));
        this.s.savePads();
        refreshPadRows();
        AlertDialog alertDialog = this.captureDialog;
        this.captureTarget = -1;
        this.captureDialog = null;
        if (alertDialog != null) {
            alertDialog.dismiss();
        }
        ControlPatch.toast(this.s.a, String.valueOf(physicalName(i)) + " → " + ControlData.NAMES[i2]);
    }

    String assigned(int i) {
        StringBuilder sb = new StringBuilder();
        for (Map.Entry<Integer, Integer> entry : this.s.mapping.entrySet()) {
            if (entry.getValue().intValue() == i) {
                if (sb.length() > 0) {
                    sb.append(" · ");
                }
                sb.append(physicalName(entry.getKey().intValue()));
            }
        }
        return sb.length() == 0 ? "연결 없음" : sb.toString();
    }

    AlertDialog.Builder builder(String str) {
        return new AlertDialog.Builder(this.style.context, this.style.themeId).setTitle(str);
    }

    void capture(final int i) {
        this.captureTarget = i;
        this.captureArmed = this.s.axesNeutral;
        ControlPatch.input.releaseAll();
        final PadDialog padDialog = new PadDialog(this.s.a);
        this.captureDialog = padDialog;
        padDialog.setTitle(String.valueOf(ControlData.NAMES[i]) + "에 연결");
        padDialog.setMessage("연결할 게임패드 버튼을 한 번 눌러 주세요.\n트리거·방향 패드도 지정할 수 있습니다.\n현재: " + assigned(i));
        padDialog.setButton(-2, "취소", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.28
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i2) {
            }
        });
        padDialog.setButton(-3, "이 키의 매핑 해제", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.29
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i2) {
                Iterator<Map.Entry<Integer, Integer>> it = ControlDialogs.this.s.mapping.entrySet().iterator();
                while (it.hasNext()) {
                    if (it.next().getValue().intValue() == i) {
                        it.remove();
                    }
                }
                ControlDialogs.this.s.savePads();
                ControlDialogs.this.refreshPadRows();
            }
        });
        track(padDialog, new Runnable() { // from class: com.jjongjjongs.minimobile.ControlDialogs.30
            @Override // java.lang.Runnable
            public void run() {
                if (ControlDialogs.this.captureDialog == padDialog) {
                    ControlDialogs.this.captureDialog = null;
                    ControlDialogs.this.captureTarget = -1;
                }
                ControlDialogs.this.refreshPadRows();
            }
        });
    }

    boolean captureKey(KeyEvent keyEvent) {
        if (this.captureTarget < 0) {
            return false;
        }
        if (keyEvent.getAction() != 0 || keyEvent.getRepeatCount() != 0) {
            return true;
        }
        assign(keyEvent.getKeyCode());
        return true;
    }

    boolean captureMotion(MotionEvent motionEvent, float f, float f2, float f3, float f4) {
        int i;
        if (this.captureTarget < 0) {
            return false;
        }
        if (!this.captureArmed) {
            if (this.s.axesNeutral) {
                this.captureArmed = true;
            }
            return true;
        }
        if (f < -0.65f) {
            i = 21;
        } else if (f > 0.65f) {
            i = 22;
        } else if (f2 < -0.65f) {
            i = 19;
        } else if (f2 > 0.65f) {
            i = 20;
        } else {
            if (f3 <= 0.65f) {
                if (f4 > 0.65f) {
                    i = 105;
                }
                return true;
            }
            i = 104;
        }
        assign(i);
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
            TextView text = this.style.text(strArr[i3], 13.0f, ControlStyle.MUTED);
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
                        ControlDialogs.this.s.mapping.clear();
                        ControlDialogs.this.s.mapping.putAll(ControlDialogs.this.s.padSlots.get(str));
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

    void mainMenu() {
        ControlStyle controlStyle;
        String str;
        String[] strArr = {"키패드 위치·크기 편집", "버튼별 표시·숨김", "버튼별 연사 ON/OFF", "현재 배치 저장", "저장된 배치 불러오기·삭제", "배치 파일로 백업", "배치 파일에서 복원", "현재 배치 초기화", "게임패드 매핑"};
        LinearLayout column = column();
        column.addView(this.style.hint("설정을 변경하는 동안 게임을 일시정지합니다."));
        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        LinearLayout linearLayout = null;
        final AlertDialog create = builder("조작 설정").setView(scrollView).setPositiveButton("게임으로 돌아가기", (DialogInterface.OnClickListener) null).create();
        for (final int i = 0; i < 9; i++) {
            if (i == 0) {
                controlStyle = this.style;
                str = "키패드";
            } else if (i == 3) {
                controlStyle = this.style;
                str = "배치 저장·백업·초기화";
            } else if (i == 8) {
                controlStyle = this.style;
                str = "게임패드";
            } else {
                this.style.menuRow(linearLayout, strArr[i], new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.2
                    @Override // android.view.View.OnClickListener
                    public void onClick(View view) {
                        ControlDialogs controlDialogs;
                        boolean z = false;
                        switch (i) {
                            case 0:
                                ControlDialogs.this.s.editor.start();
                                break;
                            case 1:
                                ControlDialogs.this.visibility();
                                break;
                            case 2:
                                ControlDialogs.this.rapidSettings();
                                break;
                            case 3:
                                ControlDialogs.this.saveName(false);
                                break;
                            case 4:
                                ControlDialogs.this.loadSlots(false);
                                break;
                            case 5:
                                controlDialogs = ControlDialogs.this;
                                z = true;
                                controlDialogs.pickFile(z);
                                break;
                            case 6:
                                controlDialogs = ControlDialogs.this;
                                controlDialogs.pickFile(z);
                                break;
                            case 7:
                                ControlDialogs.this.resetLayout();
                                break;
                            case ControlGrid.DEFAULT_DP /* 8 */:
                                ControlDialogs.this.padMenu();
                                break;
                        }
                        create.dismiss();
                    }
                });
            }
            linearLayout = controlStyle.section(column, str);
            this.style.menuRow(linearLayout, strArr[i], new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.2
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlDialogs controlDialogs;
                    boolean z = false;
                    switch (i) {
                        case 0:
                            ControlDialogs.this.s.editor.start();
                            break;
                        case 1:
                            ControlDialogs.this.visibility();
                            break;
                        case 2:
                            ControlDialogs.this.rapidSettings();
                            break;
                        case 3:
                            ControlDialogs.this.saveName(false);
                            break;
                        case 4:
                            ControlDialogs.this.loadSlots(false);
                            break;
                        case 5:
                            controlDialogs = ControlDialogs.this;
                            z = true;
                            controlDialogs.pickFile(z);
                            break;
                        case 6:
                            controlDialogs = ControlDialogs.this;
                            controlDialogs.pickFile(z);
                            break;
                        case 7:
                            ControlDialogs.this.resetLayout();
                            break;
                        case ControlGrid.DEFAULT_DP /* 8 */:
                            ControlDialogs.this.padMenu();
                            break;
                    }
                    create.dismiss();
                }
            });
        }
        track(create);
    }

    EditText nameField() {
        EditText input = this.style.input();
        input.setSingleLine(true);
        input.setHint("배치 이름");
        input.setFilters(new InputFilter[]{new InputFilter.LengthFilter(40)});
        return input;
    }

    void padMenu() {
        ControlStyle controlStyle;
        String str;
        LinearLayout column = column();
        column.addView(this.style.hint("게임 키를 고른 뒤 연결할 패드 버튼을 누르세요.\n변경은 바로 저장됩니다. 왼쪽 스틱은 방향키로 동작합니다."));
        this.padRows = new Button[21];
        LinearLayout linearLayout = null;
        for (int i = 0; i < ControlData.ORDER.length; i++) {
            if (i == 0) {
                controlStyle = this.style;
                str = "방향·확인";
            } else if (i == 5) {
                controlStyle = this.style;
                str = "기능";
            } else if (i == 9) {
                controlStyle = this.style;
                str = "숫자·기호";
            } else {
                final int i2 = ControlData.ORDER[i];
                Button mappingRow = this.style.mappingRow();
                mappingRow.setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.23
                    @Override // android.view.View.OnClickListener
                    public void onClick(View view) {
                        ControlDialogs.this.capture(i2);
                    }
                });
                this.padRows[i2] = mappingRow;
                this.style.divider(linearLayout);
                linearLayout.addView(mappingRow, new LinearLayout.LayoutParams(-1, -2));
            }
            linearLayout = controlStyle.section(column, str);
            final int i22 = ControlData.ORDER[i];
            Button mappingRow2 = this.style.mappingRow();
            mappingRow2.setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.23
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlDialogs.this.capture(i22);
                }
            });
            this.padRows[i22] = mappingRow2;
            this.style.divider(linearLayout);
            linearLayout.addView(mappingRow2, new LinearLayout.LayoutParams(-1, -2));
        }
        refreshPadRows();
        ScrollView scrollView = new ScrollView(this.style.context);
        scrollView.addView(column);
        PadDialog padDialog = new PadDialog(this.s.a);
        padDialog.setTitle("게임패드 매핑");
        padDialog.setView(scrollView);
        padDialog.setButton(-2, "닫기", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.24
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
            }
        });
        padDialog.setButton(-3, "프리셋", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.25
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
                ControlDialogs.this.padPresets();
            }
        });
        padDialog.setButton(-1, "기본 매핑", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.26
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
                ControlDialogs.this.track(ControlDialogs.this.builder("기본 매핑 복원").setMessage("위·아래·왼쪽·오른쪽만 연결하고 나머지는 연결 없음으로 바꿀까요? 저장한 프리셋은 유지됩니다.").setPositiveButton("복원", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.26.1
                    @Override // android.content.DialogInterface.OnClickListener
                    public void onClick(DialogInterface dialogInterface2, int i4) {
                        ControlDialogs.this.s.mapping.clear();
                        ControlDialogs.this.s.mapping.putAll(ControlPads.defaults());
                        ControlDialogs.this.s.savePads();
                        ControlDialogs.this.refreshPadRows();
                    }
                }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
            }
        });
        track(padDialog);
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
        boolean[] zArr = new boolean[21];
        String[] strArr = new String[21];
        for (int i = 0; i < 21; i++) {
            int i2 = ControlData.ORDER[i];
            strArr[i] = ControlData.NAMES[i2];
            zArr[i] = copy.enabled[i2];
        }
        final AlertDialog create = builder(rapidTitle(copy)).setMultiChoiceItems(strArr, zArr, new DialogInterface.OnMultiChoiceClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.3
            @Override // android.content.DialogInterface.OnMultiChoiceClickListener
            public void onClick(DialogInterface dialogInterface, int i3, boolean z) {
                copy.enabled[ControlData.ORDER[i3]] = z;
            }
        }).setPositiveButton("적용", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.4
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
                ControlPatch.input.releaseAll();
                ControlDialogs.this.s.data.rapid = copy.copy();
                ControlDialogs.this.s.saveLayout();
            }
        }).setNeutralButton("연사 속도", (DialogInterface.OnClickListener) null).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create();
        track(create);
        create.getButton(-3).setOnClickListener(new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.5
            @Override // android.view.View.OnClickListener
            public void onClick(View view) {
                ControlDialogs.this.rapidSpeed(copy, create);
            }
        });
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

    void refreshPadRows() {
        if (this.padRows != null) {
            for (int i = 0; i < this.padRows.length; i++) {
                if (this.padRows[i] != null) {
                    this.style.mappingText(this.padRows[i], ControlData.NAMES[i], assigned(i));
                }
            }
        }
    }

    void resetLayout() {
        final boolean landscape = this.s.editor.landscape();
        track(builder("현재 배치 초기화").setMessage(String.valueOf(landscape ? "가로" : "세로") + " 버튼의 위치·크기·숨김과 격자 간격을 기본값으로 되돌립니다. 저장된 배치는 유지됩니다.").setPositiveButton("초기화", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.11
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i) {
                if (landscape) {
                    ControlDialogs.this.s.data.landscape = new ControlData.Layout();
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
                                ControlDialogs.this.s.padSlots.put(name, new TreeMap<>((SortedMap) ControlDialogs.this.s.mapping));
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
        final boolean[] zArr = new boolean[21];
        String[] strArr = new String[21];
        for (int i = 0; i < 21; i++) {
            int i2 = ControlData.ORDER[i];
            zArr[i] = !layout.hidden[i2];
            strArr[i] = ControlData.NAMES[i2];
        }
        track(builder(String.valueOf(this.s.editor.landscape() ? "가로" : "세로") + " · 표시할 버튼 선택").setMultiChoiceItems(strArr, zArr, new DialogInterface.OnMultiChoiceClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.8
            @Override // android.content.DialogInterface.OnMultiChoiceClickListener
            public void onClick(DialogInterface dialogInterface, int i3, boolean z) {
                zArr[i3] = z;
            }
        }).setPositiveButton("적용", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.9
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
                ControlPatch.input.releaseAll();
                for (int i4 = 0; i4 < zArr.length; i4++) {
                    layout.hidden[ControlData.ORDER[i4]] = !zArr[i4];
                }
                ControlDialogs.this.s.saveLayout();
                ControlDialogs.this.s.editor.refresh();
            }
        }).setNeutralButton("모두 표시", new DialogInterface.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlDialogs.10
            @Override // android.content.DialogInterface.OnClickListener
            public void onClick(DialogInterface dialogInterface, int i3) {
                Arrays.fill(layout.hidden, false);
                ControlDialogs.this.s.saveLayout();
                ControlDialogs.this.s.editor.refresh();
            }
        }).setNegativeButton("취소", (DialogInterface.OnClickListener) null).create());
    }
}
