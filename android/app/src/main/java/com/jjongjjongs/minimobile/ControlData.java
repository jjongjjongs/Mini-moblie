package com.jjongjjongs.minimobile;

import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.Map;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

public final class ControlData {
    public static final int COUNT = 21;
    public static final int MAX_SLOTS = 32;
    public static final String[] NAMES = {"위 ▲", "아래 ▼", "왼쪽 ◀", "오른쪽 ▶", "확인 OK", "왼쪽 기능키 L", "오른쪽 기능키 R", "뒤로가기", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "별표 *", "샵 #", "저장"};
    public static final int[] ORDER = {0, 1, 2, 3, 4, 5, 6, 7, 20, 9, 10, 11, 12, 13, 14, 15, 16, 17, 8, 18, 19};
    public Layout portrait = new Layout();
    public Layout landscape = new Layout();
    public RapidSettings rapid = new RapidSettings();
    public final LinkedHashMap<String, ControlData> slots = new LinkedHashMap<>();

    public static final class Layout {
        public int gridDp = 8;
        public final float[][] rect = new float[21][];
        public final boolean[] hidden = new boolean[21];

        public static Layout parse(JSONObject jSONObject) throws JSONException {
            Layout layout = new Layout();
            if (jSONObject.has("grid_dp")) {
                Object obj = jSONObject.get("grid_dp");
                if (!(obj instanceof Number)) {
                    throw new JSONException("격자 간격은 숫자여야 합니다.");
                }
                Number number = (Number) obj;
                if (number.doubleValue() != number.intValue() || !ControlGrid.supported(number.intValue())) {
                    throw new JSONException("지원하지 않는 격자 간격입니다.");
                }
                layout.gridDp = number.intValue();
            }
            JSONObject jSONObject2 = jSONObject.getJSONObject("keys");
            if (jSONObject2.length() > 21) {
                throw new JSONException("버튼 수가 올바르지 않습니다.");
            }
            Iterator<String> keys = jSONObject2.keys();
            while (keys.hasNext()) {
                String next = keys.next();
                try {
                    int parseInt = Integer.parseInt(next);
                    if (parseInt < 0 || parseInt >= 21 || !next.equals(String.valueOf(parseInt))) {
                        throw new JSONException("잘못된 버튼 번호입니다.");
                    }
                    JSONObject jSONObject3 = jSONObject2.getJSONObject(next);
                    Object opt = jSONObject3.opt("hidden");
                    if (opt != null && !(opt instanceof Boolean)) {
                        throw new JSONException("잘못된 숨김 설정입니다.");
                    }
                    layout.hidden[parseInt] = jSONObject3.optBoolean("hidden", false);
                    if (jSONObject3.has("rect")) {
                        JSONArray jSONArray = jSONObject3.getJSONArray("rect");
                        if (jSONArray.length() != 4) {
                            throw new JSONException("버튼 좌표가 올바르지 않습니다.");
                        }
                        float[] fArr = new float[4];
                        for (int i = 0; i < 4; i++) {
                            Object obj2 = jSONArray.get(i);
                            if (!(obj2 instanceof Number)) {
                                throw new JSONException("버튼 좌표는 숫자여야 합니다.");
                            }
                            double doubleValue = ((Number) obj2).doubleValue();
                            if (Double.isNaN(doubleValue) || Double.isInfinite(doubleValue)) {
                                throw new JSONException("유효하지 않은 좌표입니다.");
                            }
                            fArr[i] = (float) doubleValue;
                        }
                        if (fArr[0] < 0.0f || fArr[1] < 0.0f || fArr[2] < 0.01f || fArr[3] < 0.01f || fArr[2] > 1.0f || fArr[3] > 1.0f || fArr[0] + fArr[2] > 1.001f || fArr[1] + fArr[3] > 1.001f) {
                            throw new JSONException("버튼이 키패드 영역을 벗어납니다.");
                        }
                        fArr[0] = Math.min(fArr[0], 1.0f - fArr[2]);
                        fArr[1] = Math.min(fArr[1], 1.0f - fArr[3]);
                        layout.rect[parseInt] = fArr;
                    }
                } catch (Exception e) {
                    throw new JSONException("잘못된 버튼 번호입니다.");
                }
            }
            return layout;
        }

        public Layout copy() {
            Layout layout = new Layout();
            layout.gridDp = this.gridDp;
            for (int i = 0; i < 21; i++) {
                layout.hidden[i] = this.hidden[i];
                layout.rect[i] = this.rect[i] == null ? null : (float[]) this.rect[i].clone();
            }
            return layout;
        }

        public JSONObject json() throws JSONException {
            JSONObject jSONObject = new JSONObject();
            for (int i = 0; i < 21; i++) {
                JSONObject jSONObject2 = new JSONObject();
                jSONObject2.put("hidden", this.hidden[i]);
                if (this.rect[i] != null) {
                    JSONArray jSONArray = new JSONArray();
                    float[] rectArr = this.rect[i];
                    int length = rectArr.length;
                    for (int i2 = 0; i2 < length; i2++) {
                        jSONArray.put(rectArr[i2]);
                    }
                    jSONObject2.put("rect", jSONArray);
                }
                jSONObject.put(String.valueOf(i), jSONObject2);
            }
            return new JSONObject().put("keys", jSONObject).put("grid_dp", this.gridDp);
        }
    }

    public static final class RapidSettings {
        public final boolean[] enabled = new boolean[21];
        public int periodMs = ControlRapid.DEFAULT_PERIOD_MS;

        public static RapidSettings parse(JSONObject jSONObject) throws JSONException {
            RapidSettings rapidSettings = new RapidSettings();
            Object obj = jSONObject.get("period_ms");
            if (!(obj instanceof Number)) {
                throw new JSONException("연사 간격은 숫자여야 합니다.");
            }
            Number number = (Number) obj;
            if (number.doubleValue() != number.intValue() || !ControlRapid.supported(number.intValue())) {
                throw new JSONException("지원하지 않는 연사 간격입니다.");
            }
            rapidSettings.periodMs = number.intValue();
            JSONObject jSONObject2 = jSONObject.getJSONObject("keys");
            if (jSONObject2.length() > 21) {
                throw new JSONException("연사 버튼 수가 올바르지 않습니다.");
            }
            Iterator<String> keys = jSONObject2.keys();
            while (keys.hasNext()) {
                String next = keys.next();
                try {
                    int parseInt = Integer.parseInt(next);
                    if (parseInt < 0 || parseInt >= 21 || !next.equals(String.valueOf(parseInt))) {
                        throw new JSONException("잘못된 연사 버튼 번호입니다.");
                    }
                    Object obj2 = jSONObject2.get(next);
                    if (!(obj2 instanceof Boolean)) {
                        throw new JSONException("잘못된 연사 ON/OFF 값입니다.");
                    }
                    rapidSettings.enabled[parseInt] = ((Boolean) obj2).booleanValue();
                } catch (Exception e) {
                    throw new JSONException("잘못된 연사 버튼 번호입니다.");
                }
            }
            return rapidSettings;
        }

        public RapidSettings copy() {
            RapidSettings rapidSettings = new RapidSettings();
            System.arraycopy(this.enabled, 0, rapidSettings.enabled, 0, 21);
            rapidSettings.periodMs = this.periodMs;
            return rapidSettings;
        }

        public JSONObject json() throws JSONException {
            JSONObject jSONObject = new JSONObject();
            for (int i = 0; i < 21; i++) {
                jSONObject.put(String.valueOf(i), this.enabled[i]);
            }
            return new JSONObject().put("keys", jSONObject).put("period_ms", this.periodMs);
        }
    }

    public static ControlData decode(String str) throws JSONException {
        if (str == null || str.length() > 1048576) {
            throw new JSONException("백업 파일이 너무 큽니다.");
        }
        JSONObject jSONObject = new JSONObject(str);
        if (!"mini-mobile-keypad".equals(jSONObject.optString("format")) || jSONObject.optInt("version", -1) != 1) {
            throw new JSONException("Mini 키패드 배치 백업 파일이 아닙니다.");
        }
        ControlData parseActive = parseActive(jSONObject.getJSONObject("active"));
        JSONObject jSONObject2 = jSONObject.getJSONObject("slots");
        if (jSONObject2.length() > 32) {
            throw new JSONException("저장된 배치가 너무 많습니다.");
        }
        Iterator<String> keys = jSONObject2.keys();
        while (keys.hasNext()) {
            String next = keys.next();
            if (next.length() == 0 || !next.equals(name(next))) {
                throw new JSONException("배치 이름이 올바르지 않습니다.");
            }
            parseActive.slots.put(next, parseActive(jSONObject2.getJSONObject(next)));
        }
        return parseActive;
    }

    public static String name(String str) {
        if (str == null) {
            return "";
        }
        String trim = str.trim();
        return trim.length() > 40 ? trim.substring(0, 40) : trim;
    }

    public static ControlData parseActive(JSONObject jSONObject) throws JSONException {
        ControlData controlData = new ControlData();
        controlData.portrait = Layout.parse(jSONObject.getJSONObject("portrait"));
        controlData.landscape = Layout.parse(jSONObject.getJSONObject("landscape"));
        if (jSONObject.has("rapid_fire")) {
            controlData.rapid = RapidSettings.parse(jSONObject.getJSONObject("rapid_fire"));
        }
        return controlData;
    }

    public ControlData activeCopy() {
        ControlData controlData = new ControlData();
        controlData.portrait = this.portrait.copy();
        controlData.landscape = this.landscape.copy();
        controlData.rapid = this.rapid.copy();
        return controlData;
    }

    public JSONObject activeJson() throws JSONException {
        return new JSONObject().put("portrait", this.portrait.json()).put("landscape", this.landscape.json()).put("rapid_fire", this.rapid.json());
    }

    public void applyActive(ControlData controlData) {
        this.portrait = controlData.portrait.copy();
        this.landscape = controlData.landscape.copy();
        this.rapid = controlData.rapid.copy();
    }

    public String encode() throws JSONException {
        JSONObject jSONObject = new JSONObject();
        for (Map.Entry<String, ControlData> entry : this.slots.entrySet()) {
            jSONObject.put(entry.getKey(), entry.getValue().activeJson());
        }
        return new JSONObject().put("format", "mini-mobile-keypad").put("version", 1).put("active", activeJson()).put("slots", jSONObject).toString(2);
    }

    public Layout layout(boolean z) {
        return z ? this.landscape : this.portrait;
    }
}
