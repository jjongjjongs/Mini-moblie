import Foundation
import UIKit

/// Tells whether a newer build has been published, and opens the place to get
/// it. iOS does not let an app install another, so - unlike Android, which
/// installs the apk from inside itself - this only points the player at the
/// release page (or AltStore/SideStore/TrollStore), where the new ipa is
/// downloaded and signed on the way in.
///
/// CI writes the build's run number to `version.txt` beside the ipa, and the
/// app's own `CFBundleVersion` is that same run number (the iOS build passes
/// `CURRENT_PROJECT_VERSION`). A larger number on the release is a newer build.
enum Updater {
    /// The release the CI build publishes to, under the project's public name.
    static let releasePage = URL(string: "https://github.com/jjongjjongs/Mini-moblie/releases/tag/mobile-dev")!
    private static let versionURL = URL(string: "https://github.com/jjongjjongs/Mini-moblie/releases/download/mobile-dev/version.txt")!

    /// How often an automatic check runs, so a launch is not a round-trip.
    static let checkEvery: TimeInterval = 24 * 60 * 60

    /// This build's number, or 0 when it cannot be read (an unnumbered local
    /// build), which never looks out of date.
    static var installedBuild: Int {
        (Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String)
            .flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) } ?? 0
    }

    /// Reads the latest published build number, `nil` on any failure. The
    /// result is delivered on the main queue.
    static func fetchLatest(_ done: @escaping (Int?) -> Void) {
        var request = URLRequest(url: versionURL)
        request.setValue("MiniMobile", forHTTPHeaderField: "User-Agent")
        request.timeoutInterval = 20
        request.cachePolicy = .reloadIgnoringLocalCacheData
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let latest = data
                .flatMap { String(data: $0, encoding: .utf8) }
                .flatMap { Int($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            DispatchQueue.main.async { done(latest) }
        }.resume()
    }

    /// Opens the release page in Safari, where the ipa is downloaded.
    static func openReleasePage() {
        UIApplication.shared.open(releasePage)
    }
}
