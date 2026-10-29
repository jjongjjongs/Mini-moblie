import SwiftUI

/// How the title's screen is sized: 맞춤, 정수배, or 직접 조절's own width
/// and height.
enum ScaleMode: Int, CaseIterable, Identifiable {
    /// As large as fits, keeping its shape.
    case fit = 0
    /// The largest whole multiple that fits: every handset pixel the same size.
    case integer = 1
    /// 맞춤's width and height each taken by a percent the player sets.
    case custom = 2

    var id: Int { rawValue }

    var label: String {
        switch self {
        case .fit: return "맞춤"
        case .integer: return "정수배"
        case .custom: return "직접 조절"
        }
    }

    var detail: String {
        switch self {
        case .fit: return "비율을 지키며 칸에 들어가는 가장 큰 크기예요."
        case .integer: return "2배·3배처럼 딱 떨어지게 키워요. 도트가 고르게 보여요."
        case .custom: return "가로·세로를 원하는 크기로 맞춰요."
        }
    }
}

/// How big the title's screen is drawn in its area, kept per title and per
/// way up (see `GameScale`).
struct ScreenScale: Equatable {
    /// The smallest 직접 조절 goes, in percent of 맞춤.
    static let smallest = 50
    static let fit = ScreenScale(mode: .fit, width: 100, height: 100, linked: true)

    var mode: ScaleMode
    /// 직접 조절's width and height, in percent of 맞춤's.
    var width: Int
    var height: Int
    /// Whether 직접 조절's two rulers move together.
    var linked: Bool

    /// 맞춤, 정수배, 직접 80%, or 직접 110×100% when the sides differ.
    var label: String {
        switch mode {
        case .fit: return "맞춤"
        case .integer: return "정수배"
        case .custom: return width == height ? "직접 \(width)%" : "직접 \(width)×\(height)%"
        }
    }

    /// The size a frame of `image` pixels is shown at in `area`.
    func size(of image: CGSize, in area: CGSize) -> CGSize {
        guard image.width > 0, image.height > 0 else { return .zero }
        let fit = min(area.width / image.width, area.height / image.height)
        switch mode {
        case .fit:
            return CGSize(width: image.width * fit, height: image.height * fit)
        case .integer:
            let whole = fit.rounded(.down)
            let factor = whole >= 1 ? whole : fit
            return CGSize(width: image.width * factor, height: image.height * factor)
        case .custom:
            return CGSize(
                width: min(area.width, image.width * fit * CGFloat(width) / 100),
                height: min(area.height, image.height * fit * CGFloat(height) / 100)
            )
        }
    }

    /// How far 직접 조절 can take the width and the height of a frame of
    /// `image` pixels in `area`, in percent of 맞춤: as far as the area's
    /// edge. 맞춤 already meets the edge one way, so that side stops at 100.
    static func limits(of image: CGSize, in area: CGSize) -> (width: Int, height: Int) {
        guard image.width > 0, image.height > 0, area.width > 0, area.height > 0 else { return (100, 100) }
        let fit = min(area.width / image.width, area.height / image.height)
        return (
            max(100, Int((area.width * 100 / (image.width * fit)).rounded(.down))),
            max(100, Int((area.height * 100 / (image.height * fit)).rounded(.down)))
        )
    }

    /// "mode,width,height,linked", as it is kept - the Android app's form.
    var encoded: String {
        "\(mode.rawValue),\(width),\(height),\(linked ? 1 : 0)"
    }

    init(mode: ScaleMode, width: Int, height: Int, linked: Bool) {
        self.mode = mode
        self.width = width
        self.height = height
        self.linked = linked
    }

    /// What `encoded` wrote, or nil for anything else.
    init?(encoded: String) {
        let parts = encoded.split(separator: ",").map { Int($0) }
        guard parts.count == 4, let raw = parts[0], let mode = ScaleMode(rawValue: raw),
              let width = parts[1], let height = parts[2], let linked = parts[3],
              width >= Self.smallest, height >= Self.smallest else { return nil }
        self.init(mode: mode, width: width, height: height, linked: linked != 0)
    }
}

/// The settings the game screen reads, by their UserDefaults keys.
enum SettingKey {
    /// The one 확대 방식 every title shared before 화면 배율, read only for
    /// what it carries over (see `GameScale`).
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
    /// The 숫자 크게 layout's own positions, so editing it never moves the
    /// ordinary pad's keys.
    static let layoutNumbers = "pad.layout.numbers"
    /// 펼친 화면's own positions, the keys either side of the screen.
    static let layoutSides = "pad.layout.sides"
    /// The phone on its side: its own positions, kept apart from upright's.
    static let layoutLandscape = "pad.layout.landscape"
    /// 펼친 화면: whether a wide upright window lays the pad out either side
    /// of the screen, for every game.
    static let unfoldedSides = "pad.unfolded.sides"
    static let padTheme = "pad.theme"
}

/// The narrowest play area, in points, that counts as unfolded: an iPad
/// held upright, not an iPhone or an iPad split in two.
let unfoldedMinWidth: CGFloat = 600

/// Which stored layout and which standard one the pad is drawn from: with
/// `sideways` (the phone on its side) the keys either side of the screen
/// whatever the pad's position and shape, as the Android player has it; with
/// `sides` (펼친 화면 on a wide window) the columns either side of the screen.
func padArrangement(mode: PadMode, shape: PadShape, sides: Bool = false, sideways: Bool = false) -> PadArrangement {
    if sideways {
        return .landscape
    }
    if sides && mode == .below {
        return .sides
    }
    switch mode {
    case .overlay: return .overlay
    case .below: return shape == .numbers ? .numbers : .below
    }
}

/// Which window of the game menu is up.
private enum MenuWindow {
    case main
    case design
    case shape
    case unfolded
    case orientation
}

private enum GameSheet: String, Identifiable {
    case speed
    case quality
    case scale
    case settings
    case photo
    case log

    var id: String { rawValue }
}

/// A title running: its screen on top, the handset's keys below or over it.
struct GameView: View {
    let game: GameFile

    @StateObject private var emulator = Emulator()
    @Environment(\.dismiss) private var dismiss

    @AppStorage(SettingKey.padMode) private var padMode = PadMode.below
    @AppStorage(SettingKey.padHeight) private var padHeight = 0.42
    @AppStorage(SettingKey.opacityBelow) private var opacityBelow = 1.0
    @AppStorage(SettingKey.opacityOverlay) private var opacityOverlay = 0.55
    @AppStorage(SettingKey.labelScale) private var labelScale = 1.0
    @AppStorage(SettingKey.haptics) private var haptics = true
    @AppStorage(SettingKey.layoutBelow) private var layoutBelow = ""
    @AppStorage(SettingKey.layoutOverlay) private var layoutOverlay = ""
    @AppStorage(SettingKey.layoutNumbers) private var layoutNumbers = ""
    @AppStorage(SettingKey.layoutSides) private var layoutSides = ""
    @AppStorage(SettingKey.layoutLandscape) private var layoutLandscape = ""
    @AppStorage(SettingKey.unfoldedSides) private var unfoldedSides = true
    @AppStorage(SettingKey.padTheme) private var padTheme = PadTheme.gold
    @ObservedObject private var photoStore = PadPhotoStore.shared

    /// 내 사진's picture and settings when that is the design and a picture
    /// has been chosen; nil otherwise, and the pad draws its design alone.
    private var photoLook: PadPhotoLook? {
        PadPhotoLook(theme: padTheme, store: photoStore, landscape: sideways || arrangement == .sides)
    }

    /// The palette the pad is drawn in: the design's, or 내 사진's for the way it lays the picture on.
    private var padPalette: PadPalette {
        photoLook?.palette(layout: layout) ?? padTheme.palette
    }

    /// How big the screen is drawn, this title this way up (see `GameScale`).
    @State private var scale = ScreenScale.fit
    /// The area the screen is drawn in, for 화면 배율's limits and pictures.
    @State private var screenArea = CGSize.zero
    /// Whether the play area is wide enough to be unfolded; see `unfoldedMinWidth`.
    @State private var wide = false
    /// Whether the window is on its side: the keys go either side of the
    /// screen and the bar gives way to a button over it.
    @State private var sideways = false
    /// 화면 방향, for this game.
    @State private var orientation = GameOrientation.auto
    @State private var sheet: GameSheet?
    /// This title's pad shape (see `PadShape`).
    @State private var padShape = PadShape.standard
    @State private var speed: Float = 1
    /// How the screen is enlarged (see `GameQuality`).
    @State private var quality = ScreenQuality.dot
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
    /// Whether the window asking before the game closes is up.
    @State private var confirmingExit = false
    /// The game menu's window on screen, nil with the menu closed.
    @State private var menuWindow: MenuWindow?

    /// Whether the pad goes either side of the screen: 펼친 화면 on, on a wide window.
    private var sides: Bool {
        wide && unfoldedSides
    }

    /// Which way up 화면 배율 is kept for: the keys either side of the
    /// screen - on its side, or 펼친 화면 - or under it. The area is a
    /// different shape each way.
    private var scaleSideways: Bool {
        sideways || sides
    }

    /// The title's screen in its own pixels: the frame's, or a handset
    /// panel's before the first.
    private var titlePixels: CGSize {
        guard let frame = emulator.feed.image else { return CGSize(width: 240, height: 320) }
        let doubled = CGFloat(max(emulator.feed.scale, 1))
        return CGSize(width: CGFloat(frame.width) / doubled, height: CGFloat(frame.height) / doubled)
    }

    private var arrangement: PadArrangement {
        padArrangement(mode: padMode, shape: padShape, sides: sides, sideways: sideways)
    }

    private var layout: PadLayout {
        switch arrangement {
        case .below: return PadLayout.decode(layoutBelow, standard: .below)
        case .numbers: return PadLayout.decode(layoutNumbers, standard: .numbers)
        case .overlay: return PadLayout.decode(layoutOverlay, standard: .overlay)
        case .sides: return PadLayout.decode(layoutSides, standard: .sides)
        case .landscape: return PadLayout.decode(layoutLandscape, standard: .landscape)
        }
    }

    var body: some View {
        Group {
            if padHidden && draft == nil {
                fullScreen
            } else if sideways {
                onItsSide
            } else {
                withPad
            }
        }
        .background(Color(white: 0.08).ignoresSafeArea())
        .overlay {
            if let window = menuWindow {
                menu(window)
            }
            if confirmingExit {
                ExitConfirm(onYes: closeGame, onNo: { confirmingExit = false })
            }
        }
        .onChange(of: menuWindow) { _ in updateCapture() }
        .onChange(of: confirmingExit) { _ in updateCapture() }
        .statusBar(hidden: true)
        .onAppear {
            speed = GameSpeed.get(game)
            quality = GameQuality.get(game)
            scale = GameScale.get(game, sideways: scaleSideways)
            touch = GameTouch.get(game)
            padHidden = GamePad.hidden(game)
            padShape = GamePad.shape(game)
            PlayHistory.markPlayed(game)
            OrientationControl.shared.enterGame()
            orientation = OrientationControl.shared.choice
            emulator.start(game: game)
            let emulator = self.emulator
            Gamepad.shared.send = { [weak emulator] key, pressed in
                emulator?.key(key, pressed: pressed)
            }
        }
        .onDisappear {
            Gamepad.shared.releaseAll()
            Gamepad.shared.send = nil
            emulator.stop()
            OrientationControl.shared.leaveGame()
        }
        .onChange(of: quality) { value in emulator.setHq2x(value == .hq2x) }
        // Turned, or laid out the other way, the screen takes the size kept for that way.
        .onChange(of: scaleSideways) { value in scale = GameScale.get(game, sideways: value) }
        .sheet(item: $sheet) { sheet in
            sheetContent(sheet)
        }
    }

    /// What each of the game's sheets shows. Kept out of `body`, which is
    /// as much as the compiler will type-check in one expression.
    @ViewBuilder
    private func sheetContent(_ sheet: GameSheet) -> some View {
        switch sheet {
        case .speed:
            SpeedView(game: game, speed: $speed)
        case .quality:
            QualityView(game: game, emulator: emulator, quality: $quality)
        case .scale:
            ScaleView(game: game, sideways: scaleSideways, area: screenArea, pixels: titlePixels, scale: $scale)
        case .settings:
            GameSettingsView(game: game, padShape: shapeBinding, wide: wide, sideways: sideways, onEditPad: beginEditing)
        case .photo:
            NavigationView {
                PadPhotoSettingsView(landscape: sideways || arrangement == .sides)
                    .toolbar {
                        ToolbarItem(placement: .navigationBarTrailing) {
                            Button("완료") { self.sheet = nil }
                        }
                    }
            }
        case .log:
            LogView()
        }
    }

    /// The pad put away: the screen alone, with the menu on a translucent
    /// button over its corner, as the bar that held it is gone too.
    private var fullScreen: some View {
        ZStack(alignment: .topTrailing) {
            screen(alignment: .center)
                .ignoresSafeArea(edges: .bottom)
            Button {
                menuWindow = .main
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
        .background(widthReader)
    }

    /// The phone on its side, as the Android player lays it: no bar, the
    /// screen filling the height in the middle at its own shape, the keys
    /// either side, and the menu on a button in the corner. Editing puts the
    /// edit bar over the top of the screen, so the keys are placed in the
    /// same area they are played in.
    private var onItsSide: some View {
        ZStack(alignment: .top) {
            GeometryReader { _ in
                ZStack {
                    // 내 사진 lays its pictures in the bands either side of the
                    // screen; the stock designs have no tray on their side.
                    if photoLook != nil {
                        PadTray(palette: padPalette, photo: photoLook, layout: layout, cornerRadius: 0, ground: false)
                    }
                    screen(alignment: .center)
                    pad
                }
            }
            if draft != nil {
                editBar
                    .padding(8)
                    .frame(maxWidth: 300)
                    .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.black.opacity(0.78)))
                    .padding(.top, 4)
            } else {
                HStack {
                    Spacer()
                    Button {
                        menuWindow = .main
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.headline)
                            .foregroundColor(.white)
                            .frame(width: 36, height: 36)
                            .background(Circle().fill(Color.white.opacity(0.18)))
                    }
                }
            }
        }
        .padding(8)
        .background(widthReader)
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
                case .below where arrangement == .sides:
                    // 펼친 화면: the screen in the middle, a key column in each
                    // outer band, all on the tray.
                    ZStack {
                        PadTray(palette: padPalette, photo: photoLook, layout: layout)
                        screen(alignment: .center)
                            .padding(.horizontal, geometry.size.width * PadLayout.sidesBand)
                        pad
                    }
                case .below:
                    VStack(spacing: 8) {
                        screen(alignment: .center)
                        pad
                            .frame(height: geometry.size.height * min(max(padHeight, 0.25), 0.65))
                            .background(PadTray(palette: padPalette, photo: photoLook, layout: layout))
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
        .background(widthReader)
    }

    /// Keeps `wide` up to date with the play area, so turning 펼친 화면 on
    /// or off, or a window resized, lays the pad out again.
    private var widthReader: some View {
        GeometryReader { geometry in
            Color.clear
                .onAppear { measure(geometry.size) }
                .onChange(of: geometry.size) { measure($0) }
        }
    }

    /// Whether the area is wide enough to be unfolded, and whether it is on
    /// its side. An edit in progress is for the way the window was.
    private func measure(_ size: CGSize) {
        wide = size.width >= unfoldedMinWidth
        let turned = size.width > size.height
        if turned != sideways {
            draft = nil
            selectedKey = nil
            sideways = turned
        }
    }

    // MARK: - Bars

    private var topBar: some View {
        HStack {
            Button("닫기") { confirmingExit = true }
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
            Button {
                menuWindow = .main
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
    /// the button over the screen: the APK's own menu, row for row, and the
    /// two windows that pick the pad's design and shape.
    @ViewBuilder
    private func menu(_ window: MenuWindow) -> some View {
        switch window {
        case .main: mainMenu
        case .design: designWindow
        case .shape: shapeWindow
        case .unfolded: unfoldedWindow
        case .orientation: orientationWindow
        }
    }

    private var mainMenu: some View {
        MenuDialog(title: game.title, onDismiss: { menuWindow = nil }) {
            VStack(spacing: 0) {
                MenuRow(symbol: padHidden ? "keyboard" : "keyboard.chevron.compact.down",
                        label: padHidden ? "키패드 꺼내기" : "키패드 숨기기") {
                    menuWindow = nil
                    togglePad()
                }
                MenuRow(symbol: "speedometer", label: "게임 속도", value: GameSpeed.format(speed)) { open(.speed) }
                MenuRow(symbol: "photo", label: "화질", value: quality.label) { open(.quality) }
                MenuRow(symbol: "arrow.up.left.and.arrow.down.right", label: "화면 배율", value: scale.label) { open(.scale) }
                MenuRow(symbol: "rotate.right", label: "화면 방향", value: orientation.label, opens: true) {
                    menuWindow = .orientation
                }
                MenuRow(symbol: "paintpalette", label: "키패드 디자인", value: padTheme.label, opens: true) {
                    menuWindow = .design
                }
                MenuRow(symbol: "circle.grid.3x3", label: "키패드 모양", note: "· 이 게임만", value: padShape.label, opens: true) {
                    menuWindow = .shape
                }
                // Only a wide window has anything for this to change.
                if wide {
                    MenuRow(symbol: "rectangle.split.3x1", label: "펼친 화면", value: unfoldedSides ? "가로 배치" : "세로 배치", opens: true) {
                        menuWindow = .unfolded
                    }
                }
                MenuRow(symbol: "slider.horizontal.3", label: "화면·패드 설정", opens: true) { open(.settings) }
                if !padHidden {
                    MenuRow(symbol: "square.grid.3x3", label: "가상 패드 편집") {
                        menuWindow = nil
                        beginEditing()
                    }
                }
                MenuRow(symbol: touch ? "hand.tap.fill" : "hand.tap", label: "화면 터치",
                        value: touch ? "켜짐" : "꺼짐", valueOff: !touch) {
                    menuWindow = nil
                    toggleTouch()
                }
                MenuRow(symbol: "doc.text.magnifyingglass", label: "로그 보기") { open(.log) }
                // The bar with 닫기 is put away with the pad, and on its side.
                if padHidden || sideways {
                    MenuRow(symbol: "xmark", label: "게임 닫기") {
                        menuWindow = nil
                        confirmingExit = true
                    }
                }
            }
            .padding(.bottom, 6)
        } footer: {
            Spacer()
            Button("닫기") { menuWindow = nil }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
        }
    }

    /// The pad's three designs; a choice applies at once, to every title, and
    /// goes back to the menu.
    private var designWindow: some View {
        MenuDialog(title: "키패드 디자인", onDismiss: { menuWindow = nil }) {
            VStack(alignment: .leading, spacing: 8) {
                Text("모든 게임에 함께 적용됩니다.")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach(PadTheme.allCases) { theme in
                    MenuOption(label: theme.label, on: theme == padTheme) {
                        if theme == .photo, let picture = photoStore.thumbnail {
                            Image(uiImage: picture).resizable().scaledToFill()
                        } else {
                            LinearGradient(colors: [theme.palette.faceTop, theme.palette.faceBottom], startPoint: .top, endPoint: .bottom)
                                .overlay(RoundedRectangle(cornerRadius: 7).stroke(theme.palette.edge ?? MenuPalette.raisedLine, lineWidth: 1))
                        }
                    } action: {
                        padTheme = theme
                        // With no picture yet, 내 사진 goes straight to choosing one.
                        if theme == .photo && !photoStore.hasPicture(sideways || arrangement == .sides) {
                            open(.photo)
                        } else {
                            menuWindow = .main
                        }
                    }
                }
                if padTheme == .photo {
                    MenuRow(symbol: "photo.on.rectangle", label: "내 사진 설정", opens: true) {
                        open(.photo)
                    }
                }
            }
            .padding(.horizontal, 22)
            .padding(.bottom, 18)
        } footer: {
            Spacer()
            Button("뒤로") { menuWindow = .main }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
        }
    }

    /// The pad's shapes, for this title alone.
    private var shapeWindow: some View {
        MenuDialog(title: "키패드 모양", onDismiss: { menuWindow = nil }) {
            VStack(alignment: .leading, spacing: 8) {
                Text("이 게임에만 적용됩니다.")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach(PadShape.allCases) { shape in
                    MenuOption(label: shape.label, on: shape == padShape) {
                        ZStack {
                            MenuPalette.raised
                            Image(systemName: shape == .numbers ? "textformat.123" : "circle.grid.3x3")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundColor(MenuPalette.accent)
                        }
                    } action: {
                        shapeBinding.wrappedValue = shape
                        menuWindow = .main
                    }
                }
            }
            .padding(.horizontal, 22)
            .padding(.bottom, 18)
        } footer: {
            Spacer()
            Button("뒤로") { menuWindow = .main }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
        }
    }

    /// 펼친 화면: 가로 배치 or 세로 배치, for every title; a choice applies at once.
    private var unfoldedWindow: some View {
        MenuDialog(title: "펼친 화면", onDismiss: { menuWindow = nil }) {
            VStack(alignment: .leading, spacing: 8) {
                Text("모든 게임에 적용됩니다.")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach([true, false], id: \.self) { side in
                    MenuOption(label: side ? "가로 배치 · 키를 양옆에" : "세로 배치 · 키패드를 아래에", on: side == unfoldedSides) {
                        UnfoldedLayoutPicture(side: side)
                    } action: {
                        unfoldedSides = side
                        menuWindow = .main
                    }
                }
            }
            .padding(.horizontal, 22)
            .padding(.bottom, 18)
        } footer: {
            Spacer()
            Button("뒤로") { menuWindow = .main }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
        }
    }

    /// 화면 방향: following the phone, or held upright or on its side, for
    /// this game; a choice applies at once.
    private var orientationWindow: some View {
        MenuDialog(title: "화면 방향", onDismiss: { menuWindow = nil }) {
            VStack(alignment: .leading, spacing: 8) {
                Text("이 게임을 하는 동안 적용됩니다.")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach(GameOrientation.allCases) { choice in
                    MenuOption(label: "\(choice.label) · \(choice.detail)", on: choice == orientation) {
                        ZStack {
                            MenuPalette.raised
                            Image(systemName: choice == .auto ? "arrow.triangle.2.circlepath" : choice == .portrait ? "iphone" : "iphone.landscape")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundColor(MenuPalette.accent)
                        }
                    } action: {
                        orientation = choice
                        OrientationControl.shared.choose(choice)
                        menuWindow = nil
                    }
                }
            }
            .padding(.horizontal, 22)
            .padding(.bottom, 18)
        } footer: {
            Spacer()
            Button("뒤로") { menuWindow = .main }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
        }
    }

    /// Closes the menu for one of the game's sheets.
    private func open(_ next: GameSheet) {
        menuWindow = nil
        sheet = next
    }

    private func closeGame() {
        confirmingExit = false
        Gamepad.shared.capture = nil
        emulator.stop()
        // Upright before the list comes back, rather than the list turning
        // after it has.
        OrientationControl.shared.leaveGame()
        dismiss()
    }

    /// While a menu window or 종료 is up a gamepad answers it rather than the
    /// game: B goes back (or 아니요), A says 예. Keys held into it are let go.
    private func updateCapture() {
        if confirmingExit {
            Gamepad.shared.releaseAll()
            Gamepad.shared.capture = { button in
                switch button {
                case .a?: closeGame()
                case .b?: confirmingExit = false
                default: break
                }
            }
        } else if menuWindow != nil {
            Gamepad.shared.releaseAll()
            Gamepad.shared.capture = { button in
                guard button == .b else { return }
                menuWindow = menuWindow == .main ? nil : .main
            }
        } else {
            Gamepad.shared.capture = nil
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
                Text("패드 편집 · \(arrangementName(arrangement, mode: padMode))")
                    .font(.headline)
                Spacer()
                Button("저장") {
                    if let draft {
                        switch arrangement {
                        case .below: layoutBelow = draft.encoded()
                        case .numbers: layoutNumbers = draft.encoded()
                        case .overlay: layoutOverlay = draft.encoded()
                        case .sides: layoutSides = draft.encoded()
                        case .landscape: layoutLandscape = draft.encoded()
                        }
                    }
                    draft = nil
                    selectedKey = nil
                }
                .font(.body.weight(.bold))
            }
            HStack(spacing: 8) {
                Button("기본 배치") {
                    draft = PadLayout.standard(arrangement)
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
                    .onAppear { screenArea = geometry.size }
                    .onChange(of: geometry.size) { screenArea = $0 }
                GameScreen(feed: emulator.feed, scale: scale, quality: quality, area: geometry.size, touch: touch) { pixels, shown in
                    screenTouch(frame: pixels, shown: shown)
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
                labelScale: labelScale,
                palette: padTheme.palette
            )
        } else {
            PadView(
                layout: layout,
                opacity: padMode == .below || arrangement == .landscape ? opacityBelow : opacityOverlay,
                labelScale: labelScale,
                haptics: haptics,
                palette: padPalette,
                letters: arrangement != .numbers,
                photo: photoLook
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
    private func screenTouch(frame: CGSize, shown: CGSize) -> AnyGesture<DragGesture.Value> {
        AnyGesture(DragGesture(minimumDistance: 0)
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
            })
    }

    /// This title's pad shape, kept as it changes.
    private var shapeBinding: Binding<PadShape> {
        Binding(get: { padShape }, set: { shape in
            padShape = shape
            GamePad.setShape(shape, for: game)
        })
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

/// The title's frame, sized into the area it is given. It watches the frames
/// itself, so a new one redraws this and nothing around it.
private struct GameScreen: View {
    @ObservedObject var feed: FrameFeed
    let scale: ScreenScale
    let quality: ScreenQuality
    let area: CGSize
    let touch: Bool
    /// The touches on the screen, for a frame of so many pixels shown at so
    /// many points.
    let gesture: (CGSize, CGSize) -> AnyGesture<DragGesture.Value>

    var body: some View {
        if let frame = feed.image {
            // The title's own pixels, however many of the frame's each takes
            // once doubled through hq2x.
            let pixels = CGSize(
                width: CGFloat(frame.width) / CGFloat(feed.scale),
                height: CGFloat(frame.height) / CGFloat(feed.scale)
            )
            let size = scale.size(of: pixels, in: area)
            Image(decorative: frame, scale: 1)
                .interpolation(quality == .dot ? .none : .high)
                .antialiased(quality != .dot)
                .resizable()
                .frame(width: size.width, height: size.height)
                .gesture(gesture(pixels, size), including: touch ? .all : .none)
        }
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
/// 0.1x to 4x, and the common speeds as chips. Nothing changes until 적용,
/// which keeps it for this title; 1x로 goes straight back to real time.
private struct SpeedView: View {
    let game: GameFile
    @Binding var speed: Float
    @Environment(\.dismiss) private var dismiss
    @State private var chosen: Float

    init(game: GameFile, speed: Binding<Float>) {
        self.game = game
        _speed = speed
        _chosen = State(initialValue: speed.wrappedValue)
    }

    private var tenths: Int {
        Int((chosen * 10).rounded())
    }

    var body: some View {
        MenuPanel(title: "게임 속도") {
            VStack(spacing: 0) {
                HStack(spacing: 18) {
                    step("minus", by: -1)
                    Text(GameSpeed.format(chosen))
                        .font(.system(size: 44, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundColor(MenuPalette.accent)
                        .frame(minWidth: 130)
                    step("plus", by: 1)
                }
                .frame(maxWidth: .infinity)
                Text("이 게임에만 적용 · 다음에 열어도 유지")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                    .padding(.top, 4)
                    .padding(.bottom, 10)
                SpeedRuler(tenths: Binding(get: { tenths }, set: { chosen = Float($0) / 10 }))
                    .frame(height: 58)
                HStack(spacing: 5) {
                    ForEach(GameSpeed.chips, id: \.self) { value in
                        Button(GameSpeed.format(value)) {
                            withAnimation(.easeOut(duration: 0.25)) { chosen = value }
                        }
                        .buttonStyle(SpeedChipStyle(on: chosen == value))
                    }
                }
                .padding(.top, 8)
                Text("줄자를 옆으로 밀거나 −/+ 로 0.1씩 맞출 수 있어요. 빠르게 하면 게임 시간이 그만큼 빨리 흐르고, 무거운 장면에선 폰 성능만큼만 빨라질 수 있어요. 소리는 원래 속도로 재생됩니다.")
                    .font(.system(size: 12))
                    .foregroundColor(MenuPalette.muted)
                    .lineSpacing(3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 14)
            }
        } footer: {
            Button("1x로") {
                apply(1)
                dismiss()
            }
            .buttonStyle(MenuButtonStyle(kind: .quiet))
            Spacer()
            Button("취소") { dismiss() }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
            Button("적용") {
                apply(chosen)
                dismiss()
            }
            .buttonStyle(MenuButtonStyle(kind: .primary))
        }
    }

    /// Keeps `value` for this title and puts the running one on it.
    private func apply(_ value: Float) {
        speed = value
        GameSpeed.set(value, for: game)
        wie_set_speed(value)
    }

    /// A round − or +: a tenth slower or faster.
    private func step(_ symbol: String, by change: Int) -> some View {
        Button {
            let next = min(max(tenths + change, GameSpeed.tenths.lowerBound), GameSpeed.tenths.upperBound)
            withAnimation(.easeOut(duration: 0.2)) { chosen = Float(next) / 10 }
        } label: {
            Image(systemName: symbol)
                .font(.title3.weight(.semibold))
                .foregroundColor(MenuPalette.ink)
                .frame(width: 40, height: 40)
                .overlay(Circle().stroke(MenuPalette.outline))
        }
        .buttonStyle(.borderless)
    }
}

/// A common speed under the ruler: filled when it is the one chosen.
private struct SpeedChipStyle: ButtonStyle {
    let on: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13.5, weight: .bold))
            .foregroundColor(on ? MenuPalette.onAccent : MenuPalette.ink)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .background(Capsule().fill(on ? MenuPalette.accent : Color.clear))
            .overlay(Capsule().stroke(on ? MenuPalette.accent : MenuPalette.outline, lineWidth: 1))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// The speed ruler: a tick for every tenth from 0.1x to 4x, slid sideways
/// under a fixed needle. A finger drags it, a flick throws it on, a tap brings
/// the tick tapped under the needle, and it always comes to rest on a tick.
private struct SpeedRuler: View {
    @Binding var tenths: Int
    /// The ruler's ends, in its ticks: tenths of a speed, or percents for
    /// 화면 배율.
    var range: ClosedRange<Int> = GameSpeed.tenths
    var spacing: CGFloat = 12
    /// A tick's label, nil for none: the speed every half, and at the slow end.
    var label: (Int) -> String? = { value in
        value % 5 == 0 || value == GameSpeed.tenths.lowerBound ? GameSpeed.format(Float(value) / 10) : nil
    }
    /// How far the finger has carried the ruler past the tick it is on.
    @State private var carried: CGFloat = 0
    /// The tick under the needle when the finger came down.
    @State private var startTenths: Int?


    var body: some View {
        GeometryReader { geometry in
            let centre = geometry.size.width / 2
            ZStack(alignment: .topLeading) {
                ForEach(range, id: \.self) { value in
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
            if let text = label(value) {
                Text(text)
                    .font(.system(size: 11, weight: whole ? .semibold : .regular))
                    .foregroundColor(whole ? .primary : .secondary)
                    .fixedSize()
            }
        }
        .frame(width: 34, height: 50, alignment: .top)
    }

    private func clamp(_ value: Int) -> Int {
        min(max(value, range.lowerBound), range.upperBound)
    }
}

/// How the screen is enlarged: 기본, 도트 or HQ2X, each with a picture of the
/// middle of the screen drawn that way. A choice shows on the screen behind at
/// once; 적용 keeps it for this title, or every title, and anything else puts
/// the screen back.
private struct QualityView: View {
    let game: GameFile
    let emulator: Emulator
    @Binding var quality: ScreenQuality
    @Environment(\.dismiss) private var dismiss
    @State private var everyGame = false
    @State private var previews: QualityPreviews?
    @State private var saved: ScreenQuality
    @State private var applied = false

    init(game: GameFile, emulator: Emulator, quality: Binding<ScreenQuality>) {
        self.game = game
        self.emulator = emulator
        _quality = quality
        _saved = State(initialValue: quality.wrappedValue)
    }

    var body: some View {
        MenuPanel(title: "화질") {
            VStack(alignment: .leading, spacing: 8) {
                Text("이 게임에만 적용 · 다음에 열어도 유지")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach(ScreenQuality.allCases) { option in
                    Button {
                        quality = option
                    } label: {
                        card(option)
                    }
                    .buttonStyle(.plain)
                }
                Button {
                    everyGame.toggle()
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: everyGame ? "checkmark.square.fill" : "square")
                            .foregroundColor(MenuPalette.accent)
                        Text("모든 게임에 이 화질 쓰기")
                            .font(.system(size: 12.5))
                            .foregroundColor(MenuPalette.muted)
                    }
                }
                .buttonStyle(.plain)
                .padding(.top, 2)
            }
        } footer: {
            Spacer()
            Button("취소") { dismiss() }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
            Button("적용") {
                applied = true
                GameQuality.set(quality, for: game, everyGame: everyGame)
                dismiss()
            }
            .buttonStyle(MenuButtonStyle(kind: .primary))
        }
        .onAppear { previews = QualityPreviews(emulator.lastFrame()) }
        // However the window goes away without 적용, the screen goes back.
        .onDisappear {
            if !applied {
                quality = saved
            }
        }
    }

    /// One way to draw the screen: its picture, name and what it does, and
    /// whether it is the one chosen.
    private func card(_ option: ScreenQuality) -> some View {
        let on = quality == option
        return HStack(spacing: 11) {
            if let previews {
                previews.image(option)
                    .frame(width: 84, height: 63)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            }
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(option.label)
                        .font(.system(size: 15, weight: .bold))
                        .foregroundColor(MenuPalette.ink)
                    if option == .dot {
                        Text("지금 방식")
                            .font(.system(size: 11))
                            .foregroundColor(MenuPalette.muted)
                    }
                }
                Text(option.detail)
                    .font(.system(size: 12))
                    .foregroundColor(MenuPalette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Circle()
                .fill(on ? MenuPalette.accent : Color.clear)
                .overlay(Circle().stroke(on ? MenuPalette.accent : MenuPalette.outline, lineWidth: 2))
                .frame(width: 18, height: 18)
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 10).fill(on ? MenuPalette.accent.opacity(0.1) : Color.clear))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(on ? MenuPalette.accent : MenuPalette.raisedLine, lineWidth: 1))
        .contentShape(Rectangle())
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

/// 화면 배율: 맞춤, 정수배 and 직접 조절, each with a picture of where the
/// screen sits in its area. 직접 조절 opens a ruler for the width and one for
/// the height, moving together until they are let go of each other. It is
/// kept for this title the way the screen is laid out now - under the keys,
/// or with them either side - as the area is a different shape each way. A
/// choice shows on the screen behind at once; 적용 keeps it, and anything
/// else puts the screen back.
private struct ScaleView: View {
    let game: GameFile
    let sideways: Bool
    let area: CGSize
    let pixels: CGSize
    @Binding var scale: ScreenScale
    @Environment(\.dismiss) private var dismiss
    @State private var everyGame = false
    @State private var saved: ScreenScale
    @State private var applied = false

    init(game: GameFile, sideways: Bool, area: CGSize, pixels: CGSize, scale: Binding<ScreenScale>) {
        self.game = game
        self.sideways = sideways
        self.area = area
        self.pixels = pixels
        _scale = scale
        _saved = State(initialValue: scale.wrappedValue)
    }

    private var limits: (width: Int, height: Int) {
        ScreenScale.limits(of: pixels, in: area)
    }

    /// Where each ruler ends. Linked, both stop at the nearer edge, so the
    /// shape holds all along either of them.
    private var widthEnd: Int {
        scale.linked ? min(limits.width, limits.height) : limits.width
    }

    private var heightEnd: Int {
        scale.linked ? min(limits.width, limits.height) : limits.height
    }

    var body: some View {
        MenuPanel(title: "화면 배율") {
            VStack(alignment: .leading, spacing: 8) {
                Text("이 게임의 \(sideways ? "가로" : "세로") 화면에만 적용 · 다음에 열어도 유지")
                    .font(.system(size: 12.5))
                    .foregroundColor(MenuPalette.muted)
                ForEach(ScaleMode.allCases) { mode in
                    Button {
                        scale.mode = mode
                        keepOnRulers()
                    } label: {
                        card(mode)
                    }
                    .buttonStyle(.plain)
                }
                if scale.mode == .custom {
                    customPanel
                }
                Button {
                    everyGame.toggle()
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: everyGame ? "checkmark.square.fill" : "square")
                            .foregroundColor(MenuPalette.accent)
                        Text("모든 게임에 이 배율 쓰기")
                            .font(.system(size: 12.5))
                            .foregroundColor(MenuPalette.muted)
                    }
                }
                .buttonStyle(.plain)
                .padding(.top, 2)
            }
        } footer: {
            Spacer()
            Button("취소") { dismiss() }
                .buttonStyle(MenuButtonStyle(kind: .secondary))
            Button("적용") {
                applied = true
                GameScale.set(scale, for: game, sideways: sideways, everyGame: everyGame)
                dismiss()
            }
            .buttonStyle(MenuButtonStyle(kind: .primary))
        }
        .onAppear(perform: keepOnRulers)
        // However the window goes away without 적용, the screen goes back.
        .onDisappear {
            if !applied {
                scale = saved
            }
        }
    }

    /// 직접 조절's two sides, each with its ruler, and the switch that ties
    /// them together.
    private var customPanel: some View {
        VStack(spacing: 8) {
            Toggle(isOn: Binding(get: { scale.linked }, set: { on in
                scale.linked = on
                // Linking puts the height on the width.
                if on {
                    scale.height = scale.width
                }
                keepOnRulers()
            })) {
                Text("가로·세로 같이 움직이기")
                    .font(.system(size: 13.5))
                    .foregroundColor(MenuPalette.ink)
            }
            .toggleStyle(SwitchToggleStyle(tint: MenuPalette.accent))
            axis("가로", value: side(\.width), end: widthEnd)
            axis("세로", value: side(\.height), end: heightEnd)
            Button("맞춤 크기로 되돌리기") {
                withAnimation(.easeOut(duration: 0.25)) {
                    scale.width = 100
                    scale.height = 100
                }
            }
            .font(.system(size: 12.5, weight: .bold))
            .foregroundColor(MenuPalette.accent)
            .buttonStyle(.plain)
            .padding(.top, 2)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.black.opacity(0.24)))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(MenuPalette.raisedLine, lineWidth: 1))
    }

    /// One side's percent, taking the other with it while they are linked.
    private func side(_ path: WritableKeyPath<ScreenScale, Int>) -> Binding<Int> {
        Binding(get: { scale[keyPath: path] }, set: { value in
            scale[keyPath: path] = value
            if scale.linked {
                scale.width = value
                scale.height = value
            }
        })
    }

    /// Brings both sides within their rulers' ends.
    private func keepOnRulers() {
        scale.width = min(max(scale.width, ScreenScale.smallest), widthEnd)
        scale.height = min(max(scale.height, ScreenScale.smallest), heightEnd)
    }

    /// A side's name and range, − its percent +, and its ruler. − and + move
    /// it a percent.
    private func axis(_ name: String, value: Binding<Int>, end: Int) -> some View {
        VStack(spacing: 2) {
            HStack(spacing: 8) {
                Text(name)
                    .font(.system(size: 14, weight: .bold))
                    .foregroundColor(MenuPalette.ink)
                Text("\(ScreenScale.smallest)~\(end)%")
                    .font(.system(size: 11.5))
                    .foregroundColor(MenuPalette.muted)
                Spacer(minLength: 4)
                step("minus", value: value, by: -1, end: end)
                Text("\(value.wrappedValue)%")
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .monospacedDigit()
                    .foregroundColor(MenuPalette.accent)
                    .frame(minWidth: 68)
                step("plus", value: value, by: 1, end: end)
            }
            SpeedRuler(tenths: value, range: ScreenScale.smallest...end, spacing: 6) { tick in
                // Every ten, the ends - unless the far one would sit on the last ten's label.
                tick % 10 == 0 || tick == ScreenScale.smallest || (tick == end && end % 10 >= 6) ? "\(tick)%" : nil
            }
            .frame(height: 58)
        }
    }

    private func step(_ symbol: String, value: Binding<Int>, by change: Int, end: Int) -> some View {
        Button {
            let next = min(max(value.wrappedValue + change, ScreenScale.smallest), end)
            withAnimation(.easeOut(duration: 0.2)) { value.wrappedValue = next }
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundColor(MenuPalette.ink)
                .frame(width: 32, height: 32)
                .overlay(Circle().stroke(MenuPalette.outline))
        }
        .buttonStyle(.borderless)
    }

    /// One way to size the screen: a picture of it in its area, its name and
    /// what it does, and whether it is the one chosen.
    private func card(_ mode: ScaleMode) -> some View {
        let on = scale.mode == mode
        return HStack(spacing: 11) {
            picture(mode)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(mode.label)
                        .font(.system(size: 15, weight: .bold))
                        .foregroundColor(MenuPalette.ink)
                    if mode == .fit {
                        Text("지금 방식")
                            .font(.system(size: 11))
                            .foregroundColor(MenuPalette.muted)
                    }
                }
                Text(mode.detail)
                    .font(.system(size: 12))
                    .foregroundColor(MenuPalette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
            Circle()
                .fill(on ? MenuPalette.accent : Color.clear)
                .overlay(Circle().stroke(on ? MenuPalette.accent : MenuPalette.outline, lineWidth: 2))
                .frame(width: 18, height: 18)
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 10).fill(on ? MenuPalette.accent.opacity(0.1) : Color.clear))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(on ? MenuPalette.accent : MenuPalette.raisedLine, lineWidth: 1))
        .contentShape(Rectangle())
    }

    /// The area, outlined, and the screen in it at `mode` - 직접 조절 at the
    /// sizes on its rulers.
    private func picture(_ mode: ScaleMode) -> some View {
        let room = CGSize(width: max(area.width, 1), height: max(area.height, 1))
        let shown = ScreenScale(mode: mode, width: scale.width, height: scale.height, linked: scale.linked).size(of: pixels, in: room)
        let fit = min(34 / room.width, 42 / room.height)
        return ZStack {
            Color(red: 13 / 255, green: 14 / 255, blue: 17 / 255)
            Rectangle()
                .stroke(MenuPalette.outline, lineWidth: 1)
                .frame(width: room.width * fit, height: room.height * fit)
            Rectangle()
                .fill(MenuPalette.accent)
                .frame(width: shown.width * fit, height: shown.height * fit)
        }
        .frame(width: 40, height: 48)
        .clipShape(RoundedRectangle(cornerRadius: 6))
    }
}

/// What an arrangement is called in the pad editor and the layout storage.
private func arrangementName(_ arrangement: PadArrangement, mode: PadMode) -> String {
    switch arrangement {
    case .numbers: return "숫자 크게"
    case .sides: return "펼친 화면"
    case .landscape: return "가로 화면"
    default: return mode.label
    }
}

/// A small picture of a wide screen laid out with the keys either side, or under the screen.
private struct UnfoldedLayoutPicture: View {
    let side: Bool

    var body: some View {
        GeometryReader { geometry in
            let w = geometry.size.width, h = geometry.size.height
            ZStack(alignment: .topLeading) {
                Color(red: 0.03, green: 0.035, blue: 0.045)
                if side {
                    Rectangle().fill(Color(red: 0.16, green: 0.23, blue: 0.35))
                        .frame(width: w * 0.4, height: h * 0.8).offset(x: w * 0.3, y: h * 0.1)
                    ForEach(0..<2, id: \.self) { column in
                        ForEach(0..<6, id: \.self) { key in
                            RoundedRectangle(cornerRadius: 1.5).fill(MenuPalette.outline)
                                .frame(width: w * 0.09, height: h * 0.16)
                                .offset(x: (column == 0 ? w * 0.04 : w * 0.76) + CGFloat(key % 2) * w * 0.11,
                                        y: h * 0.18 + CGFloat(key / 2) * h * 0.22)
                        }
                    }
                } else {
                    Rectangle().fill(Color(red: 0.16, green: 0.23, blue: 0.35))
                        .frame(width: w * 0.44, height: h * 0.44).offset(x: w * 0.28, y: h * 0.06)
                    ForEach(0..<12, id: \.self) { key in
                        RoundedRectangle(cornerRadius: 1.5).fill(MenuPalette.outline)
                            .frame(width: w * 0.12, height: h * 0.14)
                            .offset(x: w * 0.06 + CGFloat(key % 6) * w * 0.15, y: h * 0.56 + CGFloat(key / 6) * h * 0.2)
                    }
                }
            }
        }
    }
}

/// Screen scaling and the pad's look, applied as they change.
private struct GameSettingsView: View {
    let game: GameFile
    @Binding var padShape: PadShape
    /// Whether the play area is wide enough for 펼친 화면 to apply.
    let wide: Bool
    /// Whether the phone is on its side, which has its own layout.
    let sideways: Bool
    let onEditPad: () -> Void

    @Environment(\.dismiss) private var dismiss
    @AppStorage(SettingKey.padMode) private var padMode = PadMode.below
    @AppStorage(SettingKey.padHeight) private var padHeight = 0.42
    @AppStorage(SettingKey.opacityBelow) private var opacityBelow = 1.0
    @AppStorage(SettingKey.opacityOverlay) private var opacityOverlay = 0.55
    @AppStorage(SettingKey.labelScale) private var labelScale = 1.0
    @AppStorage(SettingKey.haptics) private var haptics = true
    @AppStorage(SettingKey.layoutBelow) private var layoutBelow = ""
    @AppStorage(SettingKey.layoutOverlay) private var layoutOverlay = ""
    @AppStorage(SettingKey.layoutNumbers) private var layoutNumbers = ""
    @AppStorage(SettingKey.layoutSides) private var layoutSides = ""
    @AppStorage(SettingKey.layoutLandscape) private var layoutLandscape = ""
    @AppStorage(SettingKey.unfoldedSides) private var unfoldedSides = true
    @AppStorage(SettingKey.padTheme) private var padTheme = PadTheme.gold
    @ObservedObject private var photoStore = PadPhotoStore.shared

    private var arrangement: PadArrangement {
        padArrangement(mode: padMode, shape: padShape, sides: wide && unfoldedSides, sideways: sideways)
    }

    private var arrangementLabel: String {
        arrangementName(arrangement, mode: padMode)
    }

    var body: some View {
        NavigationView {
            Form {
                Section {
                    designs
                    if padTheme == .photo {
                        NavigationLink {
                            PadPhotoSettingsView(landscape: arrangement == .sides || arrangement == .landscape)
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("내 사진 설정")
                                Text(photoStore.hasPicture(arrangement == .sides || arrangement == .landscape) ? "사진·방식 바꾸기" : "사진을 골라 주세요")
                                    .font(.caption).foregroundColor(.secondary)
                            }
                        }
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("위치").font(.caption).foregroundColor(.secondary)
                        Picker("위치", selection: $padMode) {
                            ForEach(PadMode.allCases) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("모양 · 이 게임(\(game.title))에만").font(.caption).foregroundColor(.secondary).lineLimit(1)
                        Picker("모양", selection: $padShape) {
                            ForEach(PadShape.allCases) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .disabled(padMode == .overlay)
                    }
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
                } footer: {
                    Text("디자인은 모든 게임에 함께 적용됩니다. ‘숫자 크게’는 리듬게임처럼 숫자키를 많이 쓰는 게임용으로, 화면 아래 위치에서 쓰고 게임마다 따로 기억됩니다.")
                }

                Section {
                    Picker("펼친 화면", selection: $unfoldedSides) {
                        Text("가로 배치").tag(true)
                        Text("세로 배치").tag(false)
                    }
                    .pickerStyle(.segmented)
                } header: {
                    Text("펼친 화면")
                } footer: {
                    Text("아이패드처럼 넓은 화면에서 가로 배치는 게임 화면을 가운데, 키를 양옆에 둡니다. 세로 배치는 화면 아래에 키패드를 둡니다. 패드 위치가 ‘화면 아래’일 때 쓰이고, 모든 게임에 적용됩니다.")
                }

                Section {
                    Button("키 위치·크기 편집") {
                        dismiss()
                        onEditPad()
                    }
                    NavigationLink {
                        PadLayoutStorageView(arrangementLabel: arrangementLabel, resetCurrent: resetLayout)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("배치 저장·백업·초기화")
                            Text("저장 · 불러오기 · 파일 백업·복원 · 초기화").font(.caption).foregroundColor(.secondary)
                        }
                    }
                } header: {
                    Text("배치")
                } footer: {
                    Text("편집은 지금 배치(\(arrangementLabel))에만 적용됩니다. 배치마다 따로 저장됩니다.")
                }

                Section {
                    NavigationLink("게임패드 버튼 배치") {
                        PadMappingView()
                    }
                } header: {
                    Text("게임패드")
                } footer: {
                    Text("블루투스나 USB로 연결한 게임패드의 버튼마다 누를 폰 키를 정합니다. 모든 게임에 함께 적용됩니다.")
                }
            }
            .navigationTitle("화면·패드 설정")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("완료") { dismiss() }
                }
            }
        }
    }

    /// Puts the arrangement in use back to its standard layout.
    private func resetLayout() {
        switch arrangement {
        case .below: layoutBelow = ""
        case .numbers: layoutNumbers = ""
        case .overlay: layoutOverlay = ""
        case .sides: layoutSides = ""
        case .landscape: layoutLandscape = ""
        }
    }

    /// The designs as small pictures of the pad itself; a tap applies one.
    private var designs: some View {
        HStack(spacing: 8) {
            ForEach(PadTheme.allCases) { theme in
                let chosen = theme == padTheme
                let look = PadPhotoLook(theme: theme, store: photoStore, landscape: false)
                let palette = look?.palette(layout: PadLayout.standard(.below)) ?? theme.palette
                Button {
                    padTheme = theme
                } label: {
                    VStack(spacing: 6) {
                        GeometryReader { geometry in
                            PadFaces(layout: PadLayout.standard(.below), size: geometry.size, pressed: [],
                                     labelScale: 0.7, palette: palette, letters: false, photo: look)
                        }
                        .frame(height: 84)
                        .background(PadTray(palette: palette, photo: look, layout: PadLayout.standard(.below), cornerRadius: 0))
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .stroke(chosen ? Color.accentColor : Color.clear, lineWidth: 2)
                        )
                        Text(theme.label)
                            .font(.caption.weight(.semibold))
                            .foregroundColor(chosen ? .accentColor : .primary)
                    }
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, 4)
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
