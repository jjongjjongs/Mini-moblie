import SwiftUI

@main
struct MiniMobileApp: App {
    /// Says which way the window may face: upright, but for a running game.
    @UIApplicationDelegateAdaptor(OrientationLock.self) private var orientationLock

    var body: some Scene {
        WindowGroup {
            LibraryView()
        }
    }
}
