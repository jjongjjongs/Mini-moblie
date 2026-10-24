import SwiftUI
import UIKit

/// When each title was last played, which the library's 최근 플레이순 sorts
/// by. It is kept, not shown.
enum PlayHistory {
    private static let key = "playHistory"

    private static func all() -> [String: Double] {
        UserDefaults.standard.dictionary(forKey: key) as? [String: Double] ?? [:]
    }

    static func lastPlayed(_ game: GameFile) -> Date? {
        all()[game.name].map { Date(timeIntervalSince1970: $0) }
    }

    static func markPlayed(_ game: GameFile) {
        var played = all()
        played[game.name] = Date().timeIntervalSince1970
        UserDefaults.standard.set(played, forKey: key)
    }

    /// A deleted title takes its time with it.
    static func forget(_ game: GameFile) {
        var played = all()
        played[game.name] = nil
        UserDefaults.standard.set(played, forKey: key)
    }

    /// The screens the grid once kept as covers are no longer shown; the
    /// folder they were kept in goes.
    static func dropOldCovers() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        try? FileManager.default.removeItem(at: base.appendingPathComponent("Covers", isDirectory: true))
    }
}

/// Each game's icon, read out of its archive by the runner (`wie_icon`, the
/// same search the Android app makes) and kept: in memory, and on disk
/// against the file's size and date so the archive is read once.
enum GameIcons {
    private static let memory = NSCache<NSString, UIImage>()
    /// The largest icon `wie_icon` hands back: a jar's picture, 512 square.
    private static let capacity = 512 * 512 * 4

    private static var directory: URL {
        let base = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        let url = base.appendingPathComponent("Icons", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private static func stamp(_ game: GameFile) -> String {
        let attributes = try? FileManager.default.attributesOfItem(atPath: game.url.path)
        let size = (attributes?[.size] as? NSNumber)?.int64Value ?? 0
        let date = (attributes?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
        return "\(size):\(Int64(date))"
    }

    private static func file(_ game: GameFile) -> URL {
        directory.appendingPathComponent(game.name + ".png")
    }

    /// The icon already read, or `.some(nil)` for a game known to have none;
    /// nil when its archive has not been read yet.
    static func known(_ game: GameFile) -> UIImage?? {
        if let image = memory.object(forKey: game.name as NSString) {
            return .some(image)
        }
        guard let mark = UserDefaults.standard.string(forKey: "icon.\(game.name)"), mark.hasPrefix(stamp(game) + "|") else {
            return nil
        }
        guard mark.hasSuffix("|1"), let image = UIImage(contentsOfFile: file(game).path) else {
            return .some(nil)
        }
        memory.setObject(image, forKey: game.name as NSString)
        return .some(image)
    }

    /// Reads the icon out of the game's archive - slow for a large one, so
    /// off the main thread - and keeps it.
    static func read(_ game: GameFile) -> UIImage? {
        let current = stamp(game)
        guard let data = try? Data(contentsOf: game.url) else { return nil }
        var rgba = [UInt8](repeating: 0, count: capacity)
        var width: UInt32 = 0
        var height: UInt32 = 0
        let found = data.withUnsafeBytes { buffer -> Bool in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return rgba.withUnsafeMutableBufferPointer { out in
                wie_icon(bytes.baseAddress, bytes.count, out.baseAddress, out.count, &width, &height)
            }
        }
        var image: UIImage?
        if found, width > 0, height > 0,
           let provider = CGDataProvider(data: Data(rgba.prefix(Int(width * height * 4))) as CFData),
           let cgImage = CGImage(width: Int(width), height: Int(height), bitsPerComponent: 8, bitsPerPixel: 32,
                                 bytesPerRow: Int(width) * 4, space: CGColorSpaceCreateDeviceRGB(),
                                 bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                                 provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent) {
            image = UIImage(cgImage: cgImage)
        }
        if let image, let png = image.pngData() {
            try? png.write(to: file(game), options: .atomic)
            memory.setObject(image, forKey: game.name as NSString)
        }
        UserDefaults.standard.set("\(current)|\(image == nil ? 0 : 1)", forKey: "icon.\(game.name)")
        return image
    }

    /// A deleted game takes its icon with it.
    static func forget(_ game: GameFile) {
        memory.removeObject(forKey: game.name as NSString)
        UserDefaults.standard.removeObject(forKey: "icon.\(game.name)")
        try? FileManager.default.removeItem(at: file(game))
    }
}

/// The order the library lists its games in.
enum LibrarySort: String {
    case recent
    case name

    var label: String {
        switch self {
        case .recent: return "최근 플레이순"
        case .name: return "이름순"
        }
    }

    /// The games in this order: last played first then the never played,
    /// each by title; or all by title.
    func sorted(_ games: [GameFile]) -> [GameFile] {
        guard self == .recent else { return games }
        let played = Dictionary(uniqueKeysWithValues: games.map { ($0.name, PlayHistory.lastPlayed($0)?.timeIntervalSince1970 ?? 0) })
        return games.enumerated().sorted { a, b in
            let (first, second) = (played[a.element.name] ?? 0, played[b.element.name] ?? 0)
            return first != second ? first > second : a.offset < b.offset
        }.map(\.element)
    }
}

/// A title's cover in the grid, square: its icon, filling the square with
/// its pixels kept sharp; or, a game without one, its first letter on its
/// own colour.
struct GameCover: View {
    let game: GameFile
    let icon: UIImage?

    var body: some View {
        let tint = Color(hue: Self.hue(game.name), saturation: 0.45, brightness: 0.55)
        ZStack {
            tint
            if let image = icon {
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.none)
                    .aspectRatio(contentMode: .fill)
            } else {
                Text(String(game.title.prefix(1)))
                    .font(.system(size: 34, weight: .bold))
                    .foregroundColor(.white)
            }
        }
    }

    /// A steady hue per title, so a cover keeps its colour.
    private static func hue(_ name: String) -> Double {
        var hash: UInt32 = 2_166_136_261
        for byte in name.utf8 {
            hash = (hash ^ UInt32(byte)) &* 16_777_619
        }
        return Double(hash % 360) / 360
    }
}
