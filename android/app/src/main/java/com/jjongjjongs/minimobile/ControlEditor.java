package com.jjongjjongs.minimobile;

import android.R;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewTreeObserver;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import com.jjongjjongs.minimobile.ControlData;
import com.jjongjjongs.minimobile.ControlHit;
import com.jjongjjongs.minimobile.ControlPatch;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.IdentityHashMap;
import java.util.Iterator;
import java.util.List;

final class ControlEditor {
    ControlData beforeEdit;
    float[] dragBeforeRect;
    boolean dragChanged;
    ControlData.Layout dragLayout;
    RectF dragStart;
    boolean dragging;
    boolean editing;
    Button gridButton;
    boolean resizing;
    final ControlPatch.Session s;
    float startX;
    float startY;
    TextView title;
    LinearLayout toolbar;
    ViewTreeObserver.OnGlobalLayoutListener toolbarLayoutListener;
    ViewTreeObserver toolbarObserver;
    View view;
    final List<KeyRef> keys = new ArrayList();
    final IdentityHashMap<Object, KeyRef> refs = new IdentityHashMap<>();
    final KeyRef[] byCode = new KeyRef[21];
    int selected = -1;
    int pointer = -1;
    final Paint paint = new Paint(1);
    final Path gridPath = new Path();
    final Path overlapPath = new Path();

    static final class KeyRef implements ControlHit.Region {
        RectF bounds;
        int code;
        RectF defaults;
        Object object;

        KeyRef() {
        }

        @Override // com.jjongjjongs.minimobile.ControlHit.Region
        public int code() {
            return this.code;
        }

        @Override // com.jjongjjongs.minimobile.ControlHit.Region
        public boolean contains(float f, float f2) {
            return this.bounds.contains(f, f2);
        }
    }

    ControlEditor(ControlPatch.Session session) {
        this.s = session;
    }

    void afterPlace(View view, int i) {
        bind(view);
        if (i < 0 || i >= this.keys.size()) {
            return;
        }
        KeyRef keyRef = this.keys.get(i);
        keyRef.defaults.set(keyRef.bounds);
        apply(keyRef);
    }

    void apply(KeyRef keyRef) {
        if (this.view == null || this.view.getWidth() == 0 || this.view.getHeight() == 0) {
            return;
        }
        float[] fArr = layout().rect[keyRef.code];
        if (fArr == null) {
            keyRef.bounds.set(keyRef.defaults);
        } else {
            keyRef.bounds.set(fArr[0] * this.view.getWidth(), fArr[1] * this.view.getHeight(), (fArr[0] + fArr[2]) * this.view.getWidth(), (fArr[1] + fArr[3]) * this.view.getHeight());
        }
        ControlPatch.call(keyRef.object, "shade");
    }

    void attachToolbar() {
        final ViewGroup viewGroup;
        if (this.editing && (viewGroup = (ViewGroup) this.s.a.findViewById(R.id.content)) != null) {
            if (this.toolbar != null && this.toolbar.getParent() == viewGroup) {
                updateTitle();
                return;
            }
            detachToolbar();
            this.toolbar = new LinearLayout(this.s.ui.style.context);
            this.toolbar.setOrientation(1);
            this.toolbar.setBackgroundColor(this.s.ui.style.BG);
            this.toolbar.setPadding(ControlPatch.dp(this.s.a, 8.0f), ControlPatch.dp(this.s.a, 6.0f), ControlPatch.dp(this.s.a, 8.0f), ControlPatch.dp(this.s.a, 8.0f));
            this.title = this.s.ui.style.text("", 12.0f, this.s.ui.style.MUTED);
            this.title.setPadding(ControlPatch.dp(this.s.a, 4.0f), ControlPatch.dp(this.s.a, 6.0f), ControlPatch.dp(this.s.a, 4.0f), 0);
            LinearLayout linearLayout = new LinearLayout(this.s.a);
            linearLayout.setGravity(16);
            View.OnClickListener[] onClickListenerArr = {new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.1
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.finish(true);
                }
            }, new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.2
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.finish(false);
                }
            }, new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.3
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.s.ui.editNumbers();
                }
            }, new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.4
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.hideSelected();
                }
            }, new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.5
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.s.ui.visibility();
                }
            }, new View.OnClickListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.6
                @Override // android.view.View.OnClickListener
                public void onClick(View view) {
                    ControlEditor.this.s.ui.gridSettings();
                }
            }};
            String[] strArr = {"완료", "취소", "수치", "숨김", "목록", "격자"};
            for (int i = 0; i < 6; i++) {
                Button olVar = tool(strArr[i], onClickListenerArr[i]);
                if (i == 5) {
                    this.gridButton = olVar;
                }
                LinearLayout.LayoutParams layoutParams = new LinearLayout.LayoutParams(0, -2, 1.0f);
                if (i > 0) {
                    layoutParams.leftMargin = ControlPatch.dp(this.s.a, 4.0f);
                }
                linearLayout.addView(olVar, layoutParams);
            }
            this.toolbar.addView(linearLayout, new LinearLayout.LayoutParams(-1, -2));
            this.toolbar.addView(this.title, new LinearLayout.LayoutParams(-1, -2));
            final View childAt = viewGroup.getChildCount() > 0 ? viewGroup.getChildAt(0) : null;
            viewGroup.addView(this.toolbar, new FrameLayout.LayoutParams(-1, -2, 48));
            this.toolbar.bringToFront();
            if (childAt != null) {
                this.toolbarObserver = viewGroup.getViewTreeObserver();
                this.toolbarLayoutListener = new ViewTreeObserver.OnGlobalLayoutListener() { // from class: com.jjongjjongs.minimobile.ControlEditor.7
                    @Override // android.view.ViewTreeObserver.OnGlobalLayoutListener
                    public void onGlobalLayout() {
                        ControlEditor.this.positionToolbar(viewGroup, childAt);
                    }
                };
                this.toolbarObserver.addOnGlobalLayoutListener(this.toolbarLayoutListener);
                positionToolbar(viewGroup, childAt);
            }
            updateTitle();
        }
    }

    void bind(View view) {
        if (this.view == view) {
            return;
        }
        this.view = view;
        this.keys.clear();
        this.refs.clear();
        Arrays.fill(this.byCode, (Object) null);
        for (Object obj : (List) ControlPatch.field(view, "keys")) {
            KeyRef keyRef = new KeyRef();
            keyRef.object = obj;
            keyRef.code = ((Integer) ControlPatch.field(obj, "code")).intValue();
            keyRef.bounds = (RectF) ControlPatch.field(obj, "bounds");
            keyRef.defaults = new RectF(keyRef.bounds);
            this.keys.add(keyRef);
            this.refs.put(obj, keyRef);
            if (keyRef.code >= 0 && keyRef.code < this.byCode.length) {
                this.byCode[keyRef.code] = keyRef;
            }
        }
    }

    void detachToolbar() {
        if (this.toolbarObserver != null && this.toolbarObserver.isAlive() && this.toolbarLayoutListener != null) {
            this.toolbarObserver.removeOnGlobalLayoutListener(this.toolbarLayoutListener);
        }
        this.toolbarObserver = null;
        this.toolbarLayoutListener = null;
        if (this.toolbar != null && (this.toolbar.getParent() instanceof ViewGroup)) {
            ((ViewGroup) this.toolbar.getParent()).removeView(this.toolbar);
        }
        this.toolbar = null;
        this.title = null;
        this.gridButton = null;
    }

    void draw(Canvas canvas) {
        if (this.editing) {
            this.overlapPath.reset();
            for (int i = 0; i < this.keys.size(); i++) {
                KeyRef keyRef = this.keys.get(i);
                if (!layout().hidden[keyRef.code]) {
                    for (int i2 = i + 1; i2 < this.keys.size(); i2++) {
                        KeyRef keyRef2 = this.keys.get(i2);
                        if (!layout().hidden[keyRef2.code]) {
                            float max = Math.max(keyRef.bounds.left, keyRef2.bounds.left);
                            float max2 = Math.max(keyRef.bounds.top, keyRef2.bounds.top);
                            float min = Math.min(keyRef.bounds.right, keyRef2.bounds.right);
                            float min2 = Math.min(keyRef.bounds.bottom, keyRef2.bounds.bottom);
                            if (max < min && max2 < min2) {
                                this.overlapPath.addRect(max, max2, min, min2, Path.Direction.CW);
                            }
                        }
                    }
                }
            }
            this.paint.setShader(null);
            this.paint.setStyle(Paint.Style.FILL);
            this.paint.setColor(Color.argb(85, 255, 82, 82));
            canvas.drawPath(this.overlapPath, this.paint);
            this.paint.setShader(null);
            this.paint.setStyle(Paint.Style.STROKE);
            this.paint.setStrokeWidth(ControlPatch.dp(this.s.a, 1.0f));
            this.paint.setColor(Color.rgb(91, 132, 111));
            for (KeyRef keyRef3 : this.keys) {
                if (!layout().hidden[keyRef3.code]) {
                    canvas.drawRoundRect(keyRef3.bounds, ControlPatch.dp(this.s.a, 7.0f), ControlPatch.dp(this.s.a, 7.0f), this.paint);
                }
            }
            if (this.selected >= 0 && this.byCode[this.selected] != null && !layout().hidden[this.selected]) {
                RectF rectF = this.byCode[this.selected].bounds;
                this.paint.setColor(Color.rgb(124, 245, 164));
                this.paint.setStrokeWidth(ControlPatch.dp(this.s.a, 3.0f));
                canvas.drawRoundRect(rectF, ControlPatch.dp(this.s.a, 7.0f), ControlPatch.dp(this.s.a, 7.0f), this.paint);
                float min3 = Math.min(ControlPatch.dp(this.s.a, 16.0f), Math.min(rectF.width(), rectF.height()) * 0.35f);
                this.paint.setStyle(Paint.Style.FILL);
                canvas.drawRect(rectF.right - min3, rectF.bottom - min3, rectF.right, rectF.bottom, this.paint);
            }
            this.paint.setStyle(Paint.Style.FILL);
        }
    }

    void drawGrid(Canvas canvas) {
        if (!this.editing || this.view == null) {
            return;
        }
        float gridStep = gridStep();
        int floor = (int) Math.floor(this.view.getWidth() / gridStep);
        int floor2 = (int) Math.floor(this.view.getHeight() / gridStep);
        this.gridPath.reset();
        this.paint.setShader(null);
        this.paint.setStyle(Paint.Style.STROKE);
        this.paint.setColor(Color.argb(105, 133, 198, 169));
        this.paint.setStrokeWidth(1.0f);
        for (int i = 0; i <= floor; i++) {
            float f = i * gridStep;
            if (f >= this.view.getWidth()) {
                break;
            }
            this.gridPath.moveTo(f, 0.0f);
            this.gridPath.lineTo(f, this.view.getHeight());
        }
        for (int i2 = 0; i2 <= floor2; i2++) {
            float f2 = i2 * gridStep;
            if (f2 >= this.view.getHeight()) {
                break;
            }
            this.gridPath.moveTo(0.0f, f2);
            this.gridPath.lineTo(this.view.getWidth(), f2);
        }
        this.gridPath.moveTo(this.view.getWidth(), 0.0f);
        this.gridPath.lineTo(this.view.getWidth(), this.view.getHeight());
        this.gridPath.moveTo(0.0f, this.view.getHeight());
        this.gridPath.lineTo(this.view.getWidth(), this.view.getHeight());
        canvas.drawPath(this.gridPath, this.paint);
    }

    void finish(boolean z) {
        if (this.editing) {
            this.dragging = false;
            this.pointer = -1;
            this.editing = false;
            if (!z && this.beforeEdit != null) {
                this.s.data.applyActive(this.beforeEdit);
            }
            this.beforeEdit = null;
            this.s.saveLayout();
            detachToolbar();
            refresh();
            this.s.syncPause();
        }
    }

    KeyRef gameHit(float f, float f2) {
        return (KeyRef) ControlHit.pick(this.keys, layout().hidden, f, f2, false);
    }

    float gridStep() {
        return layout().gridDp * this.s.a.getResources().getDisplayMetrics().density;
    }

    boolean hidden(Object obj) {
        KeyRef keyRef = this.refs.get(obj);
        return keyRef != null && layout().hidden[keyRef.code];
    }

    void hideSelected() {
        if (this.selected < 0) {
            ControlPatch.toast(this.s.a, "숨길 버튼을 먼저 선택해 주세요.");
            return;
        }
        layout().hidden[this.selected] = true;
        this.selected = -1;
        this.s.saveLayout();
        refresh();
    }

    KeyRef hit(float f, float f2) {
        return (KeyRef) ControlHit.pick(this.keys, layout().hidden, f, f2, true);
    }

    boolean landscape() {
        return this.view != null && ControlPatch.flag(this.view, "landscape");
    }

    ControlData.Layout layout() {
        return this.s.data.layout(landscape());
    }

    void positionToolbar(ViewGroup viewGroup, View view) {
        if (this.toolbar == null || this.toolbar.getParent() != viewGroup || view.getParent() != viewGroup || view.getWidth() <= 0) {
            return;
        }
        int max = Math.max(0, view.getLeft()) + view.getPaddingLeft();
        int max2 = Math.max(0, view.getTop()) + view.getPaddingTop();
        int max3 = Math.max(0, viewGroup.getWidth() - view.getRight()) + view.getPaddingRight();
        FrameLayout.LayoutParams layoutParams = (FrameLayout.LayoutParams) this.toolbar.getLayoutParams();
        if (layoutParams.leftMargin == max && layoutParams.topMargin == max2 && layoutParams.rightMargin == max3) {
            return;
        }
        layoutParams.leftMargin = max;
        layoutParams.topMargin = max2;
        layoutParams.rightMargin = max3;
        this.toolbar.setLayoutParams(layoutParams);
    }

    void refresh() {
        Iterator<KeyRef> it = this.keys.iterator();
        while (it.hasNext()) {
            apply(it.next());
        }
        if (this.view != null) {
            this.view.invalidate();
        }
        updateTitle();
    }

    void saveRect(KeyRef keyRef) {
        if (this.view == null || this.view.getWidth() <= 0 || this.view.getHeight() <= 0) {
            return;
        }
        RectF rectF = keyRef.bounds;
        float width = this.view.getWidth();
        float height = this.view.getHeight();
        layout().rect[keyRef.code] = new float[]{rectF.left / width, rectF.top / height, rectF.width() / width, rectF.height() / height};
        ControlPatch.call(keyRef.object, "shade");
        this.view.invalidate();
        updateTitle();
    }

    void select(int i) {
        this.selected = i;
        if (this.view != null) {
            this.view.invalidate();
        }
        updateTitle();
    }

    void setRect(int i, float f, float f2, float f3, float f4) {
        setRect(i, f, f2, f3, f4, false);
    }

    void setRect(int i, float f, float f2, float f3, float f4, boolean z) {
        KeyRef keyRef = this.byCode[i];
        if (keyRef != null && this.view.getWidth() > 0 && this.view.getHeight() > 0) {
            float[] snap = ControlGrid.snap(f, f2, f3, f4, this.view.getWidth(), this.view.getHeight(), gridStep(), this.s.a.getResources().getDisplayMetrics().density * 24.0f, z);
            keyRef.bounds.set(snap[0], snap[1], snap[0] + snap[2], snap[1] + snap[3]);
            saveRect(keyRef);
        }
    }

    void snapAll() {
        for (KeyRef keyRef : this.keys) {
            RectF rectF = new RectF(keyRef.bounds);
            setRect(keyRef.code, rectF.left, rectF.top, rectF.width(), rectF.height());
        }
        this.s.saveLayout();
        refresh();
    }

    void start() {
        View view = (View) ControlPatch.field(this.s.a, "keypad");
        if (view == null) {
            ControlPatch.toast(this.s.a, "게임을 실행한 뒤 키패드를 편집해 주세요.");
            return;
        }
        bind(view);
        if (!this.editing) {
            this.beforeEdit = this.s.data.activeCopy();
            this.editing = true;
            this.selected = -1;
        }
        ControlPatch.input.releaseAll();
        this.s.syncPause();
        attachToolbar();
        this.view.invalidate();
    }

    Button tool(String str, View.OnClickListener onClickListener) {
        Button button = this.s.ui.style.button(str, "완료".equals(str));
        button.setTextSize(12.0f);
        button.setPadding(ControlPatch.dp(this.s.a, 4.0f), ControlPatch.dp(this.s.a, 6.0f), ControlPatch.dp(this.s.a, 4.0f), ControlPatch.dp(this.s.a, 6.0f));
        button.setMinHeight(ControlPatch.dp(this.s.a, 44.0f));
        button.setMinimumHeight(ControlPatch.dp(this.s.a, 44.0f));
        button.setOnClickListener(onClickListener);
        return button;
    }

    /* JADX WARN: Can't fix incorrect switch cases order, some code will duplicate */
    boolean touch(MotionEvent motionEvent) {
        if (this.s.dialogs.isEmpty() && !this.s.fileBusy) {
            boolean z = false;
            switch (motionEvent.getActionMasked()) {
                case 0:
                    KeyRef hit = hit(motionEvent.getX(), motionEvent.getY());
                    if (hit != null) {
                        select(hit.code);
                        this.pointer = motionEvent.getPointerId(0);
                        this.startX = motionEvent.getX();
                        this.startY = motionEvent.getY();
                        this.dragStart = new RectF(hit.bounds);
                        this.dragging = true;
                        this.dragChanged = false;
                        this.dragLayout = layout();
                        float[] fArr = this.dragLayout.rect[hit.code];
                        this.dragBeforeRect = fArr != null ? (float[]) fArr.clone() : null;
                        float min = Math.min(ControlPatch.dp(this.s.a, 22.0f), Math.min(hit.bounds.width(), hit.bounds.height()) * 0.42f);
                        if (motionEvent.getX() >= hit.bounds.right - min && motionEvent.getY() >= hit.bounds.bottom - min) {
                            z = true;
                        }
                        this.resizing = z;
                        break;
                    } else {
                        select(-1);
                        break;
                    }
                case 1:
                    this.dragging = false;
                    this.pointer = -1;
                    this.s.saveLayout();
                    break;
                case 2:
                    int findPointerIndex = motionEvent.findPointerIndex(this.pointer);
                    if (this.dragging && this.selected >= 0 && findPointerIndex >= 0) {
                        float x = motionEvent.getX(findPointerIndex) - this.startX;
                        float y = motionEvent.getY(findPointerIndex) - this.startY;
                        if (this.dragChanged || Math.max(Math.abs(x), Math.abs(y)) >= ControlPatch.dp(this.s.a, 2.0f)) {
                            this.dragChanged = true;
                            if (!this.resizing) {
                                setRect(this.selected, x + this.dragStart.left, this.dragStart.top + y, this.dragStart.width(), this.dragStart.height());
                                break;
                            } else {
                                setRect(this.selected, this.dragStart.left, this.dragStart.top, this.dragStart.width() + x, this.dragStart.height() + y, true);
                                break;
                            }
                        }
                    }
                    break;
                case 3:
                    if (this.dragging && this.selected >= 0 && this.dragLayout != null) {
                        this.dragLayout.rect[this.selected] = this.dragBeforeRect != null ? (float[]) this.dragBeforeRect.clone() : null;
                        refresh();
                    }
                    this.dragging = false;
                    this.pointer = -1;
                    break;
                case 6:
                    if (motionEvent.getPointerId(motionEvent.getActionIndex()) != this.pointer) {
                    }
                    this.dragging = false;
                    this.pointer = -1;
                    this.s.saveLayout();
                    break;
            }
            return true;
        }
        return true;
    }

    void updateTitle() {
        if (this.gridButton != null) {
            this.gridButton.setText("격자\n" + layout().gridDp + "dp");
        }
        if (this.title == null) {
            return;
        }
        String str = String.valueOf(landscape() ? "가로" : "세로") + " · " + layout().gridDp + "dp 자동맞춤 · 이동: 끌기 / 크기: 우하단 ■";
        if (this.selected >= 0 && this.byCode[this.selected] != null) {
            RectF rectF = this.byCode[this.selected].bounds;
            float f = this.s.a.getResources().getDisplayMetrics().density;
            str = String.valueOf(ControlData.NAMES[this.selected]) + " · X " + Math.round(rectF.left / f) + " Y " + Math.round(rectF.top / f) + " · " + Math.round(rectF.width() / f) + " × " + Math.round(rectF.height() / f) + " dp";
        }
        this.title.setText(str);
    }
}
