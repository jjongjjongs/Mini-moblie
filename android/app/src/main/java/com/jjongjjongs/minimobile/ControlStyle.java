package com.jjongjjongs.minimobile;

import android.R;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.StateListDrawable;
import android.text.SpannableString;
import android.text.style.ForegroundColorSpan;
import android.text.style.RelativeSizeSpan;
import android.text.style.StyleSpan;
import android.view.ContextThemeWrapper;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ListView;
import android.widget.TextView;

final class ControlStyle {
    final Context context;
    final int themeId;
    static final int BG = Color.rgb(255, 255, 255);
    static final int INK = Color.rgb(26, 42, 32);
    static final int MUTED = Color.rgb(100, 117, 104);
    static final int LINE = Color.rgb(233, 241, 235);
    static final int DIVIDER = Color.rgb(238, 243, 239);
    static final int GREEN = Color.rgb(46, 139, 87);
    static final int DEEP = Color.rgb(34, 114, 71);
    static final int SOFT = Color.rgb(220, 242, 226);
    static final int SOFT_LINE = Color.rgb(199, 232, 209);
    static final int SOFTER = Color.rgb(238, 248, 241);

    ControlStyle(Activity activity) {
        this.themeId = theme(activity);
        this.context = new ContextThemeWrapper(activity, this.themeId);
    }

    static void playerButton(Button button) {
        button.setAllCaps(false);
        button.setTextColor(Color.rgb(182, 194, 216));
        GradientDrawable gradientDrawable = new GradientDrawable(GradientDrawable.Orientation.TOP_BOTTOM, new int[]{Color.rgb(46, 57, 84), Color.rgb(38, 48, 72)});
        gradientDrawable.setCornerRadius(ControlPatch.dp(button.getContext(), 8.0f));
        gradientDrawable.setStroke(Math.max(1, ControlPatch.dp(button.getContext(), 0.8f)), Color.rgb(24, 32, 52));
        button.setBackground(gradientDrawable);
    }

    static int theme(Context context) {
        int identifier = context.getResources().getIdentifier("MiniControlsDialogTheme", "style", context.getPackageName());
        if (identifier != 0) {
            return identifier;
        }
        throw new IllegalStateException("Missing controls dialog theme");
    }

    Button button(String str, boolean z) {
        Button button = new Button(this.context);
        button.setText(str);
        button(button, z);
        return button;
    }

    void button(Button button, boolean z) {
        button.setAllCaps(false);
        button.setTextSize(13.0f);
        button.setTypeface(Typeface.DEFAULT_BOLD);
        button.setTextColor(z ? BG : DEEP);
        button.setBackground(buttonBackground(z, false));
        button.setPadding(dp(10.0f), dp(6.0f), dp(10.0f), dp(6.0f));
        button.setMinWidth(0);
        button.setMinimumWidth(0);
        button.setMinHeight(dp(42.0f));
        button.setMinimumHeight(dp(42.0f));
    }

    StateListDrawable buttonBackground(boolean z, boolean z2) {
        StateListDrawable stateListDrawable = new StateListDrawable();
        stateListDrawable.addState(new int[]{R.attr.state_pressed}, rounded(z ? DEEP : SOFT, z ? DEEP : SOFT_LINE, 1, 15));
        stateListDrawable.addState(new int[]{R.attr.state_focused}, rounded(z ? DEEP : SOFT, z ? DEEP : GREEN, 1, 15));
        stateListDrawable.addState(new int[0], rounded(z ? GREEN : z2 ? BG : SOFT, z ? GREEN : z2 ? LINE : SOFT_LINE, !z2 ? 1 : 0, 15));
        return stateListDrawable;
    }

    LinearLayout column() {
        LinearLayout linearLayout = new LinearLayout(this.context);
        linearLayout.setOrientation(1);
        linearLayout.setPadding(dp(20.0f), dp(8.0f), dp(20.0f), dp(12.0f));
        linearLayout.setBackgroundColor(BG);
        return linearLayout;
    }

    void decorate(AlertDialog alertDialog) {
        if (alertDialog.getWindow() != null) {
            alertDialog.getWindow().setBackgroundDrawable(rounded(BG, LINE, 1, 16));
        }
        int identifier = this.context.getResources().getIdentifier("alertTitle", "id", "android");
        View findViewById = identifier == 0 ? null : alertDialog.findViewById(identifier);
        if (findViewById instanceof TextView) {
            TextView textView = (TextView) findViewById;
            textView.setTextColor(INK);
            textView.setTextSize(18.0f);
            textView.setTypeface(Typeface.DEFAULT_BOLD);
        }
        View findViewById2 = alertDialog.findViewById(R.id.message);
        if (findViewById2 instanceof TextView) {
            TextView textView2 = (TextView) findViewById2;
            textView2.setTextColor(MUTED);
            textView2.setTextSize(14.0f);
        }
        ListView listView = alertDialog.getListView();
        if (listView != null) {
            listView.setBackgroundColor(BG);
            listView.setDivider(rounded(DIVIDER, DIVIDER, 0, 0));
            listView.setDividerHeight(Math.max(1, dp(0.5f)));
        }
        int[] iArr = {-1, -2, -3};
        for (int i = 0; i < 3; i++) {
            int i2 = iArr[i];
            Button button = alertDialog.getButton(i2);
            if (button != null) {
                button(button, i2 == -1);
                ViewGroup.LayoutParams layoutParams = button.getLayoutParams();
                if (layoutParams instanceof ViewGroup.MarginLayoutParams) {
                    ViewGroup.MarginLayoutParams marginLayoutParams = (ViewGroup.MarginLayoutParams) layoutParams;
                    marginLayoutParams.leftMargin = dp(4.0f);
                    marginLayoutParams.rightMargin = dp(4.0f);
                    button.setLayoutParams(marginLayoutParams);
                }
            }
        }
    }

    void divider(LinearLayout linearLayout) {
        if (linearLayout.getChildCount() == 0) {
            return;
        }
        View view = new View(this.context);
        view.setBackgroundColor(DIVIDER);
        LinearLayout.LayoutParams layoutParams = new LinearLayout.LayoutParams(-1, Math.max(1, dp(0.5f)));
        layoutParams.leftMargin = dp(10.0f);
        layoutParams.rightMargin = dp(10.0f);
        linearLayout.addView(view, layoutParams);
    }

    int dp(float f) {
        return ControlPatch.dp(this.context, f);
    }

    TextView hint(String str) {
        TextView text = text(str, 13.0f, MUTED);
        text.setPadding(0, dp(4.0f), 0, dp(12.0f));
        return text;
    }

    EditText input() {
        EditText editText = new EditText(this.context);
        editText.setTextColor(INK);
        editText.setHintTextColor(MUTED);
        editText.setTextSize(15.0f);
        StateListDrawable stateListDrawable = new StateListDrawable();
        stateListDrawable.addState(new int[]{R.attr.state_focused}, rounded(BG, GREEN, 2, 13));
        stateListDrawable.addState(new int[0], rounded(BG, SOFT_LINE, 1, 13));
        editText.setBackground(stateListDrawable);
        editText.setPadding(dp(12.0f), dp(10.0f), dp(12.0f), dp(10.0f));
        editText.setMinHeight(dp(44.0f));
        return editText;
    }

    Button mappingRow() {
        Button button = button("", false);
        button.setTextSize(14.0f);
        button.setTypeface(Typeface.DEFAULT);
        button.setTextColor(INK);
        button.setBackground(buttonBackground(false, true));
        button.setGravity(19);
        button.setPadding(dp(12.0f), dp(9.0f), dp(12.0f), dp(9.0f));
        button.setMinHeight(dp(58.0f));
        button.setMinimumHeight(dp(58.0f));
        return button;
    }

    void mappingText(Button button, String str, String str2) {
        SpannableString spannableString = new SpannableString(String.valueOf(str) + "\n" + str2);
        int length = str.length() + 1;
        spannableString.setSpan(new StyleSpan(1), 0, str.length(), 33);
        spannableString.setSpan(new ForegroundColorSpan(MUTED), length, spannableString.length(), 33);
        spannableString.setSpan(new RelativeSizeSpan(0.86f), length, spannableString.length(), 33);
        button.setText(spannableString);
    }

    void menuRow(LinearLayout linearLayout, String str, View.OnClickListener onClickListener) {
        divider(linearLayout);
        LinearLayout linearLayout2 = new LinearLayout(this.context);
        linearLayout2.setGravity(16);
        linearLayout2.setPadding(dp(12.0f), dp(12.0f), dp(12.0f), dp(12.0f));
        linearLayout2.setMinimumHeight(dp(48.0f));
        linearLayout2.setBackground(buttonBackground(false, true));
        linearLayout2.addView(text(str, 14.0f, INK), new LinearLayout.LayoutParams(0, -2, 1.0f));
        TextView text = text("›", 22.0f, DEEP);
        text.setPadding(dp(10.0f), 0, 0, 0);
        linearLayout2.addView(text);
        linearLayout2.setFocusable(true);
        linearLayout2.setOnClickListener(onClickListener);
        linearLayout.addView(linearLayout2, new LinearLayout.LayoutParams(-1, -2));
    }

    GradientDrawable rounded(int i, int i2, int i3, int i4) {
        GradientDrawable gradientDrawable = new GradientDrawable();
        gradientDrawable.setColor(i);
        gradientDrawable.setCornerRadius(dp(i4));
        if (i3 > 0) {
            gradientDrawable.setStroke(dp(i3), i2);
        }
        return gradientDrawable;
    }

    LinearLayout section(LinearLayout linearLayout, String str) {
        TextView text = text(str, 12.5f, MUTED);
        text.setTypeface(Typeface.DEFAULT_BOLD);
        text.setPadding(dp(2.0f), dp(14.0f), 0, dp(8.0f));
        linearLayout.addView(text);
        LinearLayout linearLayout2 = new LinearLayout(this.context);
        linearLayout2.setOrientation(1);
        linearLayout2.setBackground(rounded(BG, LINE, 1, 14));
        linearLayout2.setPadding(dp(4.0f), dp(4.0f), dp(4.0f), dp(4.0f));
        linearLayout.addView(linearLayout2, new LinearLayout.LayoutParams(-1, -2));
        return linearLayout2;
    }

    TextView text(String str, float f, int i) {
        TextView textView = new TextView(this.context);
        textView.setText(str);
        textView.setTextSize(f);
        textView.setTextColor(i);
        return textView;
    }
}
