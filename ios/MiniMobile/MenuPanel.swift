import SwiftUI

/// The colours of the game menu's windows: the Android app's 조작 설정
/// panel, so a window opened over the game sits on it the same way there.
enum MenuPalette {
    static let panel = Color(red: 28 / 255, green: 30 / 255, blue: 36 / 255)
    static let ink = Color(red: 233 / 255, green: 234 / 255, blue: 237 / 255)
    static let muted = Color(red: 154 / 255, green: 156 / 255, blue: 166 / 255)
    static let line = Color(red: 43 / 255, green: 46 / 255, blue: 55 / 255)
    static let raised = Color(red: 35 / 255, green: 38 / 255, blue: 46 / 255)
    static let raisedLine = Color(red: 47 / 255, green: 50 / 255, blue: 59 / 255)
    static let accent = Color(red: 84 / 255, green: 199 / 255, blue: 214 / 255)
    static let onAccent = Color(red: 15 / 255, green: 42 / 255, blue: 46 / 255)
    static let outline = Color(red: 85 / 255, green: 90 / 255, blue: 102 / 255)
}

/// A button of a game menu window's footer: 적용 filled, 취소 raised beside
/// it, and one set apart from them (1x로) only outlined.
struct MenuButtonStyle: ButtonStyle {
    enum Kind {
        case primary
        case secondary
        case quiet
    }

    let kind: Kind

    func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: 14)
        return configuration.label
            .font(.system(size: 15, weight: .bold))
            .foregroundColor(foreground)
            .padding(.horizontal, 18)
            .frame(minHeight: 44)
            .background(shape.fill(fill))
            .overlay(shape.stroke(border, lineWidth: 1))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }

    private var foreground: Color {
        switch kind {
        case .primary: return MenuPalette.onAccent
        case .secondary: return MenuPalette.accent
        case .quiet: return MenuPalette.muted
        }
    }

    private var fill: Color {
        switch kind {
        case .primary: return MenuPalette.accent
        case .secondary: return MenuPalette.raised
        case .quiet: return Color.clear
        }
    }

    private var border: Color {
        switch kind {
        case .primary: return MenuPalette.accent
        case .secondary: return MenuPalette.raisedLine
        case .quiet: return MenuPalette.raisedLine
        }
    }
}

/// One of the game menu's windows: its title, what it holds, and its buttons
/// under a hairline, lifted off the bottom edge.
struct MenuPanel<Content: View, Footer: View>: View {
    let title: String
    let content: Content
    let footer: Footer

    init(title: String, @ViewBuilder content: () -> Content, @ViewBuilder footer: () -> Footer) {
        self.title = title
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        VStack(spacing: 0) {
            Text(title)
                .font(.system(size: 19, weight: .bold))
                .foregroundColor(MenuPalette.ink)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 22)
                .padding(.top, 22)
                .padding(.bottom, 10)
            ScrollView {
                content
                    .padding(.horizontal, 22)
                    .padding(.bottom, 16)
            }
            Rectangle()
                .fill(MenuPalette.line)
                .frame(height: 1)
            HStack(spacing: 8) {
                footer
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 20)
        }
        .background(MenuPalette.panel.ignoresSafeArea())
        .accentColor(MenuPalette.accent)
        .environment(\.colorScheme, .dark)
    }
}

/// The window asking before the game is closed, over the game and in the look
/// of the menu's others. A gamepad answers it too: A is 예, B is 아니요.
struct ExitConfirm: View {
    let onYes: () -> Void
    let onNo: () -> Void
    @State private var outerCapture: ((PadButton?) -> Void)?

    var body: some View {
        ZStack {
            Color.black.opacity(0.55)
                .ignoresSafeArea()
                .onTapGesture(perform: onNo)
            VStack(spacing: 0) {
                Text("종료")
                    .font(.system(size: 19, weight: .bold))
                    .foregroundColor(MenuPalette.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 22)
                    .padding(.top, 22)
                Text("애플리케이션을 종료하시겠습니까?")
                    .font(.system(size: 15))
                    .foregroundColor(MenuPalette.muted)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 22)
                    .padding(.top, 12)
                    .padding(.bottom, 16)
                Rectangle()
                    .fill(MenuPalette.line)
                    .frame(height: 1)
                HStack(spacing: 8) {
                    Spacer()
                    Button("아니요", action: onNo)
                        .buttonStyle(MenuButtonStyle(kind: .secondary))
                    Button("예", action: onYes)
                        .buttonStyle(MenuButtonStyle(kind: .primary))
                }
                .padding(.horizontal, 16)
                .padding(.top, 14)
                .padding(.bottom, 20)
            }
            .background(RoundedRectangle(cornerRadius: 16).fill(MenuPalette.panel))
            .overlay(RoundedRectangle(cornerRadius: 16).stroke(MenuPalette.line, lineWidth: 1))
            .frame(maxWidth: 360)
            .padding(.horizontal, 20)
        }
        .environment(\.colorScheme, .dark)
        .onAppear {
            // A key held into the window is let go: its release is the window's.
            Gamepad.shared.releaseAll()
            outerCapture = Gamepad.shared.capture
            Gamepad.shared.capture = { button in
                switch button {
                case .a?:
                    onYes()
                case .b?:
                    onNo()
                default:
                    break
                }
            }
        }
        .onDisappear {
            Gamepad.shared.capture = outerCapture
        }
    }
}
