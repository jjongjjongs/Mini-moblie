import SwiftUI

/// How the pad looks: one of three faces, picked in 화면·패드 설정 or the
/// game menu, the same three the Android app offers. Only colours and corner
/// shapes - where the keys are is the layout's business, so a player's own
/// key positions survive a change of look.
enum PadTheme: String, CaseIterable, Identifiable {
    case gold
    case glass
    case silver

    var id: String { rawValue }

    var label: String {
        switch self {
        case .gold: return "골드"
        case .glass: return "다크 글래스"
        case .silver: return "실버"
        }
    }

    var palette: PadPalette {
        switch self {
        case .gold: return .gold
        case .glass: return .glass
        case .silver: return .silver
        }
    }
}

/// Whether a title uses the ordinary pad or 숫자 크게, which gives the number
/// pad the room for a rhythm game played on the numbers. Kept per title.
enum PadShape: String, CaseIterable, Identifiable {
    case standard
    case numbers

    var id: String { rawValue }

    var label: String {
        switch self {
        case .standard: return "기본"
        case .numbers: return "숫자 크게"
        }
    }
}

/// A design's colours.
struct PadPalette {
    var trayTop: Color
    var trayBottom: Color

    var faceTop: Color
    var faceBottom: Color
    /// nil for a key without an outline.
    var edge: Color?
    var ink: Color
    var subInk: Color
    var pressedTop: Color
    var pressedBottom: Color
    var pressedInk: Color
    /// Drawn a little below a key so it reads as raised.
    var shadow: Color?
    var radius: CGFloat
    /// Number keys as pills rather than rounded squares.
    var pillNumbers: Bool

    /// The function keys - the soft keys, 저장 and 뒤로. A hollow one is an
    /// outline over the tray rather than a key face.
    var hollowFunctions: Bool
    var functionEdge: Color
    var functionInk: Color
    var pillFunctions: Bool
    var functionRadius: CGFloat
    /// 저장 filled with this rather than the ordinary face.
    var saveFill: Color?
    var saveEdge: Color
    var saveInk: Color
    var backEdge: Color
    var backInk: Color

    /// The direction disc.
    var ringTop: Color
    var ringBottom: Color
    var ringEdge: Color
    var ringSeam: Color?
    var arrow: Color
    var arrowPressed: Color
    var wedgePressed: Color
    var okTop: Color
    var okBottom: Color
    var okEdge: Color?
    var okInk: Color

    var tray: LinearGradient {
        LinearGradient(colors: [trayTop, trayBottom], startPoint: .top, endPoint: .bottom)
    }

    private static func rgb(_ red: Double, _ green: Double, _ blue: Double, _ alpha: Double = 1) -> Color {
        Color(.sRGB, red: red / 255, green: green / 255, blue: blue / 255, opacity: alpha)
    }

    static let gold = PadPalette(
        trayTop: rgb(22, 19, 15), trayBottom: rgb(42, 36, 29),
        faceTop: rgb(58, 50, 41), faceBottom: rgb(41, 35, 28), edge: rgb(110, 90, 58),
        ink: rgb(236, 208, 155), subInk: rgb(165, 142, 100),
        pressedTop: rgb(93, 78, 58), pressedBottom: rgb(74, 62, 47), pressedInk: rgb(255, 243, 214),
        shadow: rgb(13, 11, 8), radius: 13, pillNumbers: false,
        hollowFunctions: false, functionEdge: rgb(110, 90, 58), functionInk: rgb(236, 208, 155),
        pillFunctions: true, functionRadius: 13,
        saveFill: nil, saveEdge: rgb(110, 90, 58), saveInk: rgb(143, 210, 143),
        backEdge: rgb(110, 90, 58), backInk: rgb(231, 118, 108),
        ringTop: rgb(53, 45, 37), ringBottom: rgb(43, 37, 30), ringEdge: rgb(110, 90, 58),
        ringSeam: rgb(110, 90, 58, 0.55), arrow: rgb(236, 208, 155), arrowPressed: .white,
        wedgePressed: rgb(233, 196, 127, 0.22),
        okTop: rgb(67, 57, 48), okBottom: rgb(47, 40, 32), okEdge: rgb(140, 113, 72), okInk: rgb(236, 208, 155)
    )

    static let glass = PadPalette(
        trayTop: rgb(17, 18, 22), trayBottom: rgb(17, 18, 22),
        faceTop: rgb(34, 37, 45), faceBottom: rgb(34, 37, 45), edge: nil,
        ink: rgb(236, 238, 242), subInk: rgb(125, 134, 150),
        pressedTop: rgb(58, 142, 154), pressedBottom: rgb(58, 142, 154), pressedInk: .white,
        shadow: nil, radius: 16, pillNumbers: false,
        hollowFunctions: true, functionEdge: rgb(52, 56, 68), functionInk: rgb(198, 202, 211),
        pillFunctions: true, functionRadius: 16,
        saveFill: rgb(84, 199, 214), saveEdge: rgb(84, 199, 214), saveInk: rgb(12, 42, 47),
        backEdge: rgb(74, 47, 49), backInk: rgb(255, 138, 128),
        ringTop: rgb(27, 29, 35), ringBottom: rgb(27, 29, 35), ringEdge: rgb(44, 48, 58),
        ringSeam: rgb(42, 46, 55), arrow: rgb(154, 163, 178), arrowPressed: rgb(84, 199, 214),
        wedgePressed: rgb(84, 199, 214, 0.2),
        okTop: rgb(43, 47, 57), okBottom: rgb(43, 47, 57), okEdge: nil, okInk: rgb(236, 238, 242)
    )

    static let silver = PadPalette(
        trayTop: rgb(215, 219, 224), trayBottom: rgb(185, 191, 199),
        faceTop: rgb(251, 252, 253), faceBottom: rgb(227, 231, 236), edge: rgb(154, 162, 173),
        ink: rgb(28, 35, 48), subInk: rgb(93, 102, 117),
        pressedTop: rgb(205, 211, 219), pressedBottom: rgb(191, 198, 207), pressedInk: rgb(28, 35, 48),
        shadow: rgb(141, 148, 158), radius: 12, pillNumbers: true,
        hollowFunctions: false, functionEdge: rgb(154, 162, 173), functionInk: rgb(28, 35, 48),
        pillFunctions: false, functionRadius: 12,
        saveFill: nil, saveEdge: rgb(154, 162, 173), saveInk: rgb(27, 143, 58),
        backEdge: rgb(154, 162, 173), backInk: rgb(200, 53, 43),
        ringTop: rgb(244, 246, 248), ringBottom: rgb(184, 190, 199), ringEdge: rgb(143, 151, 162),
        ringSeam: nil, arrow: rgb(58, 66, 80), arrowPressed: rgb(11, 99, 201),
        wedgePressed: rgb(11, 99, 201, 0.16),
        okTop: rgb(223, 227, 232), okBottom: rgb(195, 201, 209), okEdge: rgb(143, 151, 162), okInk: rgb(28, 35, 48)
    )
}

/// The direction keys and OK drawn as one disc, where they stand as a plus.
struct PadRing: Equatable {
    var center: CGPoint
    var radius: CGFloat

    /// The share of the radius OK takes in the middle.
    static let okShare: CGFloat = 0.36

    var bounds: CGRect {
        CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
    }

    /// The key a finger at `point` presses: OK in the middle, otherwise the
    /// quarter it is in - so the corners between the arms, which no key's
    /// rectangle covers, are directions too. nil off the disc.
    func key(at point: CGPoint) -> Int32? {
        let dx = point.x - center.x
        let dy = point.y - center.y
        let distance = (dx * dx + dy * dy).squareRoot()
        guard distance <= radius else { return nil }
        if distance <= radius * Self.okShare {
            return Int32(WIE_KEY_OK)
        }
        let angle = atan2(dy, dx) * 180 / .pi
        if angle >= -45 && angle < 45 { return Int32(WIE_KEY_RIGHT) }
        if angle >= 45 && angle < 135 { return Int32(WIE_KEY_DOWN) }
        if angle >= -135 && angle < -45 { return Int32(WIE_KEY_UP) }
        return Int32(WIE_KEY_LEFT)
    }
}

/// A quarter of the disc, for a held direction.
private struct Wedge: Shape {
    let start: Double

    func path(in rect: CGRect) -> Path {
        var path = Path()
        let center = CGPoint(x: rect.midX, y: rect.midY)
        let radius = rect.width / 2
        path.move(to: center)
        // Sampled rather than an arc, so the quarter swept is plainly the one
        // from `start` a quarter turn on, angles growing towards +y.
        for step in 0...16 {
            let angle = (start + 90 * Double(step) / 16) * .pi / 180
            path.addLine(to: CGPoint(x: center.x + radius * CGFloat(cos(angle)), y: center.y + radius * CGFloat(sin(angle))))
        }
        path.closeSubpath()
        return path
    }
}

/// The disc itself: quartered by faint seams, an arrow on each quarter, OK a
/// raised button in the middle, a held direction lighting its whole quarter.
struct RingFace: View {
    let pressed: Set<Int32>
    let diameter: CGFloat
    let palette: PadPalette

    var body: some View {
        let radius = diameter / 2
        let inner = radius * PadRing.okShare
        let quarters: [(Int32, Double, String, CGSize)] = [
            (Int32(WIE_KEY_RIGHT), -45, "▶︎", CGSize(width: radius * 0.7, height: 0)),
            (Int32(WIE_KEY_DOWN), 45, "▼", CGSize(width: 0, height: radius * 0.7)),
            (Int32(WIE_KEY_LEFT), 135, "◀︎", CGSize(width: -radius * 0.7, height: 0)),
            (Int32(WIE_KEY_UP), 225, "▲", CGSize(width: 0, height: -radius * 0.7)),
        ]
        let okHeld = pressed.contains(Int32(WIE_KEY_OK))

        return ZStack {
            if let shadow = palette.shadow {
                Circle().fill(shadow).offset(y: 2)
            }
            Circle().fill(LinearGradient(colors: [palette.ringTop, palette.ringBottom], startPoint: .top, endPoint: .bottom))
            ForEach(quarters, id: \.0) { quarter in
                if pressed.contains(quarter.0) {
                    Wedge(start: quarter.1).fill(palette.wedgePressed)
                }
            }
            if let seam = palette.ringSeam {
                Path { path in
                    for step in 0..<4 {
                        let angle = Double(45 + step * 90) * .pi / 180
                        let dx = CGFloat(cos(angle))
                        let dy = CGFloat(sin(angle))
                        path.move(to: CGPoint(x: radius + dx * inner, y: radius + dy * inner))
                        path.addLine(to: CGPoint(x: radius + dx * radius, y: radius + dy * radius))
                    }
                }
                .stroke(seam, lineWidth: 1)
            }
            Circle().stroke(palette.ringEdge, lineWidth: 1)
            ForEach(quarters, id: \.0) { quarter in
                Text(quarter.2)
                    .font(.system(size: radius * 0.2))
                    .foregroundColor(pressed.contains(quarter.0) ? palette.arrowPressed : palette.arrow)
                    .offset(quarter.3)
            }
            ZStack {
                Circle().fill(LinearGradient(
                    colors: okHeld ? [palette.pressedTop, palette.pressedBottom] : [palette.okTop, palette.okBottom],
                    startPoint: .top, endPoint: .bottom
                ))
                if let edge = palette.okEdge {
                    Circle().stroke(edge, lineWidth: 1)
                }
                Text("OK")
                    .font(.system(size: inner * 0.55, weight: .bold))
                    .foregroundColor(okHeld ? palette.pressedInk : palette.okInk)
            }
            .frame(width: inner * 2, height: inner * 2)
        }
        .frame(width: diameter, height: diameter)
    }
}
