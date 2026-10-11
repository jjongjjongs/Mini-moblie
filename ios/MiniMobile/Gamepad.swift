import GameController
import SwiftUI

/// A gamepad button the mapping covers, in the order the drawing and the
/// other builds list them. SELECT is not one: held, it turns the buttons to
/// their second layer.
enum PadButton: Int, CaseIterable, Identifiable {
    case up, down, left, right, a, b, x, y, l1, r1, l2, r2, l3, r3, start

    var id: Int { rawValue }

    var name: String {
        ["D▲", "D▼", "D◀", "D▶", "A", "B", "X", "Y", "L1", "R1", "L2", "R2", "L3", "R3", "START"][rawValue]
    }

    /// Where it sits on the drawing, on a 320-unit-wide plan: x, y, width,
    /// height and how round, as the PC and handheld builds place it.
    var place: (x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat, r: CGFloat) {
        switch self {
        case .up: return (51, 32, 29, 23, 5)
        case .down: return (51, 80, 29, 23, 5)
        case .left: return (20, 56, 29, 23, 5)
        case .right: return (82, 56, 29, 23, 5)
        case .a: return (268, 54, 35, 28, 14)
        case .b: return (237, 78, 35, 28, 14)
        case .x: return (237, 30, 35, 28, 14)
        case .y: return (206, 54, 35, 28, 14)
        case .l1: return (82, 4, 56, 22, 6)
        case .r1: return (182, 4, 56, 22, 6)
        case .l2: return (22, 4, 56, 22, 6)
        case .r2: return (242, 4, 56, 22, 6)
        case .l3: return (116, 70, 37, 37, 18.5)
        case .r3: return (167, 70, 37, 37, 18.5)
        case .start: return (161, 39, 48, 20, 10)
        }
    }
}

/// What a handset key is called on a gamepad button.
func handsetKeyLabel(_ key: Int32?) -> String {
    guard let key = key else { return "없음" }
    switch key {
    case Int32(WIE_KEY_UP): return "▲"
    case Int32(WIE_KEY_DOWN): return "▼"
    case Int32(WIE_KEY_LEFT): return "◀"
    case Int32(WIE_KEY_RIGHT): return "▶"
    case Int32(WIE_KEY_OK): return "확인"
    case Int32(WIE_KEY_LEFT_SOFT): return "좌소프트"
    case Int32(WIE_KEY_RIGHT_SOFT): return "우소프트"
    case Int32(WIE_KEY_CLEAR): return "취소"
    case Int32(WIE_KEY_STAR): return "*"
    case Int32(WIE_KEY_HASH): return "#"
    case Int32(WIE_KEY_CALL): return "통화"
    case Int32(WIE_KEY_HANGUP): return "종료"
    case Int32(WIE_KEY_NUM0)...Int32(WIE_KEY_NUM0) + 9: return String(key - Int32(WIE_KEY_NUM0))
    default: return "?"
    }
}

/// Which handset key each gamepad button presses: plain, and with SELECT
/// held. A button with nothing in the second layer presses its plain key
/// there too.
struct PadMapping: Codable, Equatable {
    var plain: [Int: Int32]
    var select: [Int: Int32]

    private static let storageKey = "gamepadMapping"

    /// The pad's defaults, the same as the PC and handheld builds': the D-pad
    /// on the directions, A on confirm, B on back, X and Y on * and #, each
    /// shoulder and trigger on the soft key on its side, START on confirm.
    /// With SELECT held the buttons are the number pad.
    static let standard: PadMapping = {
        let num = { (digit: Int32) in Int32(WIE_KEY_NUM0) + digit }
        let plain: [PadButton: Int32] = [
            .up: Int32(WIE_KEY_UP), .down: Int32(WIE_KEY_DOWN), .left: Int32(WIE_KEY_LEFT), .right: Int32(WIE_KEY_RIGHT),
            .a: Int32(WIE_KEY_OK), .b: Int32(WIE_KEY_CLEAR), .x: Int32(WIE_KEY_STAR), .y: Int32(WIE_KEY_HASH),
            .l1: Int32(WIE_KEY_LEFT_SOFT), .r1: Int32(WIE_KEY_RIGHT_SOFT),
            .l2: Int32(WIE_KEY_LEFT_SOFT), .r2: Int32(WIE_KEY_RIGHT_SOFT),
            .start: Int32(WIE_KEY_OK),
        ]
        let select: [PadButton: Int32] = [
            .up: num(2), .down: num(8), .left: num(4), .right: num(6),
            .a: num(5), .b: num(0), .x: num(1), .y: num(3),
            .l1: num(7), .r1: num(9), .l2: Int32(WIE_KEY_STAR), .r2: Int32(WIE_KEY_HASH),
        ]
        return PadMapping(
            plain: Dictionary(uniqueKeysWithValues: plain.map { ($0.key.rawValue, $0.value) }),
            select: Dictionary(uniqueKeysWithValues: select.map { ($0.key.rawValue, $0.value) })
        )
    }()

    static func load() -> PadMapping {
        guard let data = UserDefaults.standard.data(forKey: storageKey),
              let mapping = try? JSONDecoder().decode(PadMapping.self, from: data)
        else { return .standard }
        return mapping
    }

    func save() {
        if let data = try? JSONEncoder().encode(self) {
            UserDefaults.standard.set(data, forKey: Self.storageKey)
        }
    }

    /// The key `button` presses, SELECT held or not.
    func key(_ button: PadButton, select held: Bool) -> Int32? {
        if held, let key = select[button.rawValue] {
            return key
        }
        return plain[button.rawValue]
    }
}

/// The game controllers connected, read through the mapping into handset
/// keys for the title running.
final class Gamepad: ObservableObject {
    static let shared = Gamepad()

    /// The first pad's name, nil with none connected.
    @Published private(set) var connected: String?
    @Published var mapping = PadMapping.load() {
        didSet { mapping.save() }
    }

    /// Where the keys go: the title running, nil with none.
    var send: ((Int32, Bool) -> Void)?
    /// While the mapping screen is up, a button pressed goes here rather than
    /// to the title - nil for SELECT.
    var capture: ((PadButton?) -> Void)?

    /// The keys held down, by the input that pressed them, so a release lets
    /// go of what its press pressed even if the mapping or SELECT changed.
    private var held: [Int: Int32] = [:]
    private var selectHeld = false

    private init() {
        let center = NotificationCenter.default
        center.addObserver(forName: .GCControllerDidConnect, object: nil, queue: .main) { [weak self] note in
            if let controller = note.object as? GCController {
                self?.configure(controller)
            }
        }
        center.addObserver(forName: .GCControllerDidDisconnect, object: nil, queue: .main) { [weak self] _ in
            self?.releaseAll()
            self?.connected = GCController.controllers().first(where: { $0.extendedGamepad != nil })?.vendorName
        }
        GCController.controllers().forEach(configure)
    }

    private func configure(_ controller: GCController) {
        guard let pad = controller.extendedGamepad else { return }
        connected = controller.vendorName ?? "게임패드"
        var inputs: [(GCControllerButtonInput, PadButton)] = [
            (pad.dpad.up, .up), (pad.dpad.down, .down), (pad.dpad.left, .left), (pad.dpad.right, .right),
            (pad.buttonA, .a), (pad.buttonB, .b), (pad.buttonX, .x), (pad.buttonY, .y),
            (pad.leftShoulder, .l1), (pad.rightShoulder, .r1), (pad.leftTrigger, .l2), (pad.rightTrigger, .r2),
            (pad.buttonMenu, .start),
        ]
        if let stick = pad.leftThumbstickButton {
            inputs.append((stick, .l3))
        }
        if let stick = pad.rightThumbstickButton {
            inputs.append((stick, .r3))
        }
        for (input, button) in inputs {
            input.pressedChangedHandler = { [weak self] _, _, pressed in
                self?.press(button, pressed, from: button.rawValue)
            }
        }
        // The left stick steers as the D-pad does, pressing what it has.
        let stick: [(GCControllerButtonInput, PadButton)] = [
            (pad.leftThumbstick.up, .up), (pad.leftThumbstick.down, .down),
            (pad.leftThumbstick.left, .left), (pad.leftThumbstick.right, .right),
        ]
        for (input, button) in stick {
            input.pressedChangedHandler = { [weak self] _, _, pressed in
                self?.press(button, pressed, from: 100 + button.rawValue)
            }
        }
        pad.buttonOptions?.pressedChangedHandler = { [weak self] _, _, pressed in
            self?.selectHeld = pressed
            if pressed {
                self?.capture?(nil)
            }
        }
    }

    private func press(_ button: PadButton, _ pressed: Bool, from input: Int) {
        if let capture = capture {
            if pressed {
                capture(button)
            }
            return
        }
        if pressed {
            guard held[input] == nil, let key = mapping.key(button, select: selectHeld) else { return }
            held[input] = key
            send?(key, true)
        } else if let key = held.removeValue(forKey: input) {
            send?(key, false)
        }
    }

    /// Lets go of every key a pad is holding.
    func releaseAll() {
        for key in held.values {
            send?(key, false)
        }
        held.removeAll()
    }
}

/// The pad's buttons on a drawing of a gamepad, each with the handset key it
/// presses, as the PC and handheld builds have it: a tab for each layer, the
/// gamepad, and the button picked with its key in both layers under it. A
/// pad button pressed picks it out; pressed or tapped again, it asks for a
/// new key.
struct PadMappingView: View {
    @ObservedObject private var gamepad = Gamepad.shared
    @State private var picked = PadButton.a
    @State private var withSelect = false
    @State private var choosing: Choice?
    @State private var confirmingReset = false

    /// A key being chosen for one button in one layer.
    private struct Choice: Identifiable {
        let button: PadButton
        let withSelect: Bool
        var id: String { "\(button.rawValue)-\(withSelect)" }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text("패드 버튼을 누르거나 그림에서 고르세요. 고른 버튼을 한 번 더 누르면 폰 키를 바꿉니다. SELECT를 누른 채 누르면 둘째 칸의 키가 눌려요.")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                Text("연결된 패드: \(gamepad.connected ?? "없음")")
                    .font(.footnote.weight(.semibold))
                    .foregroundColor(gamepad.connected == nil ? Color.secondary : Color.accentColor)
                tabs
                drawing
                strip
            }
            .padding(16)
        }
        .navigationTitle("게임패드 버튼 배치")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("↺ 기본값") { confirmingReset = true }
            }
        }
        .confirmationDialog("버튼 배치를 처음 설정으로 되돌릴까요?", isPresented: $confirmingReset, titleVisibility: .visible) {
            Button("되돌리기", role: .destructive) {
                gamepad.mapping = .standard
            }
        } message: {
            Text("게임패드 버튼의 두 칸만 바뀌어요. 화면의 가상 패드는 그대로예요.")
        }
        .sheet(item: $choosing) { choice in
            KeyChoiceView(title: "\(choice.button.name) 버튼 (\(choice.withSelect ? "SELECT+" : "그냥"))",
                          current: choice.withSelect ? gamepad.mapping.select[choice.button.rawValue] : gamepad.mapping.plain[choice.button.rawValue],
                          notes: notes(select: choice.withSelect),
                          noneNote: choice.withSelect ? "그냥과 같게" : "") { key in
                if choice.withSelect {
                    gamepad.mapping.select[choice.button.rawValue] = key
                } else {
                    gamepad.mapping.plain[choice.button.rawValue] = key
                }
            }
        }
        .onAppear {
            gamepad.releaseAll()
            gamepad.capture = { button in
                guard choosing == nil else { return }
                guard let button = button else {
                    withSelect.toggle()
                    return
                }
                if button == picked {
                    choosing = Choice(button: button, withSelect: withSelect)
                } else {
                    picked = button
                }
            }
        }
        .onDisappear { gamepad.capture = nil }
    }

    /// On each handset key, the pad buttons already pressing it in a layer.
    private func notes(select layer: Bool) -> [Int32: String] {
        var notes: [Int32: String] = [:]
        for button in PadButton.allCases {
            guard let key = (layer ? gamepad.mapping.select : gamepad.mapping.plain)[button.rawValue] else { continue }
            notes[key] = notes[key].map { $0 + "·" + button.name } ?? button.name
        }
        return notes
    }

    private var tabs: some View {
        HStack(spacing: 6) {
            ForEach([false, true], id: \.self) { layer in
                let on = layer == withSelect
                Button {
                    withSelect = layer
                } label: {
                    Text(layer ? "SELECT 누른 채" : "그냥 누를 때")
                        .font(.footnote.weight(.semibold))
                        .padding(.horizontal, 14)
                        .padding(.vertical, 6)
                        .foregroundColor(on ? Color.white : Color.secondary)
                        .background(Capsule().fill(on ? Color.accentColor : Color(.secondarySystemBackground)))
                        .overlay(Capsule().stroke(Color(.separator), lineWidth: on ? 0 : 1))
                }
                .buttonStyle(.plain)
            }
        }
    }

    /// The key a button presses in one layer, and whether it is that layer's
    /// own rather than the plain one shown through.
    private func shown(_ button: PadButton, select layer: Bool) -> (key: Int32?, own: Bool) {
        if layer, let key = gamepad.mapping.select[button.rawValue] {
            return (key, true)
        }
        let key = gamepad.mapping.plain[button.rawValue]
        return (key, !layer && key != nil)
    }

    private var drawing: some View {
        GeometryReader { geometry in
            let scale = geometry.size.width / 320
            ZStack(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 30 * scale, style: .continuous)
                    .fill(Color(.secondarySystemBackground))
                    .overlay(RoundedRectangle(cornerRadius: 30 * scale, style: .continuous).stroke(Color(.separator)))
                    .frame(width: 296 * scale, height: 118 * scale)
                    .offset(x: 12 * scale, y: 0)
                ForEach(PadButton.allCases) { button in
                    chip(button, scale: scale)
                }
                selectChip(scale: scale)
            }
        }
        .aspectRatio(CGFloat(320.0 / 118.0), contentMode: .fit)
    }

    private func chip(_ button: PadButton, scale: CGFloat) -> some View {
        let place = button.place
        let on = button == picked
        let value = shown(button, select: withSelect)
        let shape = RoundedRectangle(cornerRadius: place.r * scale, style: .continuous)
        return Button {
            if on {
                choosing = Choice(button: button, withSelect: withSelect)
            } else {
                picked = button
            }
        } label: {
            VStack(spacing: 0) {
                Text(button.name)
                    .font(.system(size: 7.5 * scale, weight: .semibold))
                    .foregroundColor(on ? Color.white.opacity(0.85) : Color.secondary)
                Text(handsetKeyLabel(value.key))
                    .font(.system(size: 8.5 * scale, weight: .bold))
                    .foregroundColor(on ? Color.white : value.own ? Color.accentColor : Color.secondary.opacity(0.6))
            }
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .frame(width: place.w * scale, height: place.h * scale)
            .background(shape.fill(on ? Color.accentColor : Color(.tertiarySystemBackground)))
            .overlay(shape.stroke(on ? Color.accentColor : Color(.separator)))
        }
        .buttonStyle(.plain)
        .offset(x: place.x * scale, y: place.y * scale)
    }

    private func selectChip(scale: CGFloat) -> some View {
        VStack(spacing: 0) {
            Text("SELECT").font(.system(size: 7.5 * scale, weight: .semibold))
            Text("숫자 전환").font(.system(size: 7.5 * scale, weight: .semibold))
        }
        .foregroundColor(.secondary)
        .frame(width: 48 * scale, height: 20 * scale)
        .background(Capsule().fill(Color(.tertiarySystemBackground)))
        .overlay(Capsule().stroke(Color(.separator)))
        .offset(x: 111 * scale, y: 39 * scale)
    }

    /// The button picked, and its key in each layer; a tap on one changes it.
    private var strip: some View {
        HStack(spacing: 12) {
            Text(picked.name)
                .font(.headline)
                .foregroundColor(.white)
                .frame(minWidth: 44, minHeight: 36)
                .padding(.horizontal, 10)
                .background(Capsule().fill(Color.accentColor))
            Image(systemName: "arrow.right").foregroundColor(.secondary)
            ForEach([false, true], id: \.self) { layer in
                let value = shown(picked, select: layer)
                let on = layer == withSelect
                Button {
                    withSelect = layer
                    choosing = Choice(button: picked, withSelect: layer)
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(layer ? "SELECT+" : "그냥").font(.caption).foregroundColor(.secondary)
                        Text(handsetKeyLabel(value.key))
                            .font(.headline)
                            .foregroundColor(value.own ? (on ? Color.primary : Color.accentColor) : Color.secondary)
                            .frame(minWidth: 64, minHeight: 32)
                            .background(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(on ? Color.accentColor.opacity(0.18) : Color(.tertiarySystemBackground))
                            )
                            .overlay(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .stroke(on ? Color.accentColor : Color(.separator))
                            )
                    }
                }
                .buttonStyle(.plain)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color(.secondarySystemBackground)))
        .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Color(.separator)))
    }
}

/// The handset's keypad, to choose the key a gamepad button presses: the soft
/// keys, the directions and the call keys on the left, the number pad on the
/// right, as the PC builds draw their keyboard layout. Each key carries the
/// pad buttons already on it; the key the button has now is outlined. A tap
/// chooses, and so do the pad's D-pad and A; B leaves it as it is.
private struct KeyChoiceView: View {
    let title: String
    let current: Int32?
    let notes: [Int32: String]
    let noneNote: String
    let choose: (Int32?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var cursor = 0
    @State private var outerCapture: ((PadButton?) -> Void)?

    /// A key of the keypad: the handset key (nil for none), its column -
    /// three on the left block, three on the number pad - and its row.
    private struct Cell {
        let key: Int32?
        let column: Int
        let row: Int
    }

    private static let cells: [Cell] = {
        let num = { (digit: Int32) in Int32(WIE_KEY_NUM0) + digit }
        return [
            Cell(key: Int32(WIE_KEY_LEFT_SOFT), column: 0, row: 0), Cell(key: Int32(WIE_KEY_UP), column: 1, row: 0),
            Cell(key: Int32(WIE_KEY_RIGHT_SOFT), column: 2, row: 0),
            Cell(key: Int32(WIE_KEY_LEFT), column: 0, row: 1), Cell(key: Int32(WIE_KEY_OK), column: 1, row: 1),
            Cell(key: Int32(WIE_KEY_RIGHT), column: 2, row: 1),
            Cell(key: Int32(WIE_KEY_CALL), column: 0, row: 2), Cell(key: Int32(WIE_KEY_DOWN), column: 1, row: 2),
            Cell(key: Int32(WIE_KEY_HANGUP), column: 2, row: 2),
            Cell(key: Int32(WIE_KEY_CLEAR), column: 1, row: 3), Cell(key: nil, column: 2, row: 3),
            Cell(key: num(1), column: 3, row: 0), Cell(key: num(2), column: 4, row: 0), Cell(key: num(3), column: 5, row: 0),
            Cell(key: num(4), column: 3, row: 1), Cell(key: num(5), column: 4, row: 1), Cell(key: num(6), column: 5, row: 1),
            Cell(key: num(7), column: 3, row: 2), Cell(key: num(8), column: 4, row: 2), Cell(key: num(9), column: 5, row: 2),
            Cell(key: Int32(WIE_KEY_STAR), column: 3, row: 3), Cell(key: num(0), column: 4, row: 3),
            Cell(key: Int32(WIE_KEY_HASH), column: 5, row: 3),
        ]
    }()

    /// Where a key sits on a 320-unit-wide plan.
    private static func frame(_ cell: Cell) -> CGRect {
        let width: CGFloat = (320 - 8 - 14 - 16) / 6
        let x = 4 + CGFloat(cell.column) * (width + 4) + (cell.column >= 3 ? 10 : 0)
        return CGRect(x: x, y: 1 + CGFloat(cell.row) * 40, width: width, height: 36)
    }

    var body: some View {
        NavigationView {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("누를 폰 키를 고르세요. 지금: \(current == nil ? (noneNote.isEmpty ? "없음" : noneNote) : handsetKeyLabel(current))")
                        .font(.footnote)
                        .foregroundColor(.secondary)
                    GeometryReader { geometry in
                        let scale = geometry.size.width / 320
                        ZStack(alignment: .topLeading) {
                            ForEach(Self.cells.indices, id: \.self) { index in
                                key(index, scale: scale)
                            }
                        }
                    }
                    .aspectRatio(CGFloat(320.0 / 158.0), contentMode: .fit)
                }
                .padding(16)
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("취소") { dismiss() }
                }
            }
        }
        .onAppear {
            cursor = Self.cells.firstIndex { $0.key == current } ?? 0
            outerCapture = Gamepad.shared.capture
            Gamepad.shared.capture = { button in
                switch button {
                case .up?, .down?, .left?, .right?:
                    cursor = step(from: cursor, button!)
                case .a?:
                    pick(Self.cells[cursor].key)
                case .b?:
                    dismiss()
                default:
                    break
                }
            }
        }
        .onDisappear {
            Gamepad.shared.capture = outerCapture
        }
    }

    private func key(_ index: Int, scale: CGFloat) -> some View {
        let cell = Self.cells[index]
        let rect = Self.frame(cell)
        let focused = index == cursor
        let now = cell.key == current
        let name = cell.key == nil ? "없음" : handsetKeyLabel(cell.key)
        let note = cell.key.map { notes[$0] ?? "" } ?? noneNote
        let shape = RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
        return Button {
            cursor = index
            pick(cell.key)
        } label: {
            VStack(spacing: 1 * scale) {
                Text(name)
                    .font(.system(size: (name.count > 2 ? 10.5 : 13) * scale, weight: .bold))
                    .foregroundColor(focused ? Color.white : cell.key == nil ? Color.secondary : Color.primary)
                if !note.isEmpty {
                    Text(note)
                        .font(.system(size: 7.5 * scale, weight: .semibold))
                        .foregroundColor(focused ? Color.white.opacity(0.85) : Color.accentColor)
                }
            }
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .padding(.horizontal, 2)
            .frame(width: rect.width * scale, height: rect.height * scale)
            .background(shape.fill(focused ? Color.accentColor : Color(.secondarySystemBackground)))
            .overlay(shape.stroke(now ? Color.accentColor : Color(.separator), lineWidth: now ? 2 : 1))
        }
        .buttonStyle(.plain)
        .offset(x: rect.minX * scale, y: rect.minY * scale)
    }

    /// The key a direction leads to: of the keys wholly past this one's edge
    /// that way, the nearest, distance across the direction counting double.
    private func step(from: Int, _ direction: PadButton) -> Int {
        let origin = Self.frame(Self.cells[from])
        var best = from
        var bestDistance = CGFloat.greatestFiniteMagnitude
        for (index, cell) in Self.cells.enumerated() {
            let rect = Self.frame(cell)
            let past: Bool
            let along: CGFloat
            let across: CGFloat
            switch direction {
            case .up:
                past = rect.maxY <= origin.minY
                along = origin.midY - rect.midY
                across = rect.midX - origin.midX
            case .down:
                past = rect.minY >= origin.maxY
                along = rect.midY - origin.midY
                across = rect.midX - origin.midX
            case .left:
                past = rect.maxX <= origin.minX
                along = origin.midX - rect.midX
                across = rect.midY - origin.midY
            default:
                past = rect.minX >= origin.maxX
                along = rect.midX - origin.midX
                across = rect.midY - origin.midY
            }
            let distance = along + 2 * abs(across)
            if past && distance < bestDistance {
                best = index
                bestDistance = distance
            }
        }
        return best
    }

    private func pick(_ key: Int32?) {
        choose(key)
        dismiss()
    }
}
