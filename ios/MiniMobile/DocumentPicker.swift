import UIKit
import UniformTypeIdentifiers

/// The system's file picker, opened straight from UIKit rather than through
/// SwiftUI's `fileImporter`, whose 열기 could leave the picker up with nothing
/// happening. It hands back copies made for this app - a file in iCloud or
/// another app's storage is fetched first - so what is picked is read like
/// any file of the app's own, and the copies go once `picked` has run.
final class DocumentPicker: NSObject, UIDocumentPickerDelegate {
    /// The picker's delegate is weak; this keeps the open one alive.
    private static var open: DocumentPicker?

    private let picked: ([URL]) -> Void

    private init(picked: @escaping ([URL]) -> Void) {
        self.picked = picked
    }

    /// Shows the picker over whatever is on screen. `picked` is not called
    /// when the player cancels.
    static func present(types: [UTType], multiple: Bool, picked: @escaping ([URL]) -> Void) {
        // A tap in a menu arrives as the menu goes; present once it has.
        DispatchQueue.main.async {
            guard let top = topController() else { return }
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: true)
            picker.allowsMultipleSelection = multiple
            picker.shouldShowFileExtensions = true
            let delegate = DocumentPicker(picked: picked)
            picker.delegate = delegate
            open = delegate
            top.present(picker, animated: true)
        }
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        Self.open = nil
        picked(urls)
        for url in urls {
            try? FileManager.default.removeItem(at: url)
        }
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        Self.open = nil
    }

    /// The controller on top of the active window, to present from.
    private static func topController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let scene = scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
        let window = scene?.windows.first { $0.isKeyWindow } ?? scene?.windows.first
        var controller = window?.rootViewController
        while let presented = controller?.presentedViewController, !presented.isBeingDismissed {
            controller = presented
        }
        return controller
    }
}
