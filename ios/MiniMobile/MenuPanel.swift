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

/// A window of the game menu over the game: the game dimmed behind it (a tap
/// there closes it), its title, what it holds - scrolling once it is taller
/// than the screen leaves room for - and its buttons under a hairline.
struct MenuDialog<Content: View, Footer: View>: View {
    let title: String
    let onDismiss: () -> Void
    let content: Content
    let footer: Footer
    @State private var contentHeight: CGFloat = 0

    init(title: String, onDismiss: @escaping () -> Void, @ViewBuilder content: () -> Content, @ViewBuilder footer: () -> Footer) {
        self.title = title
        self.onDismiss = onDismiss
        self.content = content()
        self.footer = footer()
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.black.opacity(0.55)
                    .ignoresSafeArea()
                    .onTapGesture(perform: onDismiss)
                VStack(spacing: 0) {
                    Text(title)
                        .font(.system(size: 19, weight: .bold))
                        .foregroundColor(MenuPalette.ink)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 22)
                        .padding(.top, 22)
                        .padding(.bottom, 10)
                    ScrollView {
                        content
                            .background(GeometryReader { inner in
                                Color.clear.preference(key: MenuContentHeight.self, value: inner.size.height)
                            })
                    }
                    .frame(height: min(contentHeight, max(geometry.size.height - 180, 120)))
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
                .background(RoundedRectangle(cornerRadius: 16).fill(MenuPalette.panel))
                .overlay(RoundedRectangle(cornerRadius: 16).stroke(MenuPalette.line, lineWidth: 1))
                .frame(maxWidth: 360)
                .padding(.horizontal, 20)
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
        }
        .onPreferenceChange(MenuContentHeight.self) { contentHeight = $0 }
        .accentColor(MenuPalette.accent)
        .environment(\.colorScheme, .dark)
    }
}

/// How tall a menu window's content is, so the window fits it.
private struct MenuContentHeight: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}

/// A row of the game menu: its icon on a tile, its label, and on the right
/// what it is set to and, for one that opens a window of its own, a chevron.
struct MenuRow: View {
    let symbol: String
    let label: String
    var note: String?
    var value: String?
    var valueOff = false
    var opens = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: symbol)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundColor(MenuPalette.accent)
                    .frame(width: 30, height: 30)
                    .background(RoundedRectangle(cornerRadius: 8).fill(MenuPalette.raised))
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(MenuPalette.raisedLine, lineWidth: 1))
                (Text(label).font(.system(size: 15)).foregroundColor(MenuPalette.ink)
                    + Text(note.map { "  " + $0 } ?? "").font(.system(size: 12)).foregroundColor(MenuPalette.muted))
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let value {
                    Text(value)
                        .font(.system(size: 15, weight: .bold))
                        .foregroundColor(valueOff ? MenuPalette.muted : MenuPalette.accent)
                }
                if opens {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundColor(MenuPalette.muted)
                }
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 8)
            .frame(minHeight: 46)
            .contentShape(Rectangle())
        }
        .buttonStyle(MenuRowStyle())
    }
}

/// A menu row lit while a finger is on it.
private struct MenuRowStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? MenuPalette.raised : Color.clear)
    }
}

/// One of the choices in a menu window that picks a setting: a small picture
/// of it, its name, and a round mark on the one chosen.
struct MenuOption<Swatch: View>: View {
    let label: String
    let on: Bool
    let swatch: Swatch
    let action: () -> Void

    init(label: String, on: Bool, @ViewBuilder swatch: () -> Swatch, action: @escaping () -> Void) {
        self.label = label
        self.on = on
        self.swatch = swatch()
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                swatch
                    .frame(width: 44, height: 30)
                    .clipShape(RoundedRectangle(cornerRadius: 7))
                Text(label)
                    .font(.system(size: 15, weight: .bold))
                    .foregroundColor(MenuPalette.ink)
                Spacer(minLength: 8)
                Circle()
                    .fill(on ? MenuPalette.accent : Color.clear)
                    .overlay(Circle().stroke(on ? MenuPalette.accent : MenuPalette.outline, lineWidth: 2))
                    .frame(width: 18, height: 18)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(minHeight: 56)
            .background(RoundedRectangle(cornerRadius: 12).fill(on ? MenuPalette.accent.opacity(0.1) : Color.clear))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(on ? MenuPalette.accent : MenuPalette.raisedLine, lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The window asking before the game is closed, in the look of the menu's
/// others. (A gamepad answers it too, through the game - see `GameView`.)
struct ExitConfirm: View {
    let onYes: () -> Void
    let onNo: () -> Void

    var body: some View {
        MenuDialog(title: "종료", onDismiss: onNo) {
            Text("애플리케이션을 종료하시겠습니까?")
                .font(.system(size: 15))
                .foregroundColor(MenuPalette.muted)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 22)
                .padding(.top, 2)
                .padding(.bottom, 16)
        } footer: {
            Spacer()
            Button("아니요", action: onNo)
                .buttonStyle(MenuButtonStyle(kind: .secondary))
            Button("예", action: onYes)
                .buttonStyle(MenuButtonStyle(kind: .primary))
        }
    }
}
