import SwiftUI

/// A title running: its screen on top, the handset's keys below.
struct GameView: View {
    let game: GameFile

    @StateObject private var emulator = Emulator()
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Button("닫기") {
                    emulator.stop()
                    dismiss()
                }
                Spacer()
                Text(game.title)
                    .font(.headline)
                    .lineLimit(1)
                Spacer()
            }
            .foregroundColor(.white)
            .padding(.horizontal, 4)

            ZStack {
                Color.black
                if let frame = emulator.frame {
                    Image(decorative: frame, scale: 1)
                        .interpolation(.none)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                }
                if let message = emulator.message {
                    Text(message)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .padding()
                        .background(Color.black.opacity(0.7))
                        .cornerRadius(8)
                        .padding()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            Keypad(emulator: emulator)
        }
        .padding(8)
        .background(Color(white: 0.08).ignoresSafeArea())
        .onAppear { emulator.start(game: game) }
        .onDisappear { emulator.stop() }
    }
}

/// The handset's keys, by the indexes in wie.h.
private struct Keypad: View {
    let emulator: Emulator

    private let rows: [[(String, Int32)]] = [
        [("◀︎소프트", Int32(WIE_KEY_LEFT_SOFT)), ("▲", Int32(WIE_KEY_UP)), ("소프트▶︎", Int32(WIE_KEY_RIGHT_SOFT))],
        [("◀︎", Int32(WIE_KEY_LEFT)), ("OK", Int32(WIE_KEY_OK)), ("▶︎", Int32(WIE_KEY_RIGHT))],
        [("취소", Int32(WIE_KEY_CLEAR)), ("▼", Int32(WIE_KEY_DOWN)), ("통화", Int32(WIE_KEY_CALL))],
        [("1", 9), ("2", 10), ("3", 11)],
        [("4", 12), ("5", 13), ("6", 14)],
        [("7", 15), ("8", 16), ("9", 17)],
        [("*", Int32(WIE_KEY_STAR)), ("0", Int32(WIE_KEY_NUM0)), ("#", Int32(WIE_KEY_HASH))],
    ]

    var body: some View {
        VStack(spacing: 6) {
            ForEach(0..<rows.count, id: \.self) { row in
                HStack(spacing: 6) {
                    ForEach(0..<rows[row].count, id: \.self) { column in
                        let (label, index) = rows[row][column]
                        KeyButton(label: label) { pressed in
                            emulator.key(index, pressed: pressed)
                        }
                    }
                }
            }
        }
    }
}

/// A key that reports both the press and the release, as the title sees them.
private struct KeyButton: View {
    let label: String
    let onChange: (Bool) -> Void

    @State private var pressed = false

    var body: some View {
        Text(label)
            .font(.system(size: 17, weight: .semibold))
            .foregroundColor(.white)
            .frame(maxWidth: .infinity, minHeight: 40)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(pressed ? Color.white.opacity(0.35) : Color.white.opacity(0.15))
            )
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !pressed {
                            pressed = true
                            onChange(true)
                        }
                    }
                    .onEnded { _ in
                        pressed = false
                        onChange(false)
                    }
            )
    }
}
