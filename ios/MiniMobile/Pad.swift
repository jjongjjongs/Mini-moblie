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
        Int32(WIE_KEY_LEFT_SOFT): "L",
        Int32(WIE_KEY_RIGHT_SOFT): "R",
        Int32(WIE_KEY_CLEAR): "뒤로",
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
        Int32(WIE_KEY_STAR): "✱",
        Int32(WIE_KEY_HASH): "#",
        Int32(WIE_KEY_CALL): "저장",
        Int32(WIE_KEY_HANGUP): "종료",
    ]

    /// What a Korean handset printed beside each digit: the 천지인 strokes
    /// on 1-3 and the jamo pairs on 4-0, with the Latin run each key writes -
    /// the same engraving the Android pad carries.
    static let letters: [Int32: (String, String?)] = [
        Int32(WIE_KEY_NUM0) + 1: ("ㅣ", "@:/"),
        Int32(WIE_KEY_NUM0) + 2: ("ㆍ", "ABC"),
        Int32(WIE_KEY_NUM0) + 3: ("ㅡ", "DEF"),
        Int32(WIE_KEY_NUM0) + 4: ("ㄱㅋ", "GHI"),
        Int32(WIE_KEY_NUM0) + 5: ("ㄴㄹ", "JKL"),
        Int32(WIE_KEY_NUM0) + 6: ("ㄷㅌ", "MNO"),
        Int32(WIE_KEY_NUM0) + 7: ("ㅂㅍ", "PQRS"),
        Int32(WIE_KEY_NUM0) + 8: ("ㅅㅎ", "TUV"),
        Int32(WIE_KEY_NUM0) + 9: ("ㅈㅊ", "WXYZ"),
        Int32(WIE_KEY_NUM0): ("ㅇㅁ", ".,?!"),
        Int32(WIE_KEY_HASH): ("공백", nil),
    ]

    enum Kind {
        case soft
        case save
        case back
        case number
        case plain
    }

    var kind: Kind {
        switch index {
        case Int32(WIE_KEY_LEFT_SOFT), Int32(WIE_KEY_RIGHT_SOFT): return .soft
        case Int32(WIE_KEY_CALL): return .save
        case Int32(WIE_KEY_CLEAR), Int32(WIE_KEY_HANGUP): return .back
        case Int32(WIE_KEY_NUM0)...Int32(WIE_KEY_HASH): return .number
        default: return .plain
        }
    }
}

/// Which standard layout a pad starts from: under the screen, under it as
/// 숫자 크게, or over the screen.
enum PadArrangement {
    case below
    case numbers
    case overlay
}

/// Every key of the pad, placed.
struct PadLayout: Codable, Equatable {
    var keys: [PadKey]

    /// The layout kept under `text`, or the standard one when there is none
    /// (or it does not read). Keys a stored layout lacks come from the
    /// standard one, so a key added later still shows up.
    static func decode(_ text: String, standard arrangement: PadArrangement) -> PadLayout {
        let standard = standard(arrangement)
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

    static func standard(_ arrangement: PadArrangement) -> PadLayout {
        switch arrangement {
        case .below: return below
        case .numbers: return numbers
        case .overlay: return overlay
        }
    }

    /// The direction keys and OK, which draw as one disc while they stand as a plus.
    static let ringKeys: Set<Int32> = [
        Int32(WIE_KEY_UP), Int32(WIE_KEY_DOWN), Int32(WIE_KEY_LEFT), Int32(WIE_KEY_RIGHT), Int32(WIE_KEY_OK),
    ]

    /// The disc the direction keys and OK make in an area of `size`, if they
    /// make one: all five shown, still standing as a plus round OK, and no
    /// other shown key sitting on the circle - a key a player tucked into a
    /// corner of the plus would lose its touches to the disc.
    func ring(in size: CGSize) -> PadRing? {
        func rect(_ index: Int32) -> CGRect? {
            guard let key = keys.first(where: { $0.index == Int32(index) }), !key.hidden else { return nil }
            return key.rect(in: size)
        }
        guard let up = rect(Int32(WIE_KEY_UP)), let down = rect(Int32(WIE_KEY_DOWN)), let left = rect(Int32(WIE_KEY_LEFT)),
              let right = rect(Int32(WIE_KEY_RIGHT)), let ok = rect(Int32(WIE_KEY_OK)) else { return nil }
        let center = CGPoint(x: ok.midX, y: ok.midY)
        let tolerance = max(ok.width, ok.height) * 0.3
        guard abs(up.midX - center.x) < tolerance, abs(down.midX - center.x) < tolerance,
              abs(left.midY - center.y) < tolerance, abs(right.midY - center.y) < tolerance,
              up.maxY <= center.y, down.minY >= center.y, left.maxX <= center.x, right.minX >= center.x else { return nil }
        let radius = min(min(center.y - up.minY, down.maxY - center.y), min(center.x - left.minX, right.maxX - center.x))
        for key in keys where !key.hidden && !Self.ringKeys.contains(key.index) {
            let frame = key.rect(in: size)
            let nearX = max(frame.minX, min(center.x, frame.maxX))
            let nearY = max(frame.minY, min(center.y, frame.maxY))
            if ((nearX - center.x) * (nearX - center.x) + (nearY - center.y) * (nearY - center.y)).squareRoot() < radius - 1 {
                return nil
            }
        }
        return PadRing(center: center, radius: radius)
    }

    /// Under the screen, as the Android pad lays it: a function row over the
    /// whole width - the soft keys over the directions, 저장 and 뒤로 over the
    /// numbers - then the directions on the left and the number pad on the
    /// right.
    private static let below: PadLayout = {
        let padX = 0.015, padY = 0.02, gapX = 0.012, gapY = 0.015
        let half = (1 - 2 * padX - gapX) / 2
        let leftX = padX, rightX = padX + half + gapX
        let usable = 1 - 2 * padY
        let topRow = (usable - gapY) * 0.17
        let rest = usable - gapY - topRow
        let padTop = padY + topRow + gapY
        let functionWidth = (half - gapX) / 2

        var keys: [PadKey] = [
            PadKey(index: Int32(WIE_KEY_LEFT_SOFT), x: leftX, y: padY, w: functionWidth, h: topRow, hidden: false),
            PadKey(index: Int32(WIE_KEY_RIGHT_SOFT), x: leftX + functionWidth + gapX, y: padY, w: functionWidth, h: topRow, hidden: false),
            PadKey(index: Int32(WIE_KEY_CALL), x: rightX, y: padY, w: functionWidth, h: topRow, hidden: false),
            PadKey(index: Int32(WIE_KEY_CLEAR), x: rightX + functionWidth + gapX, y: padY, w: functionWidth, h: topRow, hidden: false),
        ]

        let cellWidth = (half - 2 * gapX) / 3, cellHeight = (rest - 2 * gapY) / 3
        let plus: [(Int32, Double, Double)] = [
            (Int32(WIE_KEY_UP), 1, 0), (Int32(WIE_KEY_LEFT), 0, 1), (Int32(WIE_KEY_OK), 1, 1),
            (Int32(WIE_KEY_RIGHT), 2, 1), (Int32(WIE_KEY_DOWN), 1, 2),
        ]
        for (index, column, row) in plus {
            keys.append(PadKey(index: index, x: leftX + column * (cellWidth + gapX), y: padTop + row * (cellHeight + gapY),
                               w: cellWidth, h: cellHeight, hidden: false))
        }

        let numberWidth = (half - 2 * gapX) / 3, numberHeight = (rest - 3 * gapY) / 4
        for (position, index) in PadLayout.numberOrder.enumerated() {
            keys.append(PadKey(index: index, x: rightX + Double(position % 3) * (numberWidth + gapX),
                               y: padTop + Double(position / 3) * (numberHeight + gapY), w: numberWidth, h: numberHeight, hidden: false))
        }

        // 종료 has no place on the handset grid; it is there to be shown and
        // placed by the player that needs it.
        keys.append(PadKey(index: Int32(WIE_KEY_HANGUP), x: rightX + functionWidth + gapX, y: padY + topRow + gapY,
                           w: functionWidth, h: topRow, hidden: true))
        return PadLayout(keys: keys)
    }()

    /// 숫자 크게, for a rhythm game played on the numbers: the pad keeps its
    /// room; inside it the number pad sits centred and a size up, capped so
    /// the keys stay keys, and everything else shares one slim row above it.
    private static let numbers: PadLayout = {
        let padX = 0.015, padY = 0.02, gapX = 0.012, gapY = 0.018
        let strip = 0.13
        let order: [(Int32, Double)] = [
            (Int32(WIE_KEY_LEFT_SOFT), 1), (Int32(WIE_KEY_RIGHT_SOFT), 1), (Int32(WIE_KEY_LEFT), 1), (Int32(WIE_KEY_UP), 1),
            (Int32(WIE_KEY_DOWN), 1), (Int32(WIE_KEY_RIGHT), 1), (Int32(WIE_KEY_OK), 1), (Int32(WIE_KEY_CALL), 1.3),
            (Int32(WIE_KEY_CLEAR), 1.3),
        ]
        let total = order.reduce(0) { $0 + $1.1 }
        let unit = (1 - 2 * padX - gapX * Double(order.count - 1)) / total
        var keys: [PadKey] = []
        var x = padX
        for (index, weight) in order {
            keys.append(PadKey(index: index, x: x, y: padY, w: unit * weight, h: strip, hidden: false))
            x += unit * weight + gapX
        }

        let top = padY + strip + gapY * 1.6
        let numberWidth = min((1 - 2 * padX - 2 * gapX) / 3, 0.25)
        let numberHeight = (1 - top - padY - 3 * gapY) / 4
        let left = (1 - 3 * numberWidth - 2 * gapX) / 2
        for (position, index) in PadLayout.numberOrder.enumerated() {
            keys.append(PadKey(index: index, x: left + Double(position % 3) * (numberWidth + gapX),
                               y: top + Double(position / 3) * (numberHeight + gapY), w: numberWidth, h: numberHeight, hidden: false))
        }
        keys.append(PadKey(index: Int32(WIE_KEY_HANGUP), x: 1 - padX - unit * 1.3, y: top, w: unit * 1.3, h: strip, hidden: true))
        return PadLayout(keys: keys)
    }()

    /// 1 to 9, then ✱ 0 #.
    private static let numberOrder: [Int32] = (1...9).map { Int32(WIE_KEY_NUM0) + Int32($0) }
        + [Int32(WIE_KEY_STAR), Int32(WIE_KEY_NUM0), Int32(WIE_KEY_HASH)]

    /// Over the screen: the direction keys under the left thumb, the number
    /// keys under the right, the soft keys and 뒤로/저장 above them.
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

/// How a key looks, pressed or not, in the chosen design.
struct KeyFace: View {
    let key: PadKey
    let pressed: Bool
    let size: CGSize
    let labelScale: Double
    let palette: PadPalette
    /// Whether a digit carries the letters printed beside it; the slim keys of
    /// 숫자 크게 and the pictures in the settings leave them off.
    var letters = true

    var body: some View {
        let kind = key.kind
        let function = kind == .soft || kind == .save || kind == .back
        let short = min(size.width, size.height)
        let radius: CGFloat = function
            ? (palette.pillFunctions ? short / 2 : min(palette.functionRadius, short / 2))
            : (kind == .number && palette.pillNumbers ? short / 2 : min(palette.radius, short / 2))
        var hollow = function && palette.hollowFunctions
        var top = palette.faceTop
        var bottom = palette.faceBottom
        if kind == .save, let fill = palette.saveFill {
            top = fill
            bottom = fill
            hollow = false
        }
        if pressed {
            top = palette.pressedTop
            bottom = palette.pressedBottom
            hollow = false
        }
        let edge: Color? = !function ? palette.edge
            : kind == .save ? palette.saveEdge : kind == .back ? palette.backEdge : palette.functionEdge
        let ink = pressed ? palette.pressedInk
            : !function ? palette.ink
            : kind == .save ? palette.saveInk : kind == .back ? palette.backInk : palette.functionInk
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)

        return ZStack {
            if let shadow = palette.shadow, !hollow, !pressed {
                shape.fill(shadow).offset(y: 2)
            }
            if !hollow {
                shape.fill(LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom))
            }
            if let edge, !(pressed && palette.edge == nil) {
                shape.stroke(edge, lineWidth: 1)
            }
            label(ink: ink, function: function)
        }
        .frame(width: size.width, height: size.height)
    }

    @ViewBuilder
    private func label(ink: Color, function: Bool) -> some View {
        let base = max(9, min(size.height * (function ? 0.38 : 0.42), size.width * 0.32) * labelScale)
        if letters, let engraved = PadKey.letters[key.index], size.width > 44 {
            // Engraved the way a handset prints it: the digit, and the letters
            // stacked beside it.
            HStack(spacing: size.width * 0.06) {
                Text(key.label)
                    .font(.system(size: base, weight: .bold))
                VStack(spacing: 0) {
                    Text(engraved.0)
                    if let latin = engraved.1 {
                        Text(latin)
                    }
                }
                .font(.system(size: base * 0.42))
                .foregroundColor(pressed ? palette.pressedInk : palette.subInk)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
            }
            .foregroundColor(ink)
            .padding(.horizontal, 2)
        } else {
            Text(key.label)
                .font(.system(size: base, weight: .bold))
                .foregroundColor(ink)
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .padding(.horizontal, 2)
        }
    }
}

/// The keys drawn and nothing else - the disc where the directions make one -
/// for play, for the editor and for the pictures in the settings.
struct PadFaces: View {
    let layout: PadLayout
    let size: CGSize
    let pressed: Set<Int32>
    let labelScale: Double
    let palette: PadPalette
    var letters = true
    /// Keys drawn faded: the hidden ones, in the editor.
    var showHidden = false

    var body: some View {
        let ring = layout.ring(in: size)
        let keys = layout.keys.filter { (showHidden || !$0.hidden) && !(ring != nil && PadLayout.ringKeys.contains($0.index)) }
        return ZStack(alignment: .topLeading) {
            if let ring {
                RingFace(pressed: pressed, diameter: ring.radius * 2, palette: palette)
                    .position(x: ring.center.x, y: ring.center.y)
            }
            ForEach(keys) { key in
                let rect = key.rect(in: size).insetBy(dx: 3, dy: 3)
                KeyFace(key: key, pressed: pressed.contains(key.index), size: rect.size, labelScale: labelScale,
                        palette: palette, letters: letters)
                    .opacity(key.hidden ? 0.35 : 1)
                    .position(x: rect.midX, y: rect.midY)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
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
    let palette: PadPalette
    var letters = true
    let onKey: (Int32, Bool) -> Void

    @State private var pressed = Set<Int32>()

    var body: some View {
        GeometryReader { geometry in
            let visible = layout.keys.filter { !$0.hidden }
            ZStack(alignment: .topLeading) {
                // Only drawn: the touch area below reads the touches, and a
                // touch beside the keys goes through to the screen.
                PadFaces(layout: layout, size: geometry.size, pressed: pressed, labelScale: labelScale, palette: palette, letters: letters)
                    .allowsHitTesting(false)
                    .opacity(opacity)

                TouchArea(
                    regions: visible.map { ($0.index, $0.rect(in: geometry.size)) },
                    ring: layout.ring(in: geometry.size),
                    haptics: haptics
                ) { index, down in
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
    let ring: PadRing?
    let haptics: Bool
    let onChange: (Int32, Bool) -> Void

    func makeUIView(context: Context) -> PadTouchView {
        PadTouchView()
    }

    func updateUIView(_ view: PadTouchView, context: Context) {
        view.regions = regions
        view.ring = ring
        view.haptics = haptics
        view.onChange = onChange
    }

    static func dismantleUIView(_ view: PadTouchView, coordinator: ()) {
        view.releaseAll()
    }
}

final class PadTouchView: UIView {
    var regions: [(Int32, CGRect)] = []
    /// The disc the directions and OK are drawn as, read by angle.
    var ring: PadRing?
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
        if let ring, let index = ring.key(at: point) {
            return index
        }
        // The last drawn is on top.
        return regions.last(where: { $0.1.contains(point) })?.0
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
/// corner resizes it. Positions snap to a fine grid so rows line up. Where the
/// directions and OK make a disc, the disc moves and sizes as one.
struct PadEditor: View {
    @Binding var layout: PadLayout
    @Binding var selected: Int32?
    let labelScale: Double
    let palette: PadPalette

    /// The key as it was when the current drag began.
    @State private var origin: PadKey?
    /// The disc's five keys as they were when the current drag of it began.
    @State private var groupOrigin: [PadKey]?

    private static let snap = 0.005
    private static let smallest = 0.05

    var body: some View {
        GeometryReader { geometry in
            let size = geometry.size
            let ring = layout.ring(in: size)
            let ringSelected = ring != nil && selected.map { PadLayout.ringKeys.contains($0) } == true
            ZStack(alignment: .topLeading) {
                grid(size)

                if let ring {
                    RingFace(pressed: [], diameter: ring.radius * 2, palette: palette)
                        .overlay(Circle().stroke(ringSelected ? Color.yellow : Color.clear, lineWidth: 2))
                        .gesture(moveRing(in: size))
                        .position(x: ring.center.x, y: ring.center.y)
                }

                ForEach(layout.keys.filter { !(ring != nil && PadLayout.ringKeys.contains($0.index)) }) { key in
                    let rect = key.rect(in: size).insetBy(dx: 3, dy: 3)
                    KeyFace(key: key, pressed: false, size: rect.size, labelScale: labelScale, palette: palette)
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

                if let ring, ringSelected {
                    handle
                        .gesture(resizeRing(in: size))
                        .position(x: ring.bounds.maxX - 4, y: ring.bounds.maxY - 4)
                } else if let index = selected, let key = layout.keys.first(where: { $0.index == index }) {
                    let rect = key.rect(in: size)
                    handle
                        .gesture(resize(key, in: size))
                        .position(x: rect.maxX - 4, y: rect.maxY - 4)
                }
            }
        }
    }

    private var handle: some View {
        Circle()
            .fill(Color.yellow)
            .frame(width: 22, height: 22)
            .overlay(Image(systemName: "arrow.up.left.and.arrow.down.right").font(.system(size: 10, weight: .bold)).foregroundColor(.black))
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
    }

    /// The five keys of the disc, as they stand now.
    private var ringGroup: [PadKey] {
        layout.keys.filter { PadLayout.ringKeys.contains($0.index) }
    }

    /// The fractions of the area the five keys span.
    private static func span(_ keys: [PadKey]) -> (x: Double, y: Double, w: Double, h: Double) {
        let left = keys.map(\.x).min() ?? 0
        let top = keys.map(\.y).min() ?? 0
        let right = keys.map { $0.x + $0.w }.max() ?? 0
        let bottom = keys.map { $0.y + $0.h }.max() ?? 0
        return (left, top, right - left, bottom - top)
    }

    private func moveRing(in size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                if groupOrigin == nil {
                    groupOrigin = ringGroup
                    selected = Int32(WIE_KEY_OK)
                }
                guard let start = groupOrigin, size.width > 0, size.height > 0 else { return }
                let box = Self.span(start)
                let dx = Self.snapped(min(max(value.translation.width / size.width, -box.x), 1 - box.x - box.w))
                let dy = Self.snapped(min(max(value.translation.height / size.height, -box.y), 1 - box.y - box.h))
                for key in start {
                    update(key.index) { moved in
                        moved.x = key.x + dx
                        moved.y = key.y + dy
                    }
                }
            }
            .onEnded { _ in groupOrigin = nil }
    }

    private func resizeRing(in size: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                if groupOrigin == nil {
                    groupOrigin = ringGroup
                }
                guard let start = groupOrigin, size.width > 0, size.height > 0 else { return }
                let box = Self.span(start)
                guard box.w > 0, box.h > 0 else { return }
                let width = min(max(box.w + value.translation.width / size.width, Self.smallest * 3), 1 - box.x)
                let height = min(max(box.h + value.translation.height / size.height, Self.smallest * 3), 1 - box.y)
                let scaleX = width / box.w
                let scaleY = height / box.h
                for key in start {
                    update(key.index) { sized in
                        sized.x = Self.snapped(box.x + (key.x - box.x) * scaleX)
                        sized.y = Self.snapped(box.y + (key.y - box.y) * scaleY)
                        sized.w = Self.snapped(key.w * scaleX)
                        sized.h = Self.snapped(key.h * scaleY)
                    }
                }
            }
            .onEnded { _ in groupOrigin = nil }
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
