import SwiftUI

/// The library's own colours, light and dark, as on Android - the cards over
/// the list wear them rather than the system's.
struct LibraryPalette {
    let surface: Color
    let ink: Color
    let muted: Color
    let line: Color
    let green: Color
    let greenDeep: Color
    let greenSoft: Color
    let greenLine: Color
    let greenSofter: Color
    let red: Color
    let redSoft: Color
    let redLine: Color

    static func of(dark: Bool) -> LibraryPalette {
        dark
            ? LibraryPalette(
                surface: rgb(22, 29, 25), ink: rgb(228, 236, 230), muted: rgb(138, 156, 144), line: rgb(36, 48, 42),
                green: rgb(76, 194, 126), greenDeep: rgb(111, 212, 154), greenSoft: rgb(28, 52, 38),
                greenLine: rgb(43, 74, 55), greenSofter: rgb(24, 34, 28),
                red: rgb(229, 83, 75), redSoft: rgb(58, 29, 27), redLine: rgb(90, 42, 39))
            : LibraryPalette(
                surface: .white, ink: rgb(26, 42, 32), muted: rgb(100, 117, 104), line: rgb(233, 241, 235),
                green: rgb(46, 139, 87), greenDeep: rgb(34, 114, 71), greenSoft: rgb(220, 242, 226),
                greenLine: rgb(199, 232, 209), greenSofter: rgb(238, 248, 241),
                red: rgb(192, 57, 43), redSoft: rgb(253, 236, 235), redLine: rgb(245, 201, 196))
    }

    private static func rgb(_ r: Double, _ g: Double, _ b: Double) -> Color {
        Color(red: r / 255, green: g / 255, blue: b / 255)
    }
}

/// One choice on a tile menu: an icon, a title, a short note and what it does.
struct TileAction: Identifiable {
    let id = UUID()
    let icon: String
    let title: String
    let note: String
    let run: () -> Void
}

/// A card over the dimmed list, closed by a tap outside it.
struct LibraryCardOverlay<Card: View>: View {
    let onDismiss: () -> Void
    let card: Card

    init(onDismiss: @escaping () -> Void, @ViewBuilder card: () -> Card) {
        self.onDismiss = onDismiss
        self.card = card()
    }

    var body: some View {
        ZStack {
            Color.black.opacity(0.55)
                .ignoresSafeArea()
                .onTapGesture(perform: onDismiss)
            card
                .frame(maxWidth: 380)
                .padding(.horizontal, 18)
        }
    }
}

/// A game's actions: its cover and name with ✕ to close, the actions as tiles
/// two to a row, and the ones that lose something as red buttons under them.
/// There is no button bar, so the card is only as tall as what it offers.
struct TileMenuCard<Cover: View, Meta: View>: View {
    let title: String
    let tiles: [TileAction]
    let dangers: [TileAction]
    let palette: LibraryPalette
    let onClose: () -> Void
    let cover: Cover
    let meta: Meta

    init(
        title: String, tiles: [TileAction], dangers: [TileAction], palette: LibraryPalette, onClose: @escaping () -> Void,
        @ViewBuilder cover: () -> Cover, @ViewBuilder meta: () -> Meta
    ) {
        self.title = title
        self.tiles = tiles
        self.dangers = dangers
        self.palette = palette
        self.onClose = onClose
        self.cover = cover()
        self.meta = meta()
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 11) {
                cover
                    .frame(width: 40, height: 40)
                    .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                VStack(alignment: .leading, spacing: 3) {
                    Text(title)
                        .font(.system(size: 16, weight: .bold))
                        .foregroundColor(palette.ink)
                        .lineLimit(1)
                    meta
                }
                Spacer(minLength: 8)
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundColor(palette.muted)
                        .frame(width: 32, height: 32)
                        .background(RoundedRectangle(cornerRadius: 9).fill(palette.surface))
                        .overlay(RoundedRectangle(cornerRadius: 9).stroke(palette.line, lineWidth: 1))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("닫기")
            }

            LazyVGrid(columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)], spacing: 8) {
                ForEach(tiles) { tile($0) }
            }
            .padding(.top, 14)

            HStack(spacing: 8) {
                ForEach(dangers) { danger($0) }
            }
            .padding(.top, 10)
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 22, style: .continuous).fill(palette.surface))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(palette.line, lineWidth: 1))
    }

    private func tile(_ action: TileAction) -> some View {
        Button(action: action.run) {
            VStack(spacing: 0) {
                Image(systemName: action.icon)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundColor(palette.greenDeep)
                    .frame(width: 34, height: 34)
                    .background(RoundedRectangle(cornerRadius: 10).fill(palette.greenSoft))
                Text(action.title)
                    .font(.system(size: 13, weight: .bold))
                    .foregroundColor(palette.ink)
                    .lineLimit(1)
                    .padding(.top, 7)
                Text(action.note)
                    .font(.system(size: 11))
                    .foregroundColor(palette.muted)
                    .lineLimit(1)
            }
            .padding(.horizontal, 8)
            .padding(.top, 12)
            .padding(.bottom, 11)
            .frame(maxWidth: .infinity)
            .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(palette.greenSofter))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(palette.line, lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func danger(_ action: TileAction) -> some View {
        Button(action: action.run) {
            HStack(spacing: 6) {
                Image(systemName: action.icon)
                    .font(.system(size: 13, weight: .semibold))
                Text(action.title)
                    .font(.system(size: 13, weight: .bold))
                    .lineLimit(1)
            }
            .foregroundColor(palette.red)
            .frame(maxWidth: .infinity)
            .frame(height: 42)
            .background(RoundedRectangle(cornerRadius: 12).fill(palette.redSoft))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.redLine, lineWidth: 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Asks before an action that cannot be taken back: the question, what it
/// means, and 취소 beside the action - full height and side by side rather than
/// in a bar of their own. `danger` paints the action red, otherwise green.
struct ConfirmCard: View {
    let title: String
    let message: String
    let action: String
    let danger: Bool
    let palette: LibraryPalette
    let dark: Bool
    let onConfirm: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.system(size: 16.5, weight: .bold))
                .foregroundColor(palette.ink)
                .fixedSize(horizontal: false, vertical: true)
            Text(message)
                .font(.system(size: 13))
                .foregroundColor(palette.muted)
                .lineSpacing(2)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 7)
            HStack(spacing: 8) {
                Button(action: onCancel) {
                    Text("취소")
                        .font(.system(size: 14, weight: .bold))
                        .foregroundColor(palette.ink)
                        .frame(maxWidth: .infinity)
                        .frame(height: 44)
                        .background(RoundedRectangle(cornerRadius: 12).fill(palette.surface))
                        .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.line, lineWidth: 1))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Button(action: onConfirm) {
                    Text(action)
                        .font(.system(size: 14, weight: .bold))
                        // White on the dark list's lighter green would not read.
                        .foregroundColor(danger || !dark ? .white : Color(red: 15 / 255, green: 20 / 255, blue: 17 / 255))
                        .frame(maxWidth: .infinity)
                        .frame(height: 44)
                        .background(RoundedRectangle(cornerRadius: 12).fill(danger ? palette.red : palette.green))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .padding(.top, 18)
        }
        .padding(.horizontal, 18)
        .padding(.top, 20)
        .padding(.bottom, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 22, style: .continuous).fill(palette.surface))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(palette.line, lineWidth: 1))
    }
}

extension String {
    /// The word with the object particle it takes - 을 after a final
    /// consonant, 를 after a vowel - or both when it does not end in Hangul.
    var withObject: String {
        guard let last = unicodeScalars.last else { return self }
        guard (0xAC00...0xD7A3).contains(last.value) else { return self + "을(를)" }
        return self + ((last.value - 0xAC00) % 28 == 0 ? "를" : "을")
    }
}
