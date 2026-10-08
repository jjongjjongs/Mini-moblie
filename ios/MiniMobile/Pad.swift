import SwiftUI
import UIKit

/// Where the pad sits: in its own area under the screen, or over the screen,
/// see-through, so the screen can take the whole display.
enum PadMode: String, CaseIterable, Identifiable {
    case below
    case overlay

    var id: String { rawValue }

    var label: String {
        switch self {
        case .below: return "화면 아래"
        case .overlay: return "화면 위에 겹치기"
        }
    }
}

/// One key of the pad: which handset key it presses (the indexes in wie.h)
/// and where it is, as fractions of the pad's area so a layout fits any
/// display.
struct PadKey: Codable, Identifiable, Equatable {
    let index: Int32
    var x: Double
    var y: Double
    var w: Double
    var h: Double
    var hidden: Bool

    var id: Int32 { index }

    func rect(in size: CGSize) -> CGRect {
        CGRect(x: x * size.width, y: y * size.height, width: w * size.width, height: h * size.height)
    }

    var label: String { Self.labels[index] ?? "?" }

    static let labels: [Int32: String] = [
        Int32(WIE_KEY_UP): "▲",
        Int32(WIE_KEY_DOWN): "▼",
        Int32(WIE_KEY_LEFT): "◀︎",
        Int32(WIE_KEY_RIGHT): "▶︎",
        Int32(WIE_KEY_OK): "OK",
        Int32(WIE_KEY_LEFT_SOFT): "◀︎소프트",
        Int32(WIE_KEY_RIGHT_SOFT): "소프트▶︎",
        Int32(WIE_KEY_CLEAR): "취소",
        Int32(WIE_KEY_NUM0): "0",
        Int32(WIE_KEY_NUM0) + 1: "1",
        Int32(WIE_KEY_NUM0) + 2: "2",
        Int32(WIE_KEY_NUM0) + 3: "3",
        Int32(WIE_KEY_NUM0) + 4: "4",
        Int32(WIE_KEY_NUM0) + 5: "5",
        Int32(WIE_KEY_NUM0) + 6: "6",
        Int32(WIE_KEY_NUM0) + 7: "7",
        Int32(WIE_KEY_NUM0) + 8: "8",
        Int32(WIE_KEY_NUM0) + 9: "9",
        Int32(WIE_KEY_STAR): "*",
        Int32(WIE_KEY_HASH): "#",
        Int32(WIE_KEY_CALL): "통화",
        Int32(WIE_KEY_HANGUP): "종료",
    ]
}

/// Every key of the pad, placed.
struct PadLayout: Codable, Equatable {
    var keys: [PadKey]

    /// The layout kept under `text`, or the standard one when there is none
    /// (or it does not read). Keys a stored layout lacks come from the
    /// standard one, so a key added later still shows up.
    static func decode(_ text: String, mode: PadMode) -> PadLayout {
        let standard = standard(mode)
        guard let data = text.data(using: .utf8), let stored = try? JSONDecoder().decode(PadLayout.self, from: data) else {
            return standard
        }
        var keys = stored.keys.filter { PadKey.labels[$0.index] != nil }
        for key in standard.keys where !keys.contains(where: { $0.index == key.index }) {
            keys.append(key)
        }
        return PadLayout(keys: keys)
    }

    func encoded() -> String {
        guard let data = try? JSONEncoder().encode(self) else { return "" }
        return String(decoding: data, as: UTF8.self)
    }

    static func standard(_ mode: PadMode) -> PadLayout {
        switch mode {
        case .below: return below
        case .overlay: return overlay
        }
    }

    /// The handset's keys as a 3-wide grid filling the area under the screen.
    private static let below: PadLayout = {
        let rows: [[Int32]] = [
            [Int32(WIE_KEY_LEFT_SOFT), Int32(WIE_KEY_UP), Int32(WIE_KEY_RIGHT_SOFT)],
            [Int32(WIE_KEY_LEFT), Int32(WIE_KEY_OK), Int32(WIE_KEY_RIGHT)],
            [Int32(WIE_KEY_CLEAR), Int32(WIE_KEY_DOWN), Int32(WIE_KEY_CALL)],
            [9, 10, 11],
            [12, 13, 14],
            [15, 16, 17],
            [Int32(WIE_KEY_STAR), Int32(WIE_KEY_NUM0), Int32(WIE_KEY_HASH)],
        ]
        var keys: [PadKey] = []
        for (row, indexes) in rows.enumerated() {
            for (column, index) in indexes.enumerated() {
                keys.append(PadKey(index: index, x: Double(column) / 3, y: Double(row) / 7, w: 1.0 / 3, h: 1.0 / 7, hidden: false))
            }
        }
        // 종료 has no place on the handset grid; it is there to be shown and
        // placed by the player that needs it.
        keys.append(PadKey(index: Int32(WIE_KEY_HANGUP), x: 2.0 / 3, y: 2.0 / 7, w: 1.0 / 3, h: 1.0 / 7, hidden: true))
        return PadLayout(keys: keys)
    }()

    /// Over the screen: the direction keys under the left thumb, the number
    /// keys under the right, the soft keys and 취소/통화 above them.
    private static let overlay: PadLayout = {
        var keys: [PadKey] = [
            PadKey(index: Int32(WIE_KEY_LEFT_SOFT), x: 0.02, y: 0.55, w: 0.22, h: 0.07, hidden: false),
            PadKey(index: Int32(WIE_KEY_CLEAR), x: 0.26, y: 0.55, w: 0.22, h: 0.07, hidden: false),
            PadKey(index: Int32(WIE_KEY_CALL), x: 0.52, y: 0.55, w: 0.22, h: 0.07, hidden: false),
            PadKey(index: Int32(WIE_KEY_RIGHT_SOFT), x: 0.76, y: 0.55, w: 0.22, h: 0.07, hidden: false),
            PadKey(index: Int32(WIE_KEY_UP), x: 0.165, y: 0.66, w: 0.15, h: 0.09, hidden: false),
            PadKey(index: Int32(WIE_KEY_LEFT), x: 0.015, y: 0.755, w: 0.15, h: 0.09, hidden: false),
            PadKey(index: Int32(WIE_KEY_OK), x: 0.165, y: 0.755, w: 0.15, h: 0.09, hidden: false),
            PadKey(index: Int32(WIE_KEY_RIGHT), x: 0.315, y: 0.755, w: 0.15, h: 0.09, hidden: false),
            PadKey(index: Int32(WIE_KEY_DOWN), x: 0.165, y: 0.85, w: 0.15, h: 0.09, hidden: false),
            PadKey(index: Int32(WIE_KEY_HANGUP), x: 0.02, y: 0.47, w: 0.22, h: 0.07, hidden: true),
        ]
        let numbers: [[Int32]] = [[9, 10, 11], [12, 13, 14], [15, 16, 17], [Int32(WIE_KEY_STAR), Int32(WIE_KEY_NUM0), Int32(WIE_KEY_HASH)]]
        for (row, indexes) in numbers.enumerated() {
            for (column, index) in indexes.enumerated() {
                keys.append(PadKey(index: index, x: 0.55 + Double(column) * 0.145, y: 0.64 + Double(row) * 0.085, w: 0.145, h: 0.085, hidden: false))
            }
        }
        return PadLayout(keys: keys)
    }()
}

/// How a key looks, pressed or not.
struct KeyFace: View {
    let label: String
    let pressed: Bool
    let size: CGSize
    let labelScale: Double

    var body: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(Color(white: pressed ? 0.5 : 0.2))
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.white.opacity(0.28), lineWidth: 1))
            .overlay(
                Text(label)
                    .font(.system(size: max(9, min(size.height * 0.42, size.width * 0.32) * labelScale), weight: .semibold))
                    .foregroundColor(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                    .padding(.horizontal, 2)
            )
            .frame(width: size.width, height: size.height)
    }
}

/// The pad in play: its keys drawn, and the touches on them read by one
/// multi-touch view, so a thumb slid from key to key releases one and presses
/// the next, the way a held direction rolls on a handset.
struct PadView: View {
    let layout: PadLayout
    let opacity: Double
    let labelScale: Double
    let haptics: Bool
    let onKey: (Int32, Bool) -> Void

    @State private var pressed = Set<Int32>()

    var body: some View {
        GeometryReader { geometry in
            let visible = layout.keys.filter { !$0.hidden }
            ZStack(alignment: .topLeading) {
                ForEach(visible) { key in
                    let rect = key.rect(in: geometry.size).insetBy(dx: 3, dy: 3)
                    // Only drawn: the touch area below reads the touches, and a
                    // touch beside the keys goes through to the screen.
                    KeyFace(label: key.label, pressed: pressed.contains(key.index), size: rect.size, labelScale: labelScale)
                        .allowsHitTesting(false)
                        .position(x: rect.midX, y: rect.midY)
                }
                .opacity(opacity)

                TouchArea(regions: visible.map { ($0.index, $0.rect(in: geometry.size)) }, haptics: haptics) { index, down in
                    if down {
                        pressed.insert(index)
                    } else {
                        pressed.remove(index)
                    }
                    onKey(index, down)
                }
                .frame(width: geometry.size.width, height: geometry.size.height)
            }
        }
    }
}

/// The UIKit view under the pad's touches.
private struct TouchArea: UIViewRepresentable {
    let regions: [(Int32, CGRect)]
    let haptics: Bool
    let onChange: (Int32, Bool) -> Void

    func makeUIView(context: Context) -> PadTouchView {
        PadTouchView()
    }

    func updateUIView(_ view: PadTouchView, context: Context) {
        view.regions = regions
        view.haptics = haptics
        view.onChange = onChange
    }

    static func dismantleUIView(_ view: PadTouchView, coordinator: ()) {
        view.releaseAll()
    }
}

final class PadTouchView: UIView {
    var regions: [(Int32, CGRect)] = []
    var haptics = true
    var onChange: ((Int32, Bool) -> Void)?

    /// The key each finger is on.
    private var held: [ObjectIdentifier: Int32] = [:]
    /// How many fingers are on each key, so two on one key release it once.
    private var counts: [Int32: Int] = [:]
    private let feedback = UIImpactFeedbackGenerator(style: .light)

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        isMultipleTouchEnabled = true
    }

    private func key(at point: CGPoint) -> Int32? {
        // The last drawn is on top.
        regions.last(where: { $0.1.contains(point) })?.0
    }

    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        key(at: point) != nil
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            guard let index = key(at: touch.location(in: self)) else { continue }
            held[ObjectIdentifier(touch)] = index
            press(index)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        for touch in touches {
            let id = ObjectIdentifier(touch)
            let now = key(at: touch.location(in: self))
            let before = held[id]
            guard now != before else { continue }
            if let before { release(before) }
            if let now { press(now) }
            held[id] = now
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        lift(touches)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        lift(touches)
    }

    private func lift(_ touches: Set<UITouch>) {
        for touch in touches {
            if let index = held.removeValue(forKey: ObjectIdentifier(touch)) {
                release(index)
            }
        }
    }

    func releaseAll() {
        held.removeAll()
        for index in counts.keys {
            onChange?(index, false)
        }
        counts.removeAll()
    }

    private func press(_ index: Int32) {
        counts[index, default: 0] += 1
        if counts[index] == 1 {
            if haptics { feedback.impactOccurred() }
            onChange?(index, true)
        }
    }

    private func release(_ index: Int32) {
        guard let count = counts[index] else { return }
        if count <= 1 {
            counts.removeValue(forKey: index)
            onChange?(index, false)
        } else {
            counts[index] = count - 1
        }
    }
}

/// The pad being edited: a key dragged moves, the handle on the chosen key's
/// corner resizes it. Positions snap to a fine grid so rows line up.
struct PadEditor: View {
    @Binding var layout: PadLayout
    @Binding var selected: Int32?
    let labelScale: Double

    /// The key as it was when the current drag began.
    @State private var origin: PadKey?

    private static let snap = 0.005
    private static let smallest = 0.05

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            ZStack(alignment: .topLeading) {
                grid(size)

                ForEach(layout.keys) { key in
                    let rect = key.rect(in: size).insetBy(dx: 3, dy: 3)
                    KeyFace(label: key.label, pressed: selected == key.index, size: rect.size, labelScale: labelScale)
                        .opacity(key.hidden ? 0.35 : 1)
                        .overlay(
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(
                                    selected == key.index ? Color.yellow : Color.clear,
                                    style: StrokeStyle(lineWidth: 2, dash: key.hidden ? [5, 4] : [])
                                )
                        )
                        .gesture(move(key, in: size))
                        .position(x: rect.midX, y: rect.midY)
                }

                if let index = selected, let key = layout.keys.first(where: { $0.index == index }) {
                    let rect = key.rect(in: size)
                    Circle()
                        .fill(Color.yellow)
                        .frame(width: 22, height: 22)
                        .overlay(Image(systemName: "arrow.up.left.and.arrow.down.right").font(.system(size: 10, weight: .bold)).foregroundColor(.black))
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        .gesture(resize(key, in: size))
                        .position(x: rect.maxX - 4, y: rect.maxY - 4)
                }
            }
        }
    }

    private func grid(_ size: CGSize) -> some View {
        Path { path in
            for step in 1..<12 {
                let x = size.width * Double(step) / 12
                let y = size.height * Double(step) / 12
                path.move(to: CGPoint(x: x, y: 0))
                path.addLine(to: CGPoint(x: x, y: size.height))
                path.move(to: CGPoint(x: 0, y: y))
                path.addLine(to: CGPoint(x: size.width, y: y))
            }
        }
        .stroke(Color.white.opacity(0.08), lineWidth: 1)
    }

    private func move(_ key: PadKey, in size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                if origin?.index != key.index {
                    origin = key
                    selected = key.index
                }
                guard let start = origin, size.width > 0, size.height > 0 else { return }
                update(key.index) { moved in
                    moved.x = Self.snapped(min(max(start.x + value.translation.width / size.width, 0), 1 - moved.w))
                    moved.y = Self.snapped(min(max(start.y + value.translation.height / size.height, 0), 1 - moved.h))
                }
            }
            .onEnded { _ in origin = nil }
    }

    private func resize(_ key: PadKey, in size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                if origin?.index != key.index {
                    origin = key
                }
                guard let start = origin, size.width > 0, size.height > 0 else { return }
                update(key.index) { resized in
                    resized.w = Self.snapped(min(max(start.w + value.translation.width / size.width, Self.smallest), 1 - resized.x))
                    resized.h = Self.snapped(min(max(start.h + value.translation.height / size.height, Self.smallest), 1 - resized.y))
                }
            }
            .onEnded { _ in origin = nil }
    }

    private func update(_ index: Int32, _ change: (inout PadKey) -> Void) {
        guard let position = layout.keys.firstIndex(where: { $0.index == index }) else { return }
        change(&layout.keys[position])
    }

    private static func snapped(_ value: Double) -> Double {
        (value / snap).rounded() * snap
    }
}
