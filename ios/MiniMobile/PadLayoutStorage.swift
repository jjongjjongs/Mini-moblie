import SwiftUI
import UniformTypeIdentifiers

/// The pad's three layouts as their settings keep them, one encoded layout
/// per arrangement - what a saved layout and a backup hold.
struct PadLayouts: Codable, Equatable {
    var below: String
    var overlay: String
    var numbers: String
}

/// Layouts saved on this device under a name, at most 32, as the Android
/// app keeps them.
enum PadLayoutSlots {
    private static let key = "padLayoutSlots"
    static let limit = 32

    static func all() -> [String: PadLayouts] {
        guard let data = UserDefaults.standard.data(forKey: key),
              let slots = try? JSONDecoder().decode([String: PadLayouts].self, from: data)
        else { return [:] }
        return slots
    }

    static func store(_ slots: [String: PadLayouts]) {
        if let data = try? JSONEncoder().encode(slots) {
            UserDefaults.standard.set(data, forKey: key)
        }
    }

    /// A name as a saved layout may have it: trimmed, 40 characters at most.
    static func name(_ text: String) -> String {
        String(text.trimmingCharacters(in: .whitespacesAndNewlines).prefix(40))
    }
}

/// A backup file of the layouts, in the Android app's format
/// (`mini-mobile-keypad`, version 1), so one made on either moves to the
/// other: the pad under the screen is its portrait layout, the pad over the
/// screen its landscape one, 숫자 크게 its numbers one. Keys a handset of the
/// other app lacks are left to the standard layout.
enum PadLayoutBackup {
    enum Failure: LocalizedError {
        case notBackup
        case damaged(String)

        var errorDescription: String? {
            switch self {
            case .notBackup: return "Mini 키패드 배치 백업 파일이 아닙니다."
            case .damaged(let what): return what
            }
        }
    }

    /// The keys the Android app lays out: every handset key but 종료.
    private static let keyCount: Int32 = 21

    static func encode(active: PadLayouts, slots: [String: PadLayouts]) throws -> Data {
        var saved: [String: Any] = [:]
        for (name, layouts) in slots {
            saved[name] = json(layouts)
        }
        let root: [String: Any] = ["format": "mini-mobile-keypad", "version": 1, "active": json(active), "slots": saved]
        return try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys])
    }

    static func decode(_ data: Data) throws -> (active: PadLayouts, slots: [String: PadLayouts]) {
        guard data.count <= 1_048_576,
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              root["format"] as? String == "mini-mobile-keypad",
              (root["version"] as? NSNumber)?.intValue == 1,
              let active = root["active"] as? [String: Any]
        else { throw Failure.notBackup }
        var slots: [String: PadLayouts] = [:]
        if let saved = root["slots"] as? [String: Any] {
            guard saved.count <= PadLayoutSlots.limit else { throw Failure.damaged("저장된 배치가 너무 많습니다.") }
            for (name, value) in saved {
                guard !name.isEmpty, name == PadLayoutSlots.name(name), let value = value as? [String: Any] else {
                    throw Failure.damaged("배치 이름이 올바르지 않습니다.")
                }
                slots[name] = try layouts(value)
            }
        }
        return (try layouts(active), slots)
    }

    private static func json(_ layouts: PadLayouts) -> [String: Any] {
        [
            "portrait": json(PadLayout.decode(layouts.below, standard: .below)),
            "landscape": json(PadLayout.decode(layouts.overlay, standard: .overlay)),
            "numbers": json(PadLayout.decode(layouts.numbers, standard: .numbers)),
        ]
    }

    private static func json(_ layout: PadLayout) -> [String: Any] {
        var keys: [String: Any] = [:]
        for key in layout.keys where key.index >= 0 && key.index < keyCount {
            keys[String(key.index)] = ["hidden": key.hidden, "rect": [key.x, key.y, key.w, key.h]]
        }
        return ["keys": keys]
    }

    private static func layouts(_ json: [String: Any]) throws -> PadLayouts {
        guard let portrait = json["portrait"] as? [String: Any], let landscape = json["landscape"] as? [String: Any] else {
            throw Failure.damaged("배치가 들어 있지 않습니다.")
        }
        let numbers = json["numbers"] as? [String: Any]
        return PadLayouts(
            below: try layout(portrait, standard: .below).encoded(),
            overlay: try layout(landscape, standard: .overlay).encoded(),
            // A backup from before the arrangement existed has none.
            numbers: try numbers.map { try layout($0, standard: .numbers).encoded() } ?? ""
        )
    }

    private static func layout(_ json: [String: Any], standard arrangement: PadArrangement) throws -> PadLayout {
        guard let keys = json["keys"] as? [String: Any], keys.count <= Int(keyCount) else {
            throw Failure.damaged("버튼 수가 올바르지 않습니다.")
        }
        var layout = PadLayout.standard(arrangement)
        for (name, value) in keys {
            guard let index = Int32(name), index >= 0, index < keyCount, String(index) == name,
                  let value = value as? [String: Any],
                  let position = layout.keys.firstIndex(where: { $0.index == index })
            else { throw Failure.damaged("잘못된 버튼 번호입니다.") }
            if let hidden = value["hidden"] {
                guard let hidden = hidden as? Bool else { throw Failure.damaged("잘못된 숨김 설정입니다.") }
                layout.keys[position].hidden = hidden
            }
            if let rect = value["rect"] {
                guard let rect = rect as? [NSNumber], rect.count == 4 else { throw Failure.damaged("버튼 좌표가 올바르지 않습니다.") }
                let (x, y, w, h) = (rect[0].doubleValue, rect[1].doubleValue, rect[2].doubleValue, rect[3].doubleValue)
                guard [x, y, w, h].allSatisfy({ $0.isFinite }), x >= 0, y >= 0, w >= 0.01, h >= 0.01, w <= 1, h <= 1,
                      x + w <= 1.001, y + h <= 1.001
                else { throw Failure.damaged("버튼이 키패드 영역을 벗어납니다.") }
                layout.keys[position].x = min(x, 1 - w)
                layout.keys[position].y = min(y, 1 - h)
                layout.keys[position].w = w
                layout.keys[position].h = h
            }
        }
        return layout
    }
}

/// 배치 저장·백업·초기화: the layout saved under a name, loaded or deleted,
/// backed up to a file and restored from one, or put back to the standard.
struct PadLayoutStorageView: View {
    /// What 현재 배치 초기화 resets: the arrangement in use, by name.
    let arrangementLabel: String
    let resetCurrent: () -> Void

    @AppStorage(SettingKey.layoutBelow) private var layoutBelow = ""
    @AppStorage(SettingKey.layoutOverlay) private var layoutOverlay = ""
    @AppStorage(SettingKey.layoutNumbers) private var layoutNumbers = ""

    @State private var slots = PadLayoutSlots.all()
    @State private var sheet: StorageSheet?
    @State private var overwriting: String?
    @State private var importing = false
    @State private var confirmingReset = false
    @State private var notice: String?

    private var current: PadLayouts {
        PadLayouts(below: layoutBelow, overlay: layoutOverlay, numbers: layoutNumbers)
    }

    var body: some View {
        Form {
            Section {
                Button {
                    sheet = .naming
                } label: {
                    row("현재 배치 저장", systemImage: "square.and.arrow.down.on.square", tint: .blue)
                }
                NavigationLink {
                    PadLayoutSlotsView(slots: $slots, load: load)
                } label: {
                    HStack {
                        row("저장된 배치 불러오기·삭제", systemImage: "folder", tint: .orange)
                        Spacer()
                        Text("\(slots.count)개").foregroundColor(.secondary)
                    }
                }
            } header: {
                Text("이 기기에")
            }

            Section {
                Button(action: export) {
                    row("배치 파일로 백업", systemImage: "square.and.arrow.up", tint: .red)
                }
                Button {
                    importing = true
                } label: {
                    row("배치 파일에서 복원", systemImage: "square.and.arrow.down", tint: .green)
                }
            } header: {
                Text("파일로")
            } footer: {
                Text("백업 파일은 안드로이드 앱과 같은 형식이라 서로 옮겨 쓸 수 있어요. 저장한 배치도 함께 들어갑니다.")
            }

            Section {
                Button(role: .destructive) {
                    confirmingReset = true
                } label: {
                    Label("현재 배치 초기화", systemImage: "arrow.counterclockwise")
                }
            } header: {
                Text("되돌리기")
            } footer: {
                Text("지금 배치(\(arrangementLabel))만 처음 배치로 되돌립니다.")
            }
        }
        .navigationTitle("배치 저장·백업·초기화")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog("‘\(overwriting ?? "")’이 이미 있어요. 덮어쓸까요?",
                            isPresented: Binding(get: { overwriting != nil }, set: { if !$0 { overwriting = nil } }),
                            titleVisibility: .visible) {
            Button("덮어쓰기", role: .destructive) {
                if let name = overwriting { store(name) }
            }
        }
        .confirmationDialog("\(arrangementLabel) 배치를 기본값으로 되돌릴까요?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("되돌리기", role: .destructive, action: resetCurrent)
        }
        .alert(notice ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
            Button("확인", role: .cancel) {}
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.json, .item]) { result in
            restore(result)
        }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .naming:
                PadLayoutNameView { name in
                    // Asked once the sheet is down, so an overwrite question
                    // has somewhere to show.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { save(PadLayoutSlots.name(name)) }
                }
            case .sharing(let url):
                ShareSheet(items: [url])
            }
        }
    }

    private func row(_ title: String, systemImage: String, tint: Color) -> some View {
        Label {
            Text(title).foregroundColor(.primary)
        } icon: {
            Image(systemName: systemImage).foregroundColor(tint)
        }
    }

    private func save(_ name: String) {
        guard !name.isEmpty else {
            notice = "배치 이름을 입력하세요."
            return
        }
        if slots[name] != nil {
            overwriting = name
        } else if slots.count >= PadLayoutSlots.limit {
            notice = "최대 \(PadLayoutSlots.limit)개까지 저장할 수 있습니다."
        } else {
            store(name)
        }
    }

    private func store(_ name: String) {
        slots[name] = current
        PadLayoutSlots.store(slots)
        notice = "‘\(name)’ 저장 완료"
    }

    private func load(_ name: String) {
        guard let layouts = slots[name] else { return }
        apply(layouts)
        notice = "‘\(name)’ 적용 완료"
    }

    private func apply(_ layouts: PadLayouts) {
        layoutBelow = layouts.below
        layoutOverlay = layouts.overlay
        layoutNumbers = layouts.numbers
    }

    private func export() {
        let stamp = DateFormatter.saveStamp.string(from: Date())
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("MiniMobile_keypad_\(stamp).json")
        do {
            try PadLayoutBackup.encode(active: current, slots: slots).write(to: url, options: .atomic)
            sheet = .sharing(url)
        } catch {
            notice = "백업 파일을 만들지 못했습니다: \(error.localizedDescription)"
        }
    }

    private func restore(_ result: Result<URL, Error>) {
        do {
            let url = try result.get()
            let scoped = url.startAccessingSecurityScopedResource()
            defer {
                if scoped { url.stopAccessingSecurityScopedResource() }
            }
            let backup = try PadLayoutBackup.decode(try Data(contentsOf: url))
            apply(backup.active)
            // Saved layouts of the same name are replaced; the rest stay.
            for (name, layouts) in backup.slots where slots[name] != nil || slots.count < PadLayoutSlots.limit {
                slots[name] = layouts
            }
            PadLayoutSlots.store(slots)
            notice = "배치를 복원했습니다. 저장된 배치 \(backup.slots.count)개도 함께 가져왔어요."
        } catch {
            notice = "복원하지 못했습니다: \(error.localizedDescription)"
        }
    }
}

/// What 배치 저장·백업·초기화 shows over itself.
private enum StorageSheet: Identifiable {
    case naming
    case sharing(URL)

    var id: String {
        switch self {
        case .naming: return "naming"
        case .sharing(let url): return url.path
        }
    }
}

/// Asks the name to save the layout under.
private struct PadLayoutNameView: View {
    let save: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""

    var body: some View {
        NavigationView {
            Form {
                Section {
                    TextField("배치 이름", text: $name)
                } footer: {
                    Text("세 가지 위치(화면 아래·겹치기·숫자 크게)의 배치를 함께 저장합니다. 같은 이름이 있으면 덮어쓸지 물어봐요.")
                }
            }
            .navigationTitle("현재 배치 저장")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("취소") { dismiss() }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("저장") {
                        save(name)
                        dismiss()
                    }
                    .disabled(PadLayoutSlots.name(name).isEmpty)
                }
            }
        }
    }
}

/// The layouts saved on this device: a tap loads one, a swipe deletes it.
private struct PadLayoutSlotsView: View {
    @Binding var slots: [String: PadLayouts]
    let load: (String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var deleting: String?

    var body: some View {
        List {
            if slots.isEmpty {
                Text("저장된 배치가 없습니다.").foregroundColor(.secondary)
            }
            ForEach(slots.keys.sorted(), id: \.self) { name in
                Button {
                    load(name)
                    dismiss()
                } label: {
                    HStack {
                        Text(name).foregroundColor(.primary)
                        Spacer()
                        Text("불러오기").font(.subheadline).foregroundColor(.accentColor)
                    }
                }
                .swipeActions {
                    Button(role: .destructive) {
                        deleting = name
                    } label: {
                        Label("삭제", systemImage: "trash")
                    }
                }
            }
        }
        .navigationTitle("저장된 배치")
        .navigationBarTitleDisplayMode(.inline)
        .confirmationDialog("‘\(deleting ?? "")’을 삭제할까요?",
                            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
                            titleVisibility: .visible) {
            Button("삭제", role: .destructive) {
                if let name = deleting {
                    slots[name] = nil
                    PadLayoutSlots.store(slots)
                }
            }
        } message: {
            Text("현재 적용된 배치는 유지됩니다.")
        }
    }
}
