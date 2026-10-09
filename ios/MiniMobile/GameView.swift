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
    /// The smoothing toggle the quality setting replaced, read once as the
    /// starting choice (see `GameQuality`).
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
    case quality
    case sound
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
    /// How the screen is enlarged (see `GameQuality`).
    @State private var quality = ScreenQuality.dot
    /// Whether the music gives way to effects (see `GameSound`).
    @State private var oneSound = false
    /// The layout being edited, nil when the pad is in play.
    @State private var draft: PadLayout?
    @State private var selectedKey: Int32?
    /// Whether touches on the screen reach the title (see `GameTouch`).
    @State private var touch = false
    /// The frame pixel the finger on the screen is at, nil with none down.
    @State private var touchedAt: FramePoint?
    /// Whether the pad is put away, the whole screen left to the game (see
    /// `GamePad`).
    @State private var padHidden = false

    private var layout: PadLayout {
        PadLayout.decode(padMode == .below ? layoutBelow : layoutOverlay, mode: padMode)
    }

    var body: some View {
        Group {
            if padHidden && draft == nil {
                fullScreen
            } else {
                withPad
            }
        }
        .background(Color(white: 0.08).ignoresSafeArea())
        .statusBar(hidden: true)
        .onAppear {
            speed = GameSpeed.get(game)
            quality = GameQuality.get(game)
            oneSound = GameSound.oneAtATime(game)
            touch = GameTouch.get(game)
            padHidden = GamePad.hidden(game)
            emulator.start(game: game)
        }
        .onDisappear { emulator.stop() }
        .onChange(of: quality) { value in emulator.setHq2x(value == .hq2x) }
        .sheet(item: $sheet) { sheet in
            switch sheet {
            case .speed:
                SpeedView(game: game, speed: $speed)
            case .quality:
                QualityView(game: game, emulator: emulator, quality: $quality)
            case .sound:
                SoundView(game: game, oneSound: $oneSound)
            case .settings:
                GameSettingsView(onEditPad: beginEditing)
            case .log:
                LogView()
            }
        }
    }

    /// The pad put away: the screen alone, with the menu on a translucent
    /// button over its corner, as the bar that held it is gone too.
    private var fullScreen: some View {
        ZStack(alignment: .topTrailing) {
            screen(alignment: .center)
                .ignoresSafeArea(edges: .bottom)
            Menu {
                menuItems
            } label: {
                Image(systemName: "ellipsis")
                    .font(.headline)
                    .foregroundColor(.white)
                    .frame(width: 36, height: 36)
                    .background(Circle().fill(Color.white.opacity(0.22)))
            }
            .padding(8)
        }
        .background(Color.black.ignoresSafeArea())
    }

    private var withPad: some View {
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
                menuItems
            } label: {
                Image(systemName: "ellipsis.circle")
                    .font(.title3)
                    .frame(width: 44, height: 32)
            }
        }
        .foregroundColor(.white)
        .padding(.horizontal, 4)
    }

    /// The game's menu, behind ⋯ in the bar or, with the pad put away, on
    /// the button over the screen.
    @ViewBuilder
    private var menuItems: some View {
        Button(action: togglePad) {
            Label(padHidden ? "키패드 꺼내기" : "키패드 숨기기",
                  systemImage: padHidden ? "keyboard" : "keyboard.chevron.compact.down")
        }
        Button { sheet = .speed } label: {
            Label("게임 속도 (\(GameSpeed.format(speed)))", systemImage: "speedometer")
        }
        Button { sheet = .quality } label: {
            Label("화질 (\(quality.label))", systemImage: "photo")
        }
        Button { sheet = .sound } label: {
            Label("소리 (\(oneSound ? "각각" : "동시"))", systemImage: "speaker.wave.2")
        }
        Button { sheet = .settings } label: {
            Label("화면·패드 설정", systemImage: "slider.horizontal.3")
        }
        if !padHidden {
            Button(action: beginEditing) {
                Label("가상 패드 편집", systemImage: "square.grid.3x3")
            }
        }
        Button(action: toggleTouch) {
            Label(touch ? "화면 터치: 켜짐" : "화면 터치: 꺼짐", systemImage: touch ? "hand.tap.fill" : "hand.tap")
        }
        Button { sheet = .log } label: {
            Label("로그 보기", systemImage: "doc.text.magnifyingglass")
        }
        // The bar with 닫기 is put away with the pad.
        if padHidden {
            Button(role: .destructive) {
                emulator.stop()
                dismiss()
            } label: {
                Label("게임 닫기", systemImage: "xmark")
            }
        }
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
                    // The title's own pixels, however many of the frame's each
                    // takes once doubled through hq2x.
                    let pixels = CGSize(
                        width: CGFloat(frame.width) / CGFloat(emulator.frameScale),
                        height: CGFloat(frame.height) / CGFloat(emulator.frameScale)
                    )
                    let size = scale.size(of: pixels, in: geometry.size)
                    Image(decorative: frame, scale: 1)
                        .interpolation(quality == .dot ? .none : .high)
                        .antialiased(quality != .dot)
                        .resizable()
                        .frame(width: size.width, height: size.height)
                        .gesture(
                            screenTouch(frame: pixels, shown: size),
                            including: touch ? .all : .none
                        )
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

    /// Turns touches on the screen on or off for this title and keeps the
    /// choice. A title that asks for a touch screen asks as it starts, so one
    /// that lays itself out for touch only does so after a restart.
    private func toggleTouch() {
        if touch, let point = touchedAt {
            wie_pointer(1, point.x, point.y)
            touchedAt = nil
        }
        touch.toggle()
        GameTouch.set(touch, for: game)
        wie_set_touch(touch)
    }

    /// A finger on the screen, handed to the title in the frame's own pixels:
    /// a press where it lands, a drag each time it reaches another pixel, a
    /// release where it lifts.
    private func screenTouch(frame: CGSize, shown: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                let point = FramePoint(location: value.location, frame: frame, shown: shown)
                if touchedAt == nil {
                    wie_pointer(0, point.x, point.y)
                } else if point != touchedAt {
                    wie_pointer(2, point.x, point.y)
                }
                touchedAt = point
            }
            .onEnded { _ in
                if let point = touchedAt {
                    wie_pointer(1, point.x, point.y)
                }
                touchedAt = nil
            }
    }

    /// Puts the pad away, or brings it back, and keeps the choice for this
    /// title.
    /// (A key held on the pad is let go as the pad leaves - see `PadView`.)
    private func togglePad() {
        padHidden.toggle()
        GamePad.setHidden(padHidden, for: game)
    }

    private func beginEditing() {
        // Editing needs the pad on screen.
        if padHidden {
            togglePad()
        }
        sheet = nil
        selectedKey = nil
        draft = layout
    }
}

/// A pixel of the title's frame.
struct FramePoint: Equatable {
    let x: Int32
    let y: Int32

    /// The pixel under `location` on a frame of `frame` pixels shown at
    /// `shown` points, clamped to its edge so a finger that slides off it is
    /// still on it.
    init(location: CGPoint, frame: CGSize, shown: CGSize) {
        guard frame.width > 0, frame.height > 0, shown.width > 0, shown.height > 0 else {
            x = 0
            y = 0
            return
        }
        x = Int32(min(max(location.x / shown.width * frame.width, 0), frame.width - 1))
        y = Int32(min(max(location.y / shown.height * frame.height, 0), frame.height - 1))
    }
}

/// The game speed: the speed with a tenth either side of it, the ruler from
/// 0.1x to 4x, and the common speeds as chips. It applies as it changes and is
/// kept for this title.
private struct SpeedView: View {
    let game: GameFile
    @Binding var speed: Float
    @Environment(\.dismiss) private var dismiss

    private var tenths: Int {
        Int((speed * 10).rounded())
    }

    var body: some View {
        NavigationView {
            Form {
                Section {
                    HStack(spacing: 18) {
                        step("minus", by: -1)
                        Text(GameSpeed.format(speed))
                            .font(.system(size: 44, weight: .bold, design: .rounded))
                            .monospacedDigit()
                            .frame(minWidth: 130)
                        step("plus", by: 1)
                    }
                    .frame(maxWidth: .infinity)
                    SpeedRuler(tenths: Binding(get: { tenths }, set: { speed = Float($0) / 10 }))
                        .frame(height: 58)
                    HStack(spacing: 6) {
                        ForEach(GameSpeed.chips, id: \.self) { value in
                            Button(GameSpeed.format(value)) {
                                withAnimation(.easeOut(duration: 0.25)) { speed = value }
                            }
                            .buttonStyle(.bordered)
                            .tint(speed == value ? Color.accentColor : Color.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                } footer: {
                    Text("줄자를 옆으로 밀거나 −/+ 로 0.1씩 맞출 수 있어요. 이 게임에만 적용되고, 다음에 실행할 때도 유지됩니다. 소리는 원래 속도로 재생됩니다.")
                }
            }
            .navigationTitle("게임 속도")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("1x로") {
                        withAnimation(.easeOut(duration: 0.25)) { speed = 1 }
                    }
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

    /// A round − or +: a tenth slower or faster.
    private func step(_ symbol: String, by change: Int) -> some View {
        Button {
            let next = min(max(tenths + change, GameSpeed.tenths.lowerBound), GameSpeed.tenths.upperBound)
            withAnimation(.easeOut(duration: 0.2)) { speed = Float(next) / 10 }
        } label: {
            Image(systemName: symbol)
                .font(.title3.weight(.semibold))
                .frame(width: 40, height: 40)
                .overlay(Circle().stroke(Color.secondary.opacity(0.6)))
        }
        .buttonStyle(.borderless)
    }
}

/// The speed ruler: a tick for every tenth from 0.1x to 4x, slid sideways
/// under a fixed needle. A finger drags it, a flick throws it on, a tap brings
/// the tick tapped under the needle, and it always comes to rest on a tick.
private struct SpeedRuler: View {
    @Binding var tenths: Int
    /// How far the finger has carried the ruler past the tick it is on.
    @State private var carried: CGFloat = 0
    /// The tick under the needle when the finger came down.
    @State private var startTenths: Int?

    private let spacing: CGFloat = 12

    var body: some View {
        GeometryReader { geometry in
            let centre = geometry.size.width / 2
            ZStack(alignment: .topLeading) {
                ForEach(GameSpeed.tenths, id: \.self) { value in
                    let x = centre + CGFloat(value - tenths) * spacing + carried
                    let distance = min(1, abs(x - centre) / max(centre, 1))
                    tick(value)
                        .position(x: x, y: 27)
                        .opacity(Double(1 - distance * distance))
                }
                Capsule()
                    .fill(Color.accentColor)
                    .frame(width: 3, height: 30)
                    .position(x: centre, y: 19)
                Image(systemName: "arrowtriangle.down.fill")
                    .font(.system(size: 10))
                    .foregroundColor(.accentColor)
                    .position(x: centre, y: 3)
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        let start = startTenths ?? tenths
                        startTenths = start
                        let moved = value.translation.width
                        let next = clamp(start - Int((moved / spacing).rounded()))
                        tenths = next
                        carried = max(-spacing / 2, min(spacing / 2, moved - CGFloat(start - next) * spacing))
                    }
                    .onEnded { value in
                        let start = startTenths ?? tenths
                        startTenths = nil
                        let target: Int
                        if abs(value.translation.width) < 4 {
                            target = start + Int(((value.location.x - centre) / spacing).rounded())
                        } else {
                            target = start - Int((value.predictedEndTranslation.width / spacing).rounded())
                        }
                        withAnimation(.easeOut(duration: 0.3)) {
                            tenths = clamp(target)
                            carried = 0
                        }
                    }
            )
        }
        .clipped()
    }

    private func tick(_ value: Int) -> some View {
        let whole = value % 10 == 0
        let half = value % 5 == 0
        return VStack(spacing: 6) {
            Rectangle()
                .fill(whole ? Color.primary : Color.secondary)
                .frame(width: whole ? 2 : 1, height: whole ? 24 : half ? 16 : 10)
            if half || value == GameSpeed.tenths.lowerBound {
                Text(GameSpeed.format(Float(value) / 10))
                    .font(.system(size: 11, weight: whole ? .semibold : .regular))
                    .foregroundColor(whole ? .primary : .secondary)
                    .fixedSize()
            }
        }
        .frame(width: 34, height: 50, alignment: .top)
    }

    private func clamp(_ value: Int) -> Int {
        min(max(value, GameSpeed.tenths.lowerBound), GameSpeed.tenths.upperBound)
    }
}

/// How the screen is enlarged: 기본, 도트 or HQ2X, each with a picture of the
/// middle of the screen drawn that way. It applies as it changes - on the
/// screen behind at once - and is kept for this title, or every title.
private struct QualityView: View {
    let game: GameFile
    let emulator: Emulator
    @Binding var quality: ScreenQuality
    @Environment(\.dismiss) private var dismiss
    @State private var everyGame = false
    @State private var previews: QualityPreviews?

    var body: some View {
        NavigationView {
            Form {
                Section {
                    ForEach(ScreenQuality.allCases) { option in
                        Button {
                            quality = option
                            GameQuality.set(option, for: game, everyGame: everyGame)
                        } label: {
                            HStack(spacing: 12) {
                                if let previews {
                                    previews.image(option)
                                        .frame(width: 84, height: 63)
                                        .clipShape(RoundedRectangle(cornerRadius: 6))
                                }
                                VStack(alignment: .leading, spacing: 3) {
                                    HStack(spacing: 6) {
                                        Text(option.label).font(.body.weight(.bold))
                                        if option == .dot {
                                            Text("지금 방식")
                                                .font(.caption2)
                                                .foregroundColor(.secondary)
                                                .padding(.horizontal, 4)
                                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color.secondary.opacity(0.5)))
                                        }
                                    }
                                    Text(option.detail)
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                                Spacer()
                                Image(systemName: quality == option ? "checkmark.circle.fill" : "circle")
                                    .foregroundColor(quality == option ? .accentColor : .secondary)
                            }
                        }
                        .foregroundColor(.primary)
                    }
                    Toggle("모든 게임에 이 화질 쓰기", isOn: $everyGame)
                        .onChange(of: everyGame) { all in
                            GameQuality.set(quality, for: game, everyGame: all)
                        }
                } footer: {
                    Text("고르는 즉시 게임 화면에 반영돼요. 이 게임에만 적용되고, 다음에 실행할 때도 유지됩니다.")
                }
            }
            .navigationTitle("화질")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("완료") { dismiss() }
                }
            }
        }
        .onAppear { previews = QualityPreviews(emulator.lastFrame()) }
    }
}

/// The middle of the screen, cut out and ready to be drawn each of the three
/// ways.
private struct QualityPreviews {
    let plain: CGImage
    let doubled: CGImage?

    init?(_ frame: CGImage?) {
        guard let frame else { return nil }
        let width = min(48, frame.width)
        let height = min(36, frame.height)
        let rect = CGRect(x: (frame.width - width) / 2, y: (frame.height - height) / 2, width: width, height: height)
        guard let crop = frame.cropping(to: rect) else { return nil }
        plain = crop
        doubled = Self.hq2x(crop)
    }

    @ViewBuilder
    func image(_ quality: ScreenQuality) -> some View {
        switch quality {
        case .smooth:
            Image(decorative: plain, scale: 1).interpolation(.high).resizable()
        case .dot:
            Image(decorative: plain, scale: 1).interpolation(.none).resizable()
        case .hq2x:
            Image(decorative: doubled ?? plain, scale: 1).interpolation(.high).resizable()
        }
    }

    /// `image` doubled through hq2x.
    private static func hq2x(_ image: CGImage) -> CGImage? {
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        var out = [UInt8](repeating: 0, count: width * height * 16)
        let doubled = out.withUnsafeMutableBufferPointer { buffer in
            wie_hq2x(pixels, UInt32(width), UInt32(height), buffer.baseAddress)
        }
        guard doubled, let provider = CGDataProvider(data: Data(out) as CFData) else { return nil }
        return CGImage(
            width: width * 2, height: height * 2, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 8,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent
        )
    }
}

/// 동시 - everything mixed, as it always was - or 각각, the music holding back
/// while an effect plays. Each has a picture of the music and effects along a
/// timeline. It applies as it changes and is kept for this title.
private struct SoundView: View {
    let game: GameFile
    @Binding var oneSound: Bool
    @Environment(\.dismiss) private var dismiss

    private static let effects: [ClosedRange<CGFloat>] = [0.22...0.34, 0.62...0.77]

    var body: some View {
        NavigationView {
            Form {
                Section {
                    option(false, title: "동시", tag: "지금 방식", detail: "배경음과 효과음을 함께 재생해요.", music: [0...1])
                    option(true, title: "각각", tag: nil, detail: "한 번에 하나만 재생해요. 효과음이 나는 동안 배경음이 잠깐 멈췄다 이어져요.",
                           music: [0...0.21, 0.35...0.61, 0.78...1])
                } footer: {
                    Text("소리가 겹쳐서 뭉개지거나, 원래 폰처럼 한 소리씩 듣고 싶을 때 ‘각각’을 고르세요. 이 게임에만 적용됩니다.")
                }
            }
            .navigationTitle("소리")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("완료") { dismiss() }
                }
            }
        }
    }

    private func option(_ value: Bool, title: String, tag: String?, detail: String, music: [ClosedRange<CGFloat>]) -> some View {
        Button {
            oneSound = value
            GameSound.set(oneAtATime: value, for: game)
            wie_set_one_sound_at_a_time(value)
        } label: {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 6) {
                        Text(title).font(.body.weight(.bold))
                        if let tag {
                            Text(tag)
                                .font(.caption2)
                                .foregroundColor(.secondary)
                                .padding(.horizontal, 4)
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(Color.secondary.opacity(0.5)))
                        }
                    }
                    Text(detail).font(.caption).foregroundColor(.secondary)
                    lane("배경음", spans: music, color: Color(red: 0.36, green: 0.55, blue: 0.94))
                    lane("효과음", spans: Self.effects, color: Color(red: 0.89, green: 0.61, blue: 0.31))
                }
                Spacer()
                Image(systemName: oneSound == value ? "checkmark.circle.fill" : "circle")
                    .foregroundColor(oneSound == value ? .accentColor : .secondary)
            }
        }
        .foregroundColor(.primary)
    }

    private func lane(_ name: String, spans: [ClosedRange<CGFloat>], color: Color) -> some View {
        HStack(spacing: 6) {
            Text(name)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .frame(width: 34, alignment: .leading)
            GeometryReader { geometry in
                ForEach(spans.indices, id: \.self) { index in
                    let span = spans[index]
                    Capsule()
                        .fill(color)
                        .frame(width: (span.upperBound - span.lowerBound) * geometry.size.width, height: 8)
                        .offset(x: span.lowerBound * geometry.size.width)
                }
            }
            .frame(height: 8)
        }
    }
}

/// Screen scaling and the pad's look, applied as they change.
private struct GameSettingsView: View {
    let onEditPad: () -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage(SettingKey.scale) private var scale = ScreenScale.fit
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
                } header: {
                    Text("화면")
                } footer: {
                    Text("정수배는 모든 픽셀을 같은 크기로 키워 글자가 가장 또렷합니다. 확대할 때 부드럽게 할지는 메뉴의 ‘화질’에서 게임마다 고릅니다.")
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
