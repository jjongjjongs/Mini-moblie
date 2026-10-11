import Foundation

/// A game the player has imported, kept under Documents/Games.
struct GameFile: Identifiable, Hashable {
    let url: URL
    var id: String { url.path }
    /// The file name, which is what favorites and per-title settings key on.
    var name: String { url.lastPathComponent }
    var title: String { url.deletingPathExtension().lastPathComponent }
}

/// A failure the emulator or the file system reported, in the words the
/// player sees.
struct LibraryError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// Takes ownership of a string the emulator returned.
func takeString(_ pointer: UnsafeMutablePointer<CChar>?) -> String? {
    guard let pointer else { return nil }
    defer { wie_free_string(pointer) }
    return String(cString: pointer)
}

/// Where games and their saves live. Everything is under Documents, so the
/// Files app reaches it too (UIFileSharingEnabled).
enum Library {
    static let extensions: Set<String> = ["zip", "jar", "jad"]

    static var documents: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    private static func directory(_ name: String) -> URL {
        let url = documents.appendingPathComponent(name, isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    static var gamesDirectory: URL { directory("Games") }

    /// What the emulator keeps per title: its files and databases.
    static var dataDirectory: URL { directory("Data") }

    /// Exported saves, as zips the Android app imports as well.
    static var savesDirectory: URL { directory("Saves") }

    static func games() -> [GameFile] {
        let urls = (try? FileManager.default.contentsOfDirectory(at: gamesDirectory, includingPropertiesForKeys: nil)) ?? []
        return urls
            .filter { extensions.contains($0.pathExtension.lowercased()) }
            .map(GameFile.init)
            .sorted { $0.title.localizedStandardCompare($1.title) == .orderedAscending }
    }

    /// Copies a picked file in, replacing one of the same name.
    static func importGame(from source: URL) throws {
        let accessing = source.startAccessingSecurityScopedResource()
        defer {
            if accessing { source.stopAccessingSecurityScopedResource() }
        }
        let destination = gamesDirectory.appendingPathComponent(source.lastPathComponent)
        if FileManager.default.fileExists(atPath: destination.path) {
            try FileManager.default.removeItem(at: destination)
        }
        try FileManager.default.copyItem(at: source, to: destination)
    }

    /// Removes the game file; its saves stay, as they do on Android, so a
    /// title imported again picks up where it was.
    static func delete(_ game: GameFile) {
        try? FileManager.default.removeItem(at: game.url)
        Favorites.remove(game.name)
    }

    // MARK: - Carrier

    /// The carrier a game runs under - "KTF", "LGT", "SKT", "DRM" for a locked
    /// download, or "" - cached by name, size and date so the list does not
    /// read every file each time it is shown.
    static func carrier(of game: GameFile) -> String {
        let attributes = try? FileManager.default.attributesOfItem(atPath: game.url.path)
        let size = (attributes?[.size] as? NSNumber)?.int64Value ?? 0
        let date = (attributes?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
        let stamp = "\(size):\(Int64(date))"

        let key = "carrier.\(game.name)"
        if let cached = UserDefaults.standard.string(forKey: key), cached.hasPrefix(stamp + "|") {
            return String(cached.dropFirst(stamp.count + 1))
        }

        guard let data = try? Data(contentsOf: game.url) else { return "" }
        let carrier = data.withUnsafeBytes { buffer -> String in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return takeString(wie_carrier(bytes.baseAddress, bytes.count)) ?? ""
        }
        UserDefaults.standard.set("\(stamp)|\(carrier)", forKey: key)
        return carrier
    }

    // MARK: - Saves

    /// Writes the title's saves to Documents/Saves as a zip, laid out as the
    /// Android app's export is, and returns where; nil when the title has
    /// not saved anything yet. Each export is a file of its own, named for
    /// the game and the time with `note` after it, so the ones before it stay
    /// to go back to.
    static func exportSave(_ game: GameFile, note: String = "") throws -> URL? {
        let data = try Data(contentsOf: game.url)
        let stamp = DateFormatter.saveStamp.string(from: Date())
        let base = "\(game.title) 세이브 \(stamp)\(note)"
        var destination = savesDirectory.appendingPathComponent("\(base).zip")
        var copy = 2
        while FileManager.default.fileExists(atPath: destination.path) {
            destination = savesDirectory.appendingPathComponent("\(base) (\(copy)).zip")
            copy += 1
        }

        var exported = false
        let failure = data.withUnsafeBytes { buffer -> String? in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return takeString(wie_export_save(bytes.baseAddress, bytes.count, dataDirectory.path, destination.path, &exported))
        }
        if let failure {
            throw LibraryError(message: failure)
        }
        return exported ? destination : nil
    }

    /// The save zips in Documents/Saves: the game's own first, then other
    /// games', each newest first. Zips that hold no saves are left out.
    static func saves(for game: GameFile) -> [SaveZip] {
        let data = (try? Data(contentsOf: game.url)) ?? Data()
        let urls = (try? FileManager.default.contentsOfDirectory(
            at: savesDirectory,
            includingPropertiesForKeys: [.contentModificationDateKey]
        )) ?? []

        var zips: [SaveZip] = []
        for url in urls where url.pathExtension.lowercased() == "zip" {
            guard let zip = try? Data(contentsOf: url) else { continue }
            var files = 0
            var size: UInt64 = 0
            let belongs = zip.withUnsafeBytes { zipBuffer -> Int32 in
                data.withUnsafeBytes { dataBuffer -> Int32 in
                    let zipBytes = zipBuffer.bindMemory(to: UInt8.self)
                    let dataBytes = dataBuffer.bindMemory(to: UInt8.self)
                    return wie_save_zip_info(zipBytes.baseAddress, zipBytes.count, dataBytes.baseAddress, dataBytes.count, &files, &size)
                }
            }
            guard belongs >= 0 else { continue }
            let modified = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
            zips.append(SaveZip(url: url, modified: modified, files: files, bytes: Int64(size), ours: belongs == 1))
        }
        return zips.sorted { $0.ours != $1.ours ? $0.ours : $0.modified > $1.modified }
    }

    /// Puts a save zip from the list in place. Over the game's own saves,
    /// what is there now is exported first, so the import can be undone from
    /// the same list; that export is returned with the count.
    static func importSave(_ zip: SaveZip, into game: GameFile) throws -> (restored: Int, backup: URL?) {
        let backup = try zip.ours ? exportSave(game, note: SaveZip.beforeImport) : nil
        let restored = try importSave(from: zip.url)
        return (restored, backup)
    }

    static func deleteSave(_ zip: SaveZip) throws {
        try FileManager.default.removeItem(at: zip.url)
    }

    /// Restores a save zip - one this app or the Android app exported - and
    /// returns how many files it put back.
    static func importSave(from source: URL) throws -> Int {
        let accessing = source.startAccessingSecurityScopedResource()
        defer {
            if accessing { source.stopAccessingSecurityScopedResource() }
        }
        let zip = try Data(contentsOf: source)

        var restored = 0
        let failure = zip.withUnsafeBytes { buffer -> String? in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return takeString(wie_import_save(bytes.baseAddress, bytes.count, dataDirectory.path, &restored))
        }
        if let failure {
            throw LibraryError(message: failure)
        }
        return restored
    }

    /// Erases the title's saves and returns how many files went.
    static func eraseSave(_ game: GameFile) throws -> Int {
        let data = try Data(contentsOf: game.url)

        var removed = 0
        let failure = data.withUnsafeBytes { buffer -> String? in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return takeString(wie_erase_save(bytes.baseAddress, bytes.count, dataDirectory.path, &removed))
        }
        if let failure {
            throw LibraryError(message: failure)
        }
        return removed
    }
}

extension DateFormatter {
    /// 2026-10-08 15.30, for export file names - as the Android app and the
    /// PC name theirs.
    static let saveStamp: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH.mm"
        return formatter
    }()
}

/// The titles the player starred, by file name.
enum Favorites {
    static let key = "favorites"

    static func decode(_ text: String) -> Set<String> {
        Set(text.split(separator: "\n").map(String.init))
    }

    static func encode(_ names: Set<String>) -> String {
        names.sorted().joined(separator: "\n")
    }

    static func remove(_ name: String) {
        var names = decode(UserDefaults.standard.string(forKey: key) ?? "")
        names.remove(name)
        UserDefaults.standard.set(encode(names), forKey: key)
    }
}

/// How fast each title runs, kept per title by file name as on Android: a slow
/// title can stay sped up without every other title following it.
enum GameSpeed {
    static let chips: [Float] = [0.5, 1, 1.5, 2, 3, 4]
    /// The ruler's ends, in tenths: 0.1x to 4x.
    static let tenths: ClosedRange<Int> = 1...40

    static func get(_ game: GameFile) -> Float {
        let value = UserDefaults.standard.float(forKey: "speed.\(game.name)")
        return value.isNaN || value <= 0 ? 1 : value
    }

    static func set(_ value: Float, for game: GameFile) {
        UserDefaults.standard.set(value, forKey: "speed.\(game.name)")
    }

    /// 2x, 1.5x, 1.25x - as few digits as the value needs.
    static func format(_ value: Float) -> String {
        if value == value.rounded() {
            return "\(Int(value))x"
        }
        var text = String(format: "%.2f", value)
        if text.hasSuffix("0") {
            text.removeLast()
        }
        return text + "x"
    }
}

/// How a title's screen is enlarged.
enum ScreenQuality: Int, CaseIterable, Identifiable {
    /// Smoothed: the "기본" choice.
    case smooth = 0
    /// Pixel for pixel, sharp and blocky, as the screen always was drawn.
    case dot = 1
    /// Doubled through hq2x first, its stepped edges smoothed, then enlarged
    /// smoothly.
    case hq2x = 2

    var id: Int { rawValue }

    var label: String {
        switch self {
        case .smooth: return "기본"
        case .dot: return "도트"
        case .hq2x: return "HQ2X"
        }
    }

    var detail: String {
        switch self {
        case .smooth: return "부드럽게 확대해요. 픽셀 경계가 살짝 흐려져요."
        case .dot: return "픽셀을 그대로 키워요. 또렷하고 각진 옛날 폰 느낌."
        case .hq2x: return "계단진 테두리를 매끈하게 다듬어 그려요."
        }
    }
}

/// The quality a title's screen is shown at: its own choice, else the one made
/// for every title, else 도트 - or 기본 for a player who had turned on the
/// smoothing that came before it.
enum GameQuality {
    private static let prefix = "quality."
    private static let everyGame = "quality.*"

    static func get(_ game: GameFile) -> ScreenQuality {
        let defaults = UserDefaults.standard
        let stored = defaults.object(forKey: prefix + game.name) ?? defaults.object(forKey: everyGame)
        if let raw = stored as? Int, let quality = ScreenQuality(rawValue: raw) {
            return quality
        }
        return defaults.bool(forKey: SettingKey.smooth) ? .smooth : .dot
    }

    /// Keeps `quality` for `game`, or for every title - dropping each one's
    /// own choice, so they all follow it.
    static func set(_ quality: ScreenQuality, for game: GameFile, everyGame all: Bool) {
        let defaults = UserDefaults.standard
        if all {
            for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(prefix) {
                defaults.removeObject(forKey: key)
            }
            defaults.set(quality.rawValue, forKey: everyGame)
        } else {
            defaults.set(quality.rawValue, forKey: prefix + game.name)
        }
    }
}

/// Whether touches on the game screen reach a title, kept per title as the
/// speed is. Off unless the player turned it on for a title made for a touch
/// handset.
enum GameTouch {
    static func get(_ game: GameFile) -> Bool {
        UserDefaults.standard.bool(forKey: "touch.\(game.name)")
    }

    static func set(_ enabled: Bool, for game: GameFile) {
        UserDefaults.standard.set(enabled, forKey: "touch.\(game.name)")
    }
}

/// Whether a title is played with the pad put away, the whole screen left to
/// the game - kept per title, as the speed is.
enum GamePad {
    static func hidden(_ game: GameFile) -> Bool {
        UserDefaults.standard.bool(forKey: "padHidden.\(game.name)")
    }

    static func setHidden(_ hidden: Bool, for game: GameFile) {
        UserDefaults.standard.set(hidden, forKey: "padHidden.\(game.name)")
    }

    /// The title's pad shape: 숫자 크게 for a rhythm game, else the ordinary pad.
    static func shape(_ game: GameFile) -> PadShape {
        PadShape(rawValue: UserDefaults.standard.string(forKey: "padShape.\(game.name)") ?? "") ?? .standard
    }

    static func setShape(_ shape: PadShape, for game: GameFile) {
        UserDefaults.standard.set(shape.rawValue, forKey: "padShape.\(game.name)")
    }
}

/// Matching a search against a title: a plain substring, or - when the query is
/// only initial consonants, as Korean players type it - against the initial
/// consonant of each syllable ("ㅇㅇㅅㄱ" finds 영웅서기).
enum TitleSearch {
    private static let initials: [Character] = Array("ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ")

    static func matches(_ title: String, _ query: String) -> Bool {
        let query = query.replacingOccurrences(of: " ", with: "")
        if query.isEmpty {
            return true
        }
        if title.replacingOccurrences(of: " ", with: "").localizedCaseInsensitiveContains(query) {
            return true
        }
        guard query.allSatisfy({ initials.contains($0) }) else { return false }
        return initialsOf(title).contains(query)
    }

    private static func initialsOf(_ title: String) -> String {
        var result = ""
        for scalar in title.unicodeScalars {
            let value = scalar.value
            if (0xAC00...0xD7A3).contains(value) {
                result.append(initials[Int((value - 0xAC00) / 588)])
            } else if scalar != " " {
                result.unicodeScalars.append(scalar)
            }
        }
        return result
    }
}
