import CoreMotion
import SwiftUI
import UIKit

/// Which way a game's screen may face, as the game menu's 화면 방향 sets it:
/// following the phone, or held upright or on its side.
enum GameOrientation: String, CaseIterable, Identifiable {
    /// The phone decides, the way it decides for everything else: the
    /// player turns while the phone's rotation lock is off and stays put
    /// while it is on.
    case auto
    /// Held upright whichever way the phone is turned - for playing lying
    /// down, where the phone's idea of upright is not the player's.
    case portrait
    /// Held on its side, either side up: it turns over when the phone does,
    /// rotation lock or not, and never goes back upright.
    case landscape

    var id: String { rawValue }

    var label: String {
        switch self {
        case .auto: return "자동"
        case .portrait: return "세로 고정"
        case .landscape: return "가로 고정"
        }
    }

    var detail: String {
        switch self {
        case .auto: return "폰을 돌리는 대로 (화면 회전 잠금을 따름)"
        case .portrait: return "폰을 돌려도 세로로"
        case .landscape: return "가로로, 폰을 뒤집으면 따라 뒤집힘"
        }
    }
}

/// What the app tells the system it may turn to. The list and every screen
/// but a running game are upright; a game asks for more through
/// `OrientationControl`.
final class OrientationLock: NSObject, UIApplicationDelegate {
    static var mask: UIInterfaceOrientationMask = upright

    /// Upright, as the list is: either way up on an iPad, as it always was.
    static var upright: UIInterfaceOrientationMask {
        UIDevice.current.userInterfaceIdiom == .pad ? [.portrait, .portraitUpsideDown] : .portrait
    }

    func application(_ application: UIApplication, supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        Self.mask
    }
}

/// Turns the window for a running game, as the Android player does.
///
/// 자동 hands the window to the phone. 세로 고정 holds it upright. 가로 고정
/// holds it on its side and reads the accelerometer itself, so that a phone
/// turned over turns the game over with it - the system's own rotation stops
/// at the rotation lock, and a landscape game held the wrong way up is no
/// use to anyone. Each game starts at 자동, as each Android player starts
/// unpinned.
final class OrientationControl {
    static let shared = OrientationControl()

    private let motion = CMMotionManager()
    private(set) var choice = GameOrientation.auto
    /// Whether a game is running, so leaving twice - closed, then gone -
    /// does the once.
    private var inGame = false
    /// The side 가로 고정 is holding.
    private var side: UIInterfaceOrientationMask = .landscapeRight

    private init() {}

    /// A game has started: the phone decides.
    func enterGame() {
        if !inGame {
            inGame = true
            UIDevice.current.beginGeneratingDeviceOrientationNotifications()
        }
        choice = .auto
        apply()
    }

    /// Back to the list, which is upright.
    func leaveGame() {
        guard inGame else { return }
        inGame = false
        UIDevice.current.endGeneratingDeviceOrientationNotifications()
        stopFollowingSide()
        choice = .auto
        set(OrientationLock.upright)
    }

    func choose(_ choice: GameOrientation) {
        self.choice = choice
        apply()
    }

    private func apply() {
        switch choice {
        case .auto:
            stopFollowingSide()
            set(UIDevice.current.userInterfaceIdiom == .pad ? .all : [.portrait, .landscapeLeft, .landscapeRight])
        case .portrait:
            stopFollowingSide()
            set(.portrait)
        case .landscape:
            side = landscapeSideNow()
            set(side)
            followSide()
        }
    }

    /// The side to hold first: the one the window is already on, else the one
    /// the phone is tipped towards, else the usual one.
    private func landscapeSideNow() -> UIInterfaceOrientationMask {
        switch windowScene?.interfaceOrientation {
        case .landscapeLeft?: return .landscapeLeft
        case .landscapeRight?: return .landscapeRight
        default: break
        }
        if let gravity = motion.deviceMotion?.gravity, abs(gravity.x) > 0.5 {
            return gravity.x < 0 ? .landscapeRight : .landscapeLeft
        }
        switch UIDevice.current.orientation {
        case .landscapeRight: return .landscapeLeft
        default: return .landscapeRight
        }
    }

    /// Watches which way down is while held on its side, and turns the window
    /// over when the phone is turned over. The device's left edge down is the
    /// window's right side up, and the other way round; a phone lying flat, or
    /// only tipped a little, leaves it where it is.
    private func followSide() {
        guard motion.isDeviceMotionAvailable, !motion.isDeviceMotionActive else { return }
        motion.deviceMotionUpdateInterval = 0.2
        motion.startDeviceMotionUpdates(to: .main) { [weak self] data, _ in
            guard let self, self.choice == .landscape, let gravity = data?.gravity else { return }
            let wanted: UIInterfaceOrientationMask
            if gravity.x < -0.6 {
                wanted = .landscapeRight
            } else if gravity.x > 0.6 {
                wanted = .landscapeLeft
            } else {
                return
            }
            if wanted != self.side {
                self.side = wanted
                self.set(wanted)
            }
        }
    }

    private func stopFollowingSide() {
        if motion.isDeviceMotionActive {
            motion.stopDeviceMotionUpdates()
        }
    }

    private var windowScene: UIWindowScene? {
        UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first
    }

    /// Says what the window may face now, and turns it there if it is facing
    /// anything else.
    private func set(_ mask: UIInterfaceOrientationMask) {
        OrientationLock.mask = mask
        guard let scene = windowScene else { return }
        if #available(iOS 16.0, *) {
            // The game is presented over the list, and the controller on top
            // is the one asked; tell each of them.
            for window in scene.windows {
                var controller = window.rootViewController
                while let current = controller {
                    current.setNeedsUpdateOfSupportedInterfaceOrientations()
                    controller = current.presentedViewController
                }
            }
            scene.requestGeometryUpdate(.iOS(interfaceOrientations: mask))
        } else {
            let target: UIInterfaceOrientation?
            switch mask {
            case .portrait: target = .portrait
            case .landscapeLeft: target = .landscapeLeft
            case .landscapeRight: target = .landscapeRight
            default: target = nil
            }
            if let target {
                UIDevice.current.setValue(target.rawValue, forKey: "orientation")
            }
            UIViewController.attemptRotationToDeviceOrientation()
        }
    }
}
