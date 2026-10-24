import GameController
import SwiftUI

/// A gamepad button the mapping covers, in the order the other builds list
/// them. SELECT is not one: held, it turns the buttons to their second layer.
enum PadButton: Int, CaseIterable, Identifiable {
    case up, down, left, right, a, b, x, y, l1, r1, l2, r2, l3, r3, start

    var id: Int { rawValue }

    var name: String {
        ["D▲", "D▼", "D◀", "D▶", "A", "B", "X", "Y", "L1", "R1", "L2", "R2", "L3", "R3", "START"][rawValue]
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

/// The gamepad's layout, the way the keyboard's is on the PC builds: the
/// handset's keypad, each key with the pad buttons that press it in one
/// layer - plain, or with SELECT held - and a key picked asks for the pad
/// button to put on it. A button on another key moves over; one already on
/// this key comes off. On the pad, the D-pad and A pick a key and SELECT
/// turns to the other layer.
struct PadMappingView: View {
    @ObservedObject private var gamepad = Gamepad.shared
    @Environment(\.dismiss) private var dismiss
    @State private var cursor = HandsetKeypad.cells.firstIndex { $0.key == Int32(WIE_KEY_OK) } ?? 0
    @State private var withSelect = false
    @State private var capturing: Capture?
    @State private var status = ""
    @State private var confirmingReset = false

    /// A button being asked for, for one key in one layer.
    struct Capture: Identifiable {
        let key: Int32
        let withSelect: Bool
        var id: String { "\(key)-\(withSelect)" }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text("폰 키를 고른 뒤 쓸 패드 버튼을 누르세요. 다른 키에 있던 버튼은 옮겨 오고, 이 키에 이미 있는 버튼을 누르면 빠집니다. 패드로는 방향키로 고르고 A, SELECT로 칸을 바꿉니다.")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                Text("연결된 패드: \(gamepad.connected ?? "없음")")
                    .font(.footnote.weight(.semibold))
                    .foregroundColor(gamepad.connected == nil ? Color.secondary : Color.accentColor)
                tabs
                HandsetKeypad(cursor: cursor, notes: notes(select: withSelect)) { index in
                    if index == cursor {
                        capture()
                    } else {
                        cursor = index
                        status = ""
                    }
                }
                if !status.isEmpty {
                    Text(status)
                        .font(.footnote.weight(.semibold))
                        .foregroundColor(.accentColor)
                        .frame(maxWidth: .infinity)
                }
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
                status = "버튼 배치를 기본값으로 되돌렸습니다."
            }
        } message: {
            Text("게임패드 버튼의 두 칸만 바뀌어요. 화면의 가상 패드는 그대로예요.")
        }
        .sheet(item: $capturing) { capture in
            ButtonCaptureView(capture: capture, now: buttons(on: capture.key, select: capture.withSelect)) { outcome in
                status = outcome
            }
        }
        .onAppear(perform: listen)
        .onDisappear { gamepad.capture = nil }
    }

    /// The pad, while this is up and no button is being asked for: the
    /// D-pad moves, A asks for a button, SELECT turns the layer, B goes back.
    private func listen() {
        gamepad.releaseAll()
        gamepad.capture = { button in
            guard capturing == nil else { return }
            switch button {
            case nil:
                withSelect.toggle()
            case .up?, .down?, .left?, .right?:
                cursor = HandsetKeypad.step(from: cursor, button!)
                status = ""
            case .a?:
                capture()
            case .b?:
                dismiss()
            default:
                break
            }
        }
    }

    private func capture() {
        if let key = HandsetKeypad.cells[cursor].key {
            capturing = Capture(key: key, withSelect: withSelect)
        }
    }

    /// The pad buttons pressing `key` in one layer, by name.
    private func buttons(on key: Int32, select layer: Bool) -> String {
        let table = layer ? gamepad.mapping.select : gamepad.mapping.plain
        return PadButton.allCases.filter { table[$0.rawValue] == key }.map(\.name).joined(separator: " · ")
    }

    /// On each handset key, the pad buttons pressing it in a layer.
    private func notes(select layer: Bool) -> [Int32: String] {
        var notes: [Int32: String] = [:]
        for cell in HandsetKeypad.cells {
            if let key = cell.key {
                notes[key] = buttons(on: key, select: layer)
            }
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

    /// The key picked, and the buttons on it in each layer; a tap on a layer
    /// asks for a button there.
    private var strip: some View {
        let key = HandsetKeypad.cells[cursor].key
        return HStack(spacing: 12) {
            Text(handsetKeyLabel(key))
                .font(.headline)
                .foregroundColor(.white)
                .frame(minWidth: 44, minHeight: 36)
                .padding(.horizontal, 10)
                .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color.accentColor))
            Image(systemName: "arrow.right").foregroundColor(.secondary)
            ForEach([false, true], id: \.self) { layer in
                let on = layer == withSelect
                let names = key.map { buttons(on: $0, select: layer) } ?? ""
                Button {
                    withSelect = layer
                    capture()
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(layer ? "SELECT+" : "그냥").font(.caption).foregroundColor(.secondary)
                        Text(names.isEmpty ? "없음" : names)
                            .font(.headline)
                            .lineLimit(1)
                            .minimumScaleFactor(0.6)
                            .foregroundColor(names.isEmpty ? Color.secondary : (on ? Color.primary : Color.accentColor))
                            .padding(.horizontal, 8)
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

/// Asks for the pad button to put on one key in one layer: pressed, it goes
/// on, moving off whatever key had it - or comes off, if it was on this one.
/// 비우기 takes every button off the key in that layer; 취소 leaves it. Every
/// button but SELECT, the layer key, can be put on a key, so cancelling is a
/// tap.
private struct ButtonCaptureView: View {
    let capture: PadMappingView.Capture
    let now: String
    let done: (String) -> Void

    @ObservedObject private var gamepad = Gamepad.shared
    @Environment(\.dismiss) private var dismiss
    @State private var note = "…"
    @State private var outerCapture: ((PadButton?) -> Void)?

    private var layer: String { capture.withSelect ? "SELECT 누른 채" : "그냥 누를 때" }

    var body: some View {
        NavigationView {
            VStack(spacing: 14) {
                Text("지금: \(now.isEmpty ? "없음" : now)")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                Text("쓸 버튼을 누르세요")
                    .font(.title3.weight(.bold))
                    .padding(.top, 12)
                Text(note)
                    .font(.footnote.weight(.semibold))
                    .foregroundColor(.accentColor)
                Text("다른 키에 있던 버튼은 이 키로 옮겨 오고, 이미 이 키에 있는 버튼을 누르면 빠집니다.")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
                Spacer()
                Button("비우기", role: .destructive) { clear() }
            }
            .padding(20)
            .navigationTitle("\(handsetKeyLabel(capture.key)) (\(layer))")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button("취소") { dismiss() }
                }
            }
        }
        .onAppear {
            outerCapture = gamepad.capture
            gamepad.capture = { button in
                guard let button = button else {
                    note = "SELECT는 칸 전환이라 쓸 수 없습니다."
                    return
                }
                if button == .start && capture.withSelect {
                    note = "SELECT+START는 쓸 수 없습니다."
                    return
                }
                put(button)
            }
        }
        .onDisappear {
            gamepad.capture = outerCapture
        }
    }

    private func put(_ button: PadButton) {
        let key = capture.key
        let before = capture.withSelect ? gamepad.mapping.select[button.rawValue] : gamepad.mapping.plain[button.rawValue]
        let outcome: String
        if before == key {
            set(button, nil)
            outcome = "\(button.name): '\(handsetKeyLabel(key))'에서 뺐습니다."
        } else {
            set(button, key)
            outcome = before.map { "\(button.name): '\(handsetKeyLabel($0))'에서 옮겨 왔습니다." }
                ?? "\(button.name): '\(handsetKeyLabel(key))'에 붙였습니다."
        }
        done(outcome)
        dismiss()
    }

    private func set(_ button: PadButton, _ key: Int32?) {
        if capture.withSelect {
            gamepad.mapping.select[button.rawValue] = key
        } else {
            gamepad.mapping.plain[button.rawValue] = key
        }
    }

    private func clear() {
        for button in PadButton.allCases {
            let table = capture.withSelect ? gamepad.mapping.select : gamepad.mapping.plain
            if table[button.rawValue] == capture.key {
                set(button, nil)
            }
        }
        done("\(handsetKeyLabel(capture.key)): \(layer) 버튼을 모두 뺐습니다.")
        dismiss()
    }
}

/// The handset's keypad: the soft keys, the directions and the call keys on
/// the left, the number pad on the right, as the PC builds draw their
/// layouts. Each key carries what is on it; the key picked is filled.
struct HandsetKeypad: View {
    let cursor: Int
    let notes: [Int32: String]
    let tap: (Int) -> Void

    /// A key of the keypad: the handset key (nil for none), its column -
    /// three on the left block, three on the number pad - and its row.
    struct Cell {
        let key: Int32?
        let column: Int
        let row: Int
    }

    static let cells: [Cell] = {
        let num = { (digit: Int32) in Int32(WIE_KEY_NUM0) + digit }
        return [
            Cell(key: Int32(WIE_KEY_LEFT_SOFT), column: 0, row: 0), Cell(key: Int32(WIE_KEY_UP), column: 1, row: 0),
            Cell(key: Int32(WIE_KEY_RIGHT_SOFT), column: 2, row: 0),
            Cell(key: Int32(WIE_KEY_LEFT), column: 0, row: 1), Cell(key: Int32(WIE_KEY_OK), column: 1, row: 1),
            Cell(key: Int32(WIE_KEY_RIGHT), column: 2, row: 1),
            Cell(key: Int32(WIE_KEY_CALL), column: 0, row: 2), Cell(key: Int32(WIE_KEY_DOWN), column: 1, row: 2),
            Cell(key: Int32(WIE_KEY_HANGUP), column: 2, row: 2),
            Cell(key: Int32(WIE_KEY_CLEAR), column: 1, row: 3),
            Cell(key: num(1), column: 3, row: 0), Cell(key: num(2), column: 4, row: 0), Cell(key: num(3), column: 5, row: 0),
            Cell(key: num(4), column: 3, row: 1), Cell(key: num(5), column: 4, row: 1), Cell(key: num(6), column: 5, row: 1),
            Cell(key: num(7), column: 3, row: 2), Cell(key: num(8), column: 4, row: 2), Cell(key: num(9), column: 5, row: 2),
            Cell(key: Int32(WIE_KEY_STAR), column: 3, row: 3), Cell(key: num(0), column: 4, row: 3),
            Cell(key: Int32(WIE_KEY_HASH), column: 5, row: 3),
        ]
    }()

    /// Where a key sits on a 320-unit-wide plan.
    static func frame(_ cell: Cell) -> CGRect {
        let width: CGFloat = (320 - 8 - 14 - 16) / 6
        let x = 4 + CGFloat(cell.column) * (width + 4) + (cell.column >= 3 ? 10 : 0)
        return CGRect(x: x, y: 1 + CGFloat(cell.row) * 40, width: width, height: 36)
    }

    var body: some View {
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

    private func key(_ index: Int, scale: CGFloat) -> some View {
        let cell = Self.cells[index]
        let rect = Self.frame(cell)
        let focused = index == cursor
        let name = handsetKeyLabel(cell.key)
        let note = cell.key.flatMap { notes[$0] } ?? ""
        let shape = RoundedRectangle(cornerRadius: 8 * scale, style: .continuous)
        return Button {
            tap(index)
        } label: {
            VStack(spacing: 1 * scale) {
                Text(name)
                    .font(.system(size: (name.count > 2 ? 10.5 : 13) * scale, weight: .bold))
                    .foregroundColor(focused ? Color.white : Color.primary)
                Text(note.isEmpty ? "없음" : note)
                    .font(.system(size: 7.5 * scale, weight: .semibold))
                    .foregroundColor(focused ? Color.white.opacity(0.85) : note.isEmpty ? Color.secondary.opacity(0.6) : Color.accentColor)
            }
            .lineLimit(1)
            .minimumScaleFactor(0.6)
            .padding(.horizontal, 2)
            .frame(width: rect.width * scale, height: rect.height * scale)
            .background(shape.fill(focused ? Color.accentColor : Color(.secondarySystemBackground)))
            .overlay(shape.stroke(Color(.separator)))
        }
        .buttonStyle(.plain)
        .offset(x: rect.minX * scale, y: rect.minY * scale)
    }

    /// The key a direction leads to: of the keys wholly past this one's edge
    /// that way, the nearest, distance across the direction counting double.
    static func step(from: Int, _ direction: PadButton) -> Int {
        let origin = frame(cells[from])
        var best = from
        var bestDistance = CGFloat.greatestFiniteMagnitude
        for (index, cell) in cells.enumerated() {
            let rect = frame(cell)
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
}
