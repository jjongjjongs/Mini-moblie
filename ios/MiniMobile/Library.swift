import Foundation

/// A game the player has imported, kept under Documents/Games.
struct GameFile: Identifiable, Hashable {
    let url: URL
    var id: String { url.path }
    var title: String { url.deletingPathExtension().lastPathComponent }
}

/// Where games and their saves live. Both are under Documents, so the Files
/// app reaches them too (UIFileSharingEnabled).
enum Library {
    static let extensions: Set<String> = ["zip", "jar", "jad"]

    static var documents: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    static var gamesDirectory: URL {
        let url = documents.appendingPathComponent("Games", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// What the emulator keeps per title: its files and databases.
    static var dataDirectory: URL {
        let url = documents.appendingPathComponent("Data", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

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

    static func delete(_ game: GameFile) {
        try? FileManager.default.removeItem(at: game.url)
    }
}
