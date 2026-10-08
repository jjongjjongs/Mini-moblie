import SwiftUI

/// How the title's screen fills its area.
enum ScreenScale: String, CaseIterable, Identifiable {
    /// As large as fits, keeping its shape.
    case fit
    /// The largest whole multiple that fits: every handset pixel the same size.
    case integer
    /// The whole area, shape or not.
    case fill

    var id: String { rawValue }

    var label: String {
        switch self {
        case .fit: return "맞춤"
        case .integer: return "정수배"
        case .fill: return "꽉 채움"
        }
    }

    func size(of image: CGSize, in area: CGSize) -> CGSize {
        guard image.width > 0, image.height > 0 else { return .zero }
        let fit = min(area.width / image.width, area.height / image.height)
        switch self {
        case .fit:
            return CGSize(width: image.width * fit, height: image.height * fit)
        case .integer:
            let whole = fit.rounded(.down)
            let factor = whole >= 1 ? whole : fit
            return CGSize(width: image.width * factor, height: image.height * factor)
        case .fill:
            return area
        }
    }
}

/// The settings the game screen reads, by their UserDefaults keys.
enum SettingKey {
    static let scale = "screen.scale"
    static let smooth = "screen.smooth"
    static let padMode = "pad.mode"
    static let padHeight = "pad.height"
    static let opacityBelow = "pad.opacity.below"
    static let opacityOverlay = "pad.opacity.overlay"
    static let labelScale = "pad.label"
    static let haptics = "pad.haptics"
    static let layoutBelow = "pad.layout.below"
    static let layoutOverlay = "pad.layout.overlay"
}

private enum GameSheet: String, Identifiable {
    case speed
    case settings
    case log

    var id: String { rawValue }
}

/// A title running: its screen on top, the handset's keys below or over it.
struct GameView: View {
    let game: GameFile

    @StateObject private var emulator = Emulator()
    @Environment(\.dismiss) private var dismiss

    @AppStorage(SettingKey.scale) private var scale = ScreenScale.fit
    @AppStorage(SettingKey.smooth) private var smooth = false
    @AppStorage(SettingKey.padMode) private var padMode = PadMode.below
    @AppStorage(SettingKey.padHeight) private var padHeight = 0.42
    @AppStorage(SettingKey.opacityBelow) private var opacityBelow = 1.0
    @AppStorage(SettingKey.opacityOverlay) private var opacityOverlay = 0.55
    @AppStorage(SettingKey.labelScale) private var labelScale = 1.0
    @AppStorage(SettingKey.haptics) private var haptics = true
    @AppStorage(SettingKey.layoutBelow) private var layoutBelow = ""
    @AppStorage(SettingKey.layoutOverlay) private var layoutOverlay = ""

    @State private var sheet: GameSheet?
    @State private var speed: Float = 1
    /// The layout being edited, nil when the pad is in play.
    @State private var draft: PadLayout?
    @State private var selectedKey: Int32?

    private var layout: PadLayout {
        PadLayout.decode(padMode == .below ? layoutBelow : layoutOverlay, mode: padMode)
    }

    var body: some View {
        VStack(spacing: 6) {
            if draft != nil {
                editBar
            } else {
                topBar
            }

            GeometryReader { geometry in
                switch padMode {
                case .below:
                    VStack(spacing: 8) {
                        screen(alignment: .center)
                        pad
                            .frame(height: geometry.size.height * min(max(padHeight, 0.25), 0.65))
                    }
                case .overlay:
                    ZStack {
                        screen(alignment: .top)
                        pad
                    }
                }
            }
        }
        .padding(8)
        .background(Color(white: 0.08).ignoresSafeArea())
        .statusBar(hidden: true)
        .onAppear {
            speed = GameSpeed.get(game)
            emulator.start(game: game)
        }
        .onDisappear { emulator.stop() }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .speed:
                SpeedView(game: game, speed: $speed)
            case .settings:
                GameSettingsView(onEditPad: beginEditing)
            case .log:
                LogView()
            }
        }
    }

    // MARK: - Bars

    private var topBar: some View {
        HStack {
            Button("닫기") {
                emulator.stop()
                dismiss()
            }
            Spacer()
            Text(game.title)
                .font(.headline)
                .lineLimit(1)
            if speed != 1 {
                Text(GameSpeed.format(speed))
                    .font(.caption.weight(.bold))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Capsule().fill(Color.orange.opacity(0.8)))
            }
            Spacer()
            Menu {
                Button { sheet = .speed } label: {
                    Label("게임 속도 (\(GameSpeed.format(speed)))", systemImage: "speedometer")
                }
                Button { sheet = .settings } label: {
                    Label("화면·패드 설정", systemImage: "slider.horizontal.3")
                }
                Button(action: beginEditing) {
                    Label("가상 패드 편집", systemImage: "square.grid.3x3")
                }
                Button { sheet = .log } label: {
                    Label("로그 보기", systemImage: "doc.text.magnifyingglass")
                }
            } label: {
                Image(systemName: "ellipsis.circle")
                    .font(.title3)
                    .frame(width: 44, height: 32)
            }
        }
        .foregroundColor(.white)
        .padding(.horizontal, 4)
    }

    private var editBar: some View {
        let chosen = draft?.keys.first { $0.index == selectedKey }

        return VStack(spacing: 6) {
            HStack {
                Button("취소") {
                    draft = nil
                    selectedKey = nil
                }
                Spacer()
                Text("패드 편집 · \(padMode.label)")
                    .font(.headline)
                Spacer()
                Button("저장") {
                    if let draft {
                        if padMode == .below {
                            layoutBelow = draft.encoded()
                        } else {
                            layoutOverlay = draft.encoded()
                        }
                    }
                    draft = nil
                    selectedKey = nil
                }
                .font(.body.weight(.bold))
            }
            HStack(spacing: 8) {
                Button("기본 배치") {
                    draft = PadLayout.standard(padMode)
                    selectedKey = nil
                }
                .buttonStyle(.bordered)
                Button(chosen?.hidden == true ? "\(chosen?.label ?? "") 보이기" : "\(chosen?.label ?? "키") 숨기기") {
                    guard let index = selectedKey, let position = draft?.keys.firstIndex(where: { $0.index == index }) else { return }
                    draft?.keys[position].hidden.toggle()
                }
                .buttonStyle(.bordered)
                .disabled(chosen == nil)
                Spacer()
            }
            Text("키를 끌어 옮기고, 노란 모서리를 끌어 크기를 바꿉니다. 흐린 키는 숨겨진 키입니다.")
                .font(.caption)
                .foregroundColor(.white.opacity(0.7))
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundColor(.white)
        .tint(.yellow)
        .padding(.horizontal, 4)
    }

    // MARK: - Screen and pad

    private func screen(alignment: Alignment) -> some View {
        GeometryReader { geometry in
            ZStack(alignment: alignment) {
                Color.black
                if let frame = emulator.frame {
                    let size = scale.size(of: CGSize(width: frame.width, height: frame.height), in: geometry.size)
                    Image(decorative: frame, scale: 1)
                        .interpolation(smooth ? .high : .none)
                        .antialiased(smooth)
                        .resizable()
                        .frame(width: size.width, height: size.height)
                }
                if let message = emulator.message {
                    Text(message)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .padding()
                        .background(Color.black.opacity(0.7))
                        .cornerRadius(8)
                        .padding()
                        .frame(maxHeight: .infinity)
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height, alignment: alignment)
        }
    }

    @ViewBuilder
    private var pad: some View {
        if draft != nil {
            PadEditor(
                layout: Binding(get: { draft ?? layout }, set: { draft = $0 }),
                selected: $selectedKey,
                labelScale: labelScale
            )
        } else {
            PadView(
                layout: layout,
                opacity: padMode == .below ? opacityBelow : opacityOverlay,
                labelScale: labelScale,
                haptics: haptics
            ) { index, pressed in
                emulator.key(index, pressed: pressed)
            }
        }
    }

    private func beginEditing() {
        sheet = nil
        selectedKey = nil
        draft = layout
    }
}

/// The game speed: the common speeds as chips, quarters between on a slider.
/// It applies as it changes and is kept for this title.
private struct SpeedView: View {
    let game: GameFile
    @Binding var speed: Float
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationView {
            Form {
                Section {
                    Text(GameSpeed.format(speed))
                        .font(.system(size: 44, weight: .bold, design: .rounded))
                        .frame(maxWidth: .infinity)
                    HStack(spacing: 6) {
                        ForEach(GameSpeed.chips, id: \.self) { value in
                            Button(GameSpeed.format(value)) { speed = value }
                                .buttonStyle(.bordered)
                                .tint(speed == value ? Color.accentColor : Color.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    Slider(value: $speed, in: GameSpeed.range, step: GameSpeed.step) {
                        Text("속도")
                    } minimumValueLabel: {
                        Text("0.5x")
                    } maximumValueLabel: {
                        Text("4x")
                    }
                } footer: {
                    Text("이 게임에만 적용되고, 다음에 실행할 때도 유지됩니다. 느려지는 장면은 빠르게, 너무 빠른 게임은 느리게 맞추세요.")
                }
            }
            .navigationTitle("게임 속도")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("1x로") { speed = 1 }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("완료") { dismiss() }
                }
            }
        }
        .onChange(of: speed) { value in
            GameSpeed.set(value, for: game)
            wie_set_speed(value)
        }
    }
}

/// Screen scaling and the pad's look, applied as they change.
private struct GameSettingsView: View {
    let onEditPad: () -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage(SettingKey.scale) private var scale = ScreenScale.fit
    @AppStorage(SettingKey.smooth) private var smooth = false
    @AppStorage(SettingKey.padMode) private var padMode = PadMode.below
    @AppStorage(SettingKey.padHeight) private var padHeight = 0.42
    @AppStorage(SettingKey.opacityBelow) private var opacityBelow = 1.0
    @AppStorage(SettingKey.opacityOverlay) private var opacityOverlay = 0.55
    @AppStorage(SettingKey.labelScale) private var labelScale = 1.0
    @AppStorage(SettingKey.haptics) private var haptics = true
    @AppStorage(SettingKey.layoutBelow) private var layoutBelow = ""
    @AppStorage(SettingKey.layoutOverlay) private var layoutOverlay = ""

    @State private var confirmingReset = false

    var body: some View {
        NavigationView {
            Form {
                Section {
                    Picker("확대 방식", selection: $scale) {
                        ForEach(ScreenScale.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    Toggle("부드럽게 (필터링)", isOn: $smooth)
                } header: {
                    Text("화면")
                } footer: {
                    Text("정수배는 모든 픽셀을 같은 크기로 키워 글자가 가장 또렷합니다. 부드럽게를 켜면 계단 현상 대신 흐릿하게 확대합니다.")
                }

                Section {
                    Picker("배치", selection: $padMode) {
                        ForEach(PadMode.allCases) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    if padMode == .below {
                        slider("패드 높이", value: $padHeight, range: 0.25...0.65)
                        slider("불투명도", value: $opacityBelow, range: 0.2...1)
                    } else {
                        slider("불투명도", value: $opacityOverlay, range: 0.1...1)
                    }
                    slider("글자 크기", value: $labelScale, range: 0.6...1.6)
                    Toggle("누를 때 진동", isOn: $haptics)
                } header: {
                    Text("가상 패드")
                }

                Section {
                    Button("키 위치·크기 편집") {
                        dismiss()
                        onEditPad()
                    }
                    Button("기본 배치로 되돌리기", role: .destructive) {
                        confirmingReset = true
                    }
                } footer: {
                    Text("편집은 지금 배치(\(padMode.label))에만 적용됩니다. 두 배치는 따로 저장됩니다.")
                }
            }
            .navigationTitle("화면·패드 설정")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("완료") { dismiss() }
                }
            }
            .confirmationDialog("\(padMode.label) 배치를 기본값으로 되돌릴까요?", isPresented: $confirmingReset, titleVisibility: .visible) {
                Button("되돌리기", role: .destructive) {
                    if padMode == .below {
                        layoutBelow = ""
                    } else {
                        layoutOverlay = ""
                    }
                }
            }
        }
    }

    private func slider(_ title: String, value: Binding<Double>, range: ClosedRange<Double>) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(title)
                Spacer()
                Text("\(Int((value.wrappedValue * 100).rounded()))%")
                    .foregroundColor(.secondary)
                    .monospacedDigit()
            }
            Slider(value: value, in: range)
        }
    }
}

/// The emulator's log, to read or to send along with a report.
private struct LogView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var sharing: SharedFile?

    /// How much of the log's end is shown; a long session's whole log would
    /// make the text view crawl. Sharing sends all of it.
    private static let shown = 200_000

    var body: some View {
        NavigationView {
            ScrollViewReader { reader in
                ScrollView {
                    Text(text.isEmpty ? "로그가 비어 있습니다." : String(text.suffix(Self.shown)))
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(10)
                    Color.clear.frame(height: 1).id("end")
                }
                .onAppear {
                    reload()
                    DispatchQueue.main.async { reader.scrollTo("end", anchor: .bottom) }
                }
            }
            .navigationTitle("로그")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("닫기") { dismiss() }
                }
                ToolbarItemGroup(placement: .navigationBarTrailing) {
                    Button(action: reload) { Image(systemName: "arrow.clockwise") }
                    Button {
                        UIPasteboard.general.string = text
                    } label: {
                        Image(systemName: "doc.on.doc")
                    }
                    Button(action: share) { Image(systemName: "square.and.arrow.up") }
                }
            }
            .sheet(item: $sharing) { file in
                ShareSheet(items: [file.url])
            }
        }
    }

    private func reload() {
        text = takeString(wie_log()) ?? ""
    }

    /// Sends the log as a text file, which survives being sent where a long
    /// pasted message would be cut.
    private func share() {
        reload()
        let stamp = DateFormatter.saveStamp.string(from: Date())
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("MiniMobile_log_\(stamp).txt")
        do {
            try text.write(to: url, atomically: true, encoding: .utf8)
            sharing = SharedFile(url: url)
        } catch {
            UIPasteboard.general.string = text
        }
    }
}
