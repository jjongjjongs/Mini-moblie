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
                          allowsNone: true,
                          noneLabel: choice.withSelect ? "없음 (그냥 누를 때와 같게)" : "없음") { key in
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

/// The handset's keys to choose from: the number pad, then the rest.
private struct KeyChoiceView: View {
    let title: String
    let current: Int32?
    let allowsNone: Bool
    let noneLabel: String
    let choose: (Int32?) -> Void

    @Environment(\.dismiss) private var dismiss

    private static let numberPad: [Int32] = [1, 2, 3, 4, 5, 6, 7, 8, 9].map { Int32(WIE_KEY_NUM0) + $0 }
        + [Int32(WIE_KEY_STAR), Int32(WIE_KEY_NUM0), Int32(WIE_KEY_HASH)]
    private static let others: [Int32] = [
        Int32(WIE_KEY_UP), Int32(WIE_KEY_DOWN), Int32(WIE_KEY_LEFT), Int32(WIE_KEY_RIGHT),
        Int32(WIE_KEY_OK), Int32(WIE_KEY_CLEAR), Int32(WIE_KEY_LEFT_SOFT), Int32(WIE_KEY_RIGHT_SOFT),
        Int32(WIE_KEY_CALL), Int32(WIE_KEY_HANGUP),
    ]

    var body: some View {
        NavigationView {
            ScrollView {
                VStack(spacing: 16) {
                    grid(Self.numberPad, columns: 3)
                    grid(Self.others, columns: 4)
                    if allowsNone {
                        cell(noneLabel, chosen: current == nil) { pick(nil) }
                    }
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
    }

    private func grid(_ keys: [Int32], columns: Int) -> some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: columns), spacing: 8) {
            ForEach(keys, id: \.self) { key in
                cell(handsetKeyLabel(key), chosen: key == current) { pick(key) }
            }
        }
    }

    private func cell(_ label: String, chosen: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.headline)
                .frame(maxWidth: .infinity, minHeight: 44)
                .foregroundColor(chosen ? Color.white : Color.primary)
                .background(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(chosen ? Color.accentColor : Color(.secondarySystemBackground))
                )
        }
        .buttonStyle(.plain)
    }

    private func pick(_ key: Int32?) {
        choose(key)
        dismiss()
    }
}
