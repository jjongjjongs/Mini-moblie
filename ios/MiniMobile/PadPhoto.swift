import CoreImage
import PhotosUI
import SwiftUI
import UIKit

/// 키패드 디자인 → 내 사진: pictures of the player's own laid on the pad, one
/// of four ways, the same four the Android app offers. The keys keep their
/// places, sizes and shapes; the pictures only fill them or lie under them.
enum PadPhotoMode: Int, CaseIterable, Identifiable, Codable {
    /// The picture under the whole pad, the keys frosted glass over it.
    case backdrop
    /// A dark tray, the picture showing only through the keys - one picture
    /// across all of them.
    case stencil
    /// The picture blurred under the pad, the keys coloured from it.
    case tint
    /// 다크 글래스 as it is, the picture on the direction disc alone.
    case disc

    var id: Int { rawValue }

    var label: String {
        switch self {
        case .backdrop: return "배경에 깔고 유리 키"
        case .stencil: return "키 모양으로 찍어내기"
        case .tint: return "사진에서 색 뽑기"
        case .disc: return "방향 원판에만"
        }
    }
}

/// How one picture sits in its part of the pad.
struct PadPhotoSlot: Codable, Hashable {
    /// Quarter turns clockwise, 0 to 3.
    var turns = 0
    /// Which part of a picture bigger than its place shows, 0 to 1 each way:
    /// 0 its left or top edge, 1 its right or bottom.
    var focusX = 0.5
    var focusY = 0.5
    /// How far in it is drawn, 1 (just filling its place) to 3.
    var zoom = 1.0
}

/// Everything 내 사진 draws one way up with.
struct PadPhotoFace: Codable, Hashable {
    var mode = PadPhotoMode.backdrop
    /// How much the pictures are darkened, 0 to 0.8.
    var dim = 0.35
    /// How soft they are drawn, 0 to 20 points.
    var blur = 2.0
    /// How clear the glass keys of 배경 and the tinted keys of 색 뽑기 are, 0
    /// to 1. Fully clear, a glass key is its outline and label over the picture.
    var glass = 0.45
    /// A picture for each half rather than one across the pad.
    var split = false
    /// The one across the pad, then the left half's and the right's.
    var slots = [PadPhotoSlot(), PadPhotoSlot(), PadPhotoSlot()]

    static let whole = 0
    static let left = 1
    static let right = 2
}

/// The 내 사진 pictures and settings, for every title. Upright and on its
/// side the pad is a different shape - on its side two bands either side of
/// the screen - so each keeps a face of its own. A change shows on the pad at
/// once, the way a design does.
///
/// Pictures are copied into the app's own files when they are chosen -
/// upright, and no bigger than the pad can use - so they stay when the
/// originals are deleted from the photo library.
final class PadPhotoStore: ObservableObject {
    static let shared = PadPhotoStore()

    @Published var portrait: PadPhotoFace { didSet { save() } }
    @Published var landscape: PadPhotoFace { didSet { save() } }
    /// The pictures by face and slot ("p0", "l2").
    @Published private(set) var images: [String: UIImage] = [:]
    /// Whether a picture is being read in.
    @Published private(set) var loading = false

    private var layers: [String: UIImage] = [:]
    private var layerOrder: [String] = []
    private var tones: [ObjectIdentifier: (average: Color, vivid: Color)] = [:]
    private let context = CIContext(options: [.useSoftwareRenderer: false])

    private static let longest: CGFloat = 1280
    private static let defaults = UserDefaults.standard
    private static let facesKey = "pad.photo.faces"

    private init() {
        let stored = Self.defaults
        var upright = PadPhotoFace()
        var onItsSide = PadPhotoFace()
        if let data = stored.data(forKey: Self.facesKey),
           let faces = try? JSONDecoder().decode([PadPhotoFace].self, from: data), faces.count == 2 {
            upright = faces[0]
            onItsSide = faces[1]
        } else if stored.object(forKey: "pad.photo.mode") != nil {
            // The first 내 사진 kept one picture and one set of settings for
            // both ways up; both faces start from them.
            var first = PadPhotoFace()
            first.mode = PadPhotoMode(rawValue: stored.integer(forKey: "pad.photo.mode")) ?? .backdrop
            first.dim = stored.object(forKey: "pad.photo.dim") as? Double ?? first.dim
            first.blur = stored.object(forKey: "pad.photo.blur") as? Double ?? first.blur
            first.glass = stored.object(forKey: "pad.photo.glass") as? Double ?? first.glass
            upright = first
            onItsSide = first
            upright.slots[PadPhotoFace.whole].focusY = stored.object(forKey: "pad.photo.focus.portrait") as? Double ?? 0.5
            onItsSide.slots[PadPhotoFace.whole].focusY = stored.object(forKey: "pad.photo.focus.landscape") as? Double ?? 0.5
            if let old = Self.folder?.appendingPathComponent("keypad_photo.jpg"), let data = try? Data(contentsOf: old) {
                for name in ["p0", "l0"] {
                    if let url = Self.fileURL(name) {
                        try? data.write(to: url, options: .atomic)
                    }
                }
                try? FileManager.default.removeItem(at: old)
            }
        }
        portrait = upright
        landscape = onItsSide

        var found: [String: UIImage] = [:]
        for side in ["p", "l"] {
            for slot in 0..<3 {
                let name = "\(side)\(slot)"
                if let url = Self.fileURL(name), let data = try? Data(contentsOf: url), let image = UIImage(data: data) {
                    found[name] = image
                }
            }
        }
        images = found
        save()
    }

    private func save() {
        if let data = try? JSONEncoder().encode([portrait, landscape]) {
            Self.defaults.set(data, forKey: Self.facesKey)
        }
    }

    private static var folder: URL? {
        guard let folder = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return nil
        }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private static func fileURL(_ name: String) -> URL? {
        folder?.appendingPathComponent("keypad_photo_\(name).jpg")
    }

    private static func name(_ onItsSide: Bool, _ slot: Int) -> String {
        "\(onItsSide ? "l" : "p")\(slot)"
    }

    func face(_ onItsSide: Bool) -> PadPhotoFace {
        onItsSide ? landscape : portrait
    }

    /// Changes one face.
    func update(_ onItsSide: Bool, _ change: (inout PadPhotoFace) -> Void) {
        if onItsSide {
            change(&landscape)
        } else {
            change(&portrait)
        }
    }

    func image(_ onItsSide: Bool, _ slot: Int) -> UIImage? {
        images[Self.name(onItsSide, slot)]
    }

    /// Any one of the pictures, for a swatch.
    var thumbnail: UIImage? {
        ["p0", "p1", "p2", "l0", "l1", "l2"].lazy.compactMap { self.images[$0] }.first
    }

    /// Whether the face has a picture to draw: its one, or either half's.
    func hasPicture(_ onItsSide: Bool) -> Bool {
        if face(onItsSide).split {
            return image(onItsSide, PadPhotoFace.left) != nil || image(onItsSide, PadPhotoFace.right) != nil
        }
        return image(onItsSide, PadPhotoFace.whole) != nil
    }

    /// Takes a newly chosen picture for a face's slot: drawn upright at no
    /// more than 1280 points on its long side, and written into the app's files.
    func choose(_ picked: UIImage, onItsSide: Bool, slot: Int) {
        loading = true
        let name = Self.name(onItsSide, slot)
        DispatchQueue.global(qos: .userInitiated).async {
            let size = picked.size
            let scale = min(1, Self.longest / max(size.width, size.height, 1))
            let target = CGSize(width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1
            format.opaque = true
            // Drawing honours the picture's own orientation, so what comes
            // out is upright whichever way the camera held it.
            let upright = UIGraphicsImageRenderer(size: target, format: format).image { _ in
                picked.draw(in: CGRect(origin: .zero, size: target))
            }
            if let url = Self.fileURL(name), let data = upright.jpegData(compressionQuality: 0.9) {
                try? data.write(to: url, options: .atomic)
            }
            DispatchQueue.main.async {
                self.forget()
                self.images[name] = upright
                self.update(onItsSide) { $0.slots[slot] = PadPhotoSlot() }
                self.loading = false
            }
        }
    }

    /// Takes a face's picture out, and its file with it.
    func remove(onItsSide: Bool, slot: Int) {
        let name = Self.name(onItsSide, slot)
        if let url = Self.fileURL(name) {
            try? FileManager.default.removeItem(at: url)
        }
        forget()
        images[name] = nil
    }

    private func forget() {
        layers.removeAll()
        layerOrder.removeAll()
        tones.removeAll()
    }

    /// Where the pictures go on a pad of `size` laid out as `layout`. On its
    /// side, the bands either side of the screen - up to the keys' clusters,
    /// so no picture is hidden under the screen. Upright, the whole pad for
    /// one picture, or its halves - the directions' and the numbers' - for a
    /// picture each.
    static func regions(size: CGSize, layout: PadLayout, onItsSide: Bool, split: Bool) -> [CGRect] {
        let width = size.width
        let height = size.height
        if !onItsSide {
            if !split {
                return [CGRect(origin: .zero, size: size)]
            }
            return [CGRect(x: 0, y: 0, width: width / 2, height: height),
                    CGRect(x: width / 2, y: 0, width: width - width / 2, height: height)]
        }
        let leftSide: Set<Int32> = [Int32(WIE_KEY_LEFT_SOFT), Int32(WIE_KEY_RIGHT_SOFT), Int32(WIE_KEY_UP), Int32(WIE_KEY_DOWN),
                                    Int32(WIE_KEY_LEFT), Int32(WIE_KEY_RIGHT), Int32(WIE_KEY_OK)]
        var leftEdge: CGFloat = 0
        var rightEdge: CGFloat = width
        for key in layout.keys where !key.hidden {
            let rect = key.rect(in: size)
            if leftSide.contains(key.index) {
                leftEdge = max(leftEdge, rect.maxX + 4)
            } else {
                rightEdge = min(rightEdge, rect.minX - 4)
            }
        }
        if leftEdge >= rightEdge || leftEdge <= 0 || rightEdge >= width {
            leftEdge = width / 2
            rightEdge = width / 2
        }
        return [CGRect(x: 0, y: 0, width: leftEdge, height: height),
                CGRect(x: rightEdge, y: 0, width: width - rightEdge, height: height)]
    }

    /// A face's pictures laid into `regions` of a pad of `size`, softened by
    /// `blur` points and darkened by `dim`; clear outside them. One picture
    /// in two regions is stretched across both as though the gap were not
    /// there. Kept for the next frame, which nearly always asks for the same.
    func composite(_ onItsSide: Bool, size: CGSize, regions: [CGRect], blur: Double, dim: Double) -> UIImage? {
        guard size.width >= 1, size.height >= 1, hasPicture(onItsSide) else { return nil }
        let face = self.face(onItsSide)
        let scale = UIScreen.main.scale
        var key = "\(onItsSide)/\(face.hashValue)/\(Int(size.width))x\(Int(size.height))/\(Int((blur * 10).rounded()))/\(Int((dim * 100).rounded()))"
        for slot in 0..<3 {
            if let image = image(onItsSide, slot) {
                key += "/\(slot):\(ObjectIdentifier(image).hashValue)"
            }
        }
        for region in regions {
            key += "/\(Int(region.minX)),\(Int(region.maxX))"
        }
        if let cached = layers[key] {
            return cached
        }

        let full = CGRect(x: 0, y: 0, width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
        var pieces: [(CIImage, CGRect)] = []
        let pixelRegions = regions.map { Self.pixels($0, height: size.height, scale: scale) }
        if !face.split {
            if let source = image(onItsSide, PadPhotoFace.whole).flatMap(Self.source) {
                let slot = face.slots[PadPhotoFace.whole]
                if pixelRegions.count == 1 {
                    pieces.append((Self.place(source, slot: slot, into: pixelRegions[0]), pixelRegions[0]))
                } else if pixelRegions.count == 2 {
                    let first = pixelRegions[0]
                    let second = pixelRegions[1]
                    let across = CGRect(x: 0, y: 0, width: first.width + second.width, height: full.height)
                    let placed = Self.place(source, slot: slot, into: across)
                    let leftPart = placed.cropped(to: CGRect(x: 0, y: 0, width: first.width, height: full.height))
                        .transformed(by: CGAffineTransform(translationX: first.minX, y: first.minY))
                    let rightPart = placed.cropped(to: CGRect(x: first.width, y: 0, width: second.width, height: full.height))
                        .transformed(by: CGAffineTransform(translationX: second.minX - first.width, y: second.minY))
                    pieces.append((leftPart, first))
                    pieces.append((rightPart, second))
                }
            }
        } else {
            for half in 0..<min(2, pixelRegions.count) {
                let slotIndex = PadPhotoFace.left + half
                if let source = image(onItsSide, slotIndex).flatMap(Self.source) {
                    pieces.append((Self.place(source, slot: face.slots[slotIndex], into: pixelRegions[half]), pixelRegions[half]))
                }
            }
        }
        guard !pieces.isEmpty else { return nil }

        var out = CIImage.empty()
        for (piece, region) in pieces {
            var drawn = piece.cropped(to: region)
            if blur >= 0.5 {
                drawn = drawn.clampedToExtent().applyingGaussianBlur(sigma: blur * Double(scale) / 2).cropped(to: region)
            }
            if dim > 0 {
                let shade = CIImage(color: CIColor(red: 0, green: 0, blue: 0, alpha: CGFloat(min(dim, 1)))).cropped(to: region)
                drawn = shade.composited(over: drawn)
            }
            out = drawn.composited(over: out)
        }
        guard let rendered = context.createCGImage(out, from: full) else { return nil }
        let made = UIImage(cgImage: rendered, scale: scale, orientation: .up)
        remember(key, made)
        return made
    }

    /// The direction disc's own picture, `diameter` across: the left half's,
    /// which it sits in, or the one across the pad.
    func disc(_ onItsSide: Bool, diameter: CGFloat, blur: Double, dim: Double) -> UIImage? {
        let face = self.face(onItsSide)
        var slotIndex = PadPhotoFace.whole
        if face.split {
            slotIndex = image(onItsSide, PadPhotoFace.left) != nil ? PadPhotoFace.left : PadPhotoFace.right
        }
        guard diameter >= 1, let picture = image(onItsSide, slotIndex), let source = Self.source(picture) else { return nil }
        let scale = UIScreen.main.scale
        let key = "disc/\(onItsSide)/\(face.hashValue)/\(ObjectIdentifier(picture).hashValue)/\(Int(diameter))/\(Int((blur * 10).rounded()))/\(Int((dim * 100).rounded()))"
        if let cached = layers[key] {
            return cached
        }
        let side = (diameter * scale).rounded()
        let square = CGRect(x: 0, y: 0, width: side, height: side)
        var drawn = Self.place(source, slot: face.slots[slotIndex], into: square).cropped(to: square)
        if blur >= 0.5 {
            drawn = drawn.clampedToExtent().applyingGaussianBlur(sigma: blur * Double(scale) / 2).cropped(to: square)
        }
        if dim > 0 {
            drawn = CIImage(color: CIColor(red: 0, green: 0, blue: 0, alpha: CGFloat(min(dim, 1)))).cropped(to: square).composited(over: drawn)
        }
        guard let rendered = context.createCGImage(drawn, from: square) else { return nil }
        let made = UIImage(cgImage: rendered, scale: scale, orientation: .up)
        remember(key, made)
        return made
    }

    private func remember(_ key: String, _ image: UIImage) {
        layers[key] = image
        layerOrder.append(key)
        if layerOrder.count > 10 {
            layers.removeValue(forKey: layerOrder.removeFirst())
        }
    }

    private static func source(_ image: UIImage) -> CIImage? {
        image.cgImage.map { CIImage(cgImage: $0) }
    }

    /// A rectangle in points, top-left origin, as Core Image's pixels,
    /// counted up from the bottom.
    private static func pixels(_ rect: CGRect, height: CGFloat, scale: CGFloat) -> CGRect {
        CGRect(x: (rect.minX * scale).rounded(), y: ((height - rect.maxY) * scale).rounded(),
               width: (rect.width * scale).rounded(), height: (rect.height * scale).rounded())
    }

    /// `source` turned, scaled to cover `place` and zoomed further, and
    /// placed in it by the slot's focus.
    private static func place(_ source: CIImage, slot: PadPhotoSlot, into place: CGRect) -> CIImage {
        let orientations: [CGImagePropertyOrientation] = [.up, .right, .down, .left]
        var turned = source.oriented(orientations[((slot.turns % 4) + 4) % 4])
        turned = turned.transformed(by: CGAffineTransform(translationX: -turned.extent.minX, y: -turned.extent.minY))
        let width = turned.extent.width
        let height = turned.extent.height
        guard width > 0, height > 0 else { return turned }
        let cover = max(place.width / width, place.height / height)
        let fit = cover * CGFloat(max(1, slot.zoom))
        let drawnWidth = width * fit
        let drawnHeight = height * fit
        let x = place.minX + (place.width - drawnWidth) * CGFloat(slot.focusX)
        // Focus 0 shows the picture's top: its top on the place's, which in
        // Core Image is the place's higher edge.
        let y = place.minY + (place.height - drawnHeight) * CGFloat(1 - slot.focusY)
        return turned
            .transformed(by: CGAffineTransform(scaleX: fit, y: fit))
            .transformed(by: CGAffineTransform(translationX: x, y: y))
    }

    /// Two colours from `picture` - the face's pictures as laid on: their
    /// average, and their most vivid colour that is not dark - what 색 뽑기
    /// paints the keys and their accents with.
    func palette(of picture: UIImage?) -> (average: Color, vivid: Color) {
        let fallback = (average: Color(.sRGB, red: 40 / 255, green: 44 / 255, blue: 56 / 255, opacity: 1),
                        vivid: Color(.sRGB, red: 84 / 255, green: 199 / 255, blue: 214 / 255, opacity: 1))
        guard let picture, let source = picture.cgImage else { return fallback }
        if let known = tones[ObjectIdentifier(picture)] {
            return known
        }
        let width = 32
        let height = 20
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn: Bool = pixels.withUnsafeMutableBytes { buffer in
            guard let bitmap = CGContext(data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                                         bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            bitmap.interpolationQuality = .medium
            bitmap.draw(source, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return fallback }

        var red = 0.0
        var green = 0.0
        var blue = 0.0
        var count = 0.0
        var best = -1.0
        var vivid = (84.0, 199.0, 214.0)
        for index in stride(from: 0, to: pixels.count, by: 4) {
            // The clear parts are no part of any picture.
            if pixels[index + 3] < 128 {
                continue
            }
            let r = Double(pixels[index])
            let g = Double(pixels[index + 1])
            let b = Double(pixels[index + 2])
            red += r
            green += g
            blue += b
            count += 1
            let high = max(r, g, b)
            let low = min(r, g, b)
            let saturation = high > 0 ? (high - low) / high : 0
            let value = high / 255
            let score = saturation * value * (value > 0.35 ? 1 : 0.2)
            if score > best {
                best = score
                vivid = (r, g, b)
            }
        }
        guard count > 0 else { return fallback }
        let average = Color(.sRGB, red: red / count / 255, green: green / count / 255, blue: blue / count / 255, opacity: 1)
        let found = (average: average, vivid: Color(.sRGB, red: vivid.0 / 255, green: vivid.1 / 255, blue: vivid.2 / 255, opacity: 1))
        tones[ObjectIdentifier(picture)] = found
        return found
    }
}

/// What the pad draws with in 내 사진: the face for the way it is laid out,
/// and the palette its keys take for the way the pictures are laid on. Nil
/// - the pad keeps 다크 글래스 - until that face has a picture.
struct PadPhotoLook {
    let store: PadPhotoStore
    let face: PadPhotoFace
    /// Whether the pad is laid out on its side, which has its own face.
    let onItsSide: Bool

    init?(theme: PadTheme, store: PadPhotoStore, landscape: Bool) {
        guard theme == .photo, store.hasPicture(landscape) else { return nil }
        self.store = store
        self.face = store.face(landscape)
        self.onItsSide = landscape
    }

    var mode: PadPhotoMode { face.mode }

    private func regions(_ size: CGSize, _ layout: PadLayout) -> [CGRect] {
        PadPhotoStore.regions(size: size, layout: layout, onItsSide: onItsSide, split: face.split)
    }

    /// The tray: the pictures, a dark ground or 다크 글래스's, by the way they are laid on.
    func tray(size: CGSize, layout: PadLayout) -> UIImage? {
        switch mode {
        case .backdrop:
            return store.composite(onItsSide, size: size, regions: regions(size, layout), blur: face.blur, dim: face.dim)
        case .tint:
            return store.composite(onItsSide, size: size, regions: regions(size, layout), blur: face.blur + 10, dim: min(0.85, face.dim + 0.25))
        case .stencil, .disc:
            return nil
        }
    }

    /// What shows through the keys and the disc, for the two ways that cut
    /// the pictures to them.
    func cut(size: CGSize, layout: PadLayout) -> UIImage? {
        switch mode {
        case .backdrop:
            // Frosted, less so the clearer the keys are set: fully clear they
            // show the picture just as the tray does.
            return store.composite(onItsSide, size: size, regions: regions(size, layout), blur: face.blur + 8 * (1 - face.glass), dim: face.dim)
        case .stencil:
            return store.composite(onItsSide, size: size, regions: regions(size, layout), blur: face.blur, dim: face.dim * 0.8)
        case .tint, .disc:
            return nil
        }
    }

    /// The disc's own picture, for 방향 원판에만.
    func disc(diameter: CGFloat) -> UIImage? {
        mode == .disc ? store.disc(onItsSide, diameter: diameter, blur: face.blur, dim: face.dim) : nil
    }

    private static func rgb(_ red: Double, _ green: Double, _ blue: Double, _ alpha: Double = 1) -> Color {
        Color(.sRGB, red: red / 255, green: green / 255, blue: blue / 255, opacity: alpha)
    }

    private static let accent = rgb(84, 199, 214)
    private static let accentInk = rgb(12, 42, 47)
    private static let backInk = rgb(255, 138, 128)

    /// The palette for the way the pictures are laid on. 색 뽑기's colours
    /// are taken from the pictures as they lie on a pad laid out as `layout`,
    /// at a size of its kind - the colours do not change with the size.
    func palette(layout: PadLayout) -> PadPalette {
        switch mode {
        case .backdrop: return backdropPalette
        case .stencil: return stencilPalette
        case .tint:
            let size = onItsSide ? CGSize(width: 367, height: 176) : CGSize(width: 360, height: 230)
            return tintPalette(store.palette(of: tray(size: size, layout: layout)))
        case .disc: return discPalette
        }
    }

    private var backdropPalette: PadPalette {
        let white = Color.white
        let accent = Self.accent
        let face = white.opacity(0.30 * (1 - self.face.glass))
        let ok = white.opacity(0.38 * (1 - self.face.glass))
        let rim = white.opacity(0.32)
        return PadPalette(
            trayTop: Self.rgb(17, 18, 22), trayBottom: Self.rgb(17, 18, 22),
            faceTop: face, faceBottom: face, edge: rim,
            ink: white, subInk: white.opacity(0.8),
            pressedTop: accent.opacity(0.5), pressedBottom: accent.opacity(0.5), pressedInk: white,
            shadow: nil, radius: 16, pillNumbers: false,
            hollowFunctions: false, functionEdge: rim, functionInk: white,
            pillFunctions: true, functionRadius: 16,
            saveFill: accent, saveEdge: accent, saveInk: Self.accentInk,
            backEdge: rim, backInk: Self.backInk,
            ringTop: face, ringBottom: face, ringEdge: rim,
            ringSeam: white.opacity(0.22), arrow: white, arrowPressed: accent,
            wedgePressed: accent.opacity(0.45),
            okTop: ok, okBottom: ok, okEdge: rim, okInk: white,
            labelShadow: true
        )
    }

    private var stencilPalette: PadPalette {
        let white = Color.white
        let accent = Self.accent
        let rim = white.opacity(0.18)
        let ok = Self.rgb(12, 13, 16, 0.55)
        return PadPalette(
            trayTop: Self.rgb(12, 13, 16), trayBottom: Self.rgb(12, 13, 16),
            faceTop: .clear, faceBottom: .clear, edge: rim,
            ink: white, subInk: white.opacity(0.8),
            pressedTop: accent.opacity(0.55), pressedBottom: accent.opacity(0.55), pressedInk: white,
            shadow: nil, radius: 16, pillNumbers: false,
            hollowFunctions: false, functionEdge: rim, functionInk: white,
            pillFunctions: true, functionRadius: 16,
            saveFill: accent, saveEdge: accent, saveInk: Self.accentInk,
            backEdge: rim, backInk: Self.backInk,
            ringTop: .clear, ringBottom: .clear, ringEdge: rim,
            ringSeam: Color.black.opacity(0.55), arrow: white, arrowPressed: accent,
            wedgePressed: accent.opacity(0.5),
            okTop: ok, okBottom: ok, okEdge: white.opacity(0.25), okInk: white,
            labelShadow: true
        )
    }

    private func tintPalette(_ tones: (average: Color, vivid: Color)) -> PadPalette {
        let white = Color.white
        let vivid = tones.vivid
        let tinted = Self.luminance(vivid) < 0.3 ? Self.mix(vivid, .white, 0.45) : vivid
        let dark = Self.mix(Self.rgb(18, 19, 24), tones.average, 0.3)
        let base = Self.mix(dark, vivid, 0.18)
        let through = 1 - face.glass * 0.85
        let keyFace = base.opacity(through)
        let ring = Self.mix(base, .black, 0.1).opacity(through)
        let ok = Self.mix(base, .white, 0.06).opacity(through)
        let rim = tinted.opacity(0.45)
        let ink = Self.rgb(244, 245, 248)
        let saveInk = Self.luminance(tinted) > 0.55 ? Self.rgb(19, 22, 28) : white
        return PadPalette(
            trayTop: Self.rgb(17, 18, 22), trayBottom: Self.rgb(17, 18, 22),
            faceTop: keyFace, faceBottom: keyFace, edge: rim,
            ink: ink, subInk: Self.mix(tinted, .white, 0.35),
            pressedTop: tinted.opacity(0.55), pressedBottom: tinted.opacity(0.55), pressedInk: white,
            shadow: nil, radius: 16, pillNumbers: false,
            hollowFunctions: false, functionEdge: rim, functionInk: ink,
            pillFunctions: true, functionRadius: 16,
            saveFill: tinted, saveEdge: tinted, saveInk: saveInk,
            backEdge: rim, backInk: Self.backInk,
            ringTop: ring, ringBottom: ring, ringEdge: rim,
            ringSeam: tinted.opacity(0.25), arrow: tinted, arrowPressed: white,
            wedgePressed: tinted.opacity(0.45),
            okTop: ok, okBottom: ok, okEdge: rim, okInk: ink
        )
    }

    private var discPalette: PadPalette {
        let white = Color.white
        var palette = PadPalette.glass
        palette.ringTop = .clear
        palette.ringBottom = .clear
        palette.ringEdge = white.opacity(0.18)
        palette.ringSeam = white.opacity(0.25)
        palette.arrow = white
        palette.okTop = Self.rgb(34, 37, 45, 0.82)
        palette.okBottom = Self.rgb(34, 37, 45, 0.82)
        palette.okEdge = white.opacity(0.2)
        palette.ringShadow = true
        return palette
    }

    private static func components(_ color: Color) -> (Double, Double, Double) {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        UIColor(color).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (Double(red), Double(green), Double(blue))
    }

    /// `from` moved `amount` (0 to 1) of the way to `to`.
    private static func mix(_ from: Color, _ to: Color, _ amount: Double) -> Color {
        let a = components(from)
        let b = components(to)
        let red = a.0 + (b.0 - a.0) * amount
        let green = a.1 + (b.1 - a.1) * amount
        let blue = a.2 + (b.2 - a.2) * amount
        return Color(.sRGB, red: red, green: green, blue: blue, opacity: 1)
    }

    /// How light a colour reads, 0 to 1.
    private static func luminance(_ color: Color) -> Double {
        let c = components(color)
        return 0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2
    }
}

/// The tray under the pad: the design's, or 내 사진's pictures laid over it.
struct PadTray: View {
    let palette: PadPalette
    let photo: PadPhotoLook?
    let layout: PadLayout
    var cornerRadius: CGFloat = 14
    /// Whether the design's own tray is drawn under the pictures; on its
    /// side the pad has none, and only the pictures' bands are drawn.
    var ground = true

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if ground {
                    palette.tray
                }
                if let picture = photo?.tray(size: geometry.size, layout: layout) {
                    Image(uiImage: picture)
                        .resizable()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }
}

/// The pictures where 내 사진 cuts them to the pad: through every key and the
/// disc (배경, 찍어내기), or on the disc alone (원판에만). Drawn under the
/// keys' own faces, which lay their glass or outline over them.
struct PadPhotoCut: View {
    let photo: PadPhotoLook
    let layout: PadLayout
    let size: CGSize
    let ring: PadRing?
    /// Each key's rectangle and corner radius, as the keys are drawn.
    let shapes: [(CGRect, CGFloat)]

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let picture = photo.cut(size: size, layout: layout) {
                Image(uiImage: picture)
                    .resizable()
                    .frame(width: size.width, height: size.height)
                    .mask(keyShapes)
            }
            if let ring, let picture = photo.disc(diameter: ring.radius * 2) {
                Image(uiImage: picture)
                    .resizable()
                    .frame(width: ring.radius * 2, height: ring.radius * 2)
                    .clipShape(Circle())
                    .position(x: ring.center.x, y: ring.center.y)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
    }

    /// Every key's shape and the disc, where the picture shows through.
    private var keyShapes: some View {
        ZStack(alignment: .topLeading) {
            ForEach(shapes.indices, id: \.self) { index in
                RoundedRectangle(cornerRadius: shapes[index].1, style: .continuous)
                    .frame(width: shapes[index].0.width, height: shapes[index].0.height)
                    .position(x: shapes[index].0.midX, y: shapes[index].0.midY)
            }
            if let ring {
                Circle()
                    .frame(width: ring.radius * 2, height: ring.radius * 2)
                    .position(x: ring.center.x, y: ring.center.y)
            }
        }
        .frame(width: size.width, height: size.height, alignment: .topLeading)
    }
}

/// The photo library's picker, for one picture.
struct PadPhotoPicker: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    let onPick: (UIImage) -> Void

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.selectionLimit = 1
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: PHPickerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        private var parent: PadPhotoPicker

        init(_ parent: PadPhotoPicker) {
            self.parent = parent
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            parent.isPresented = false
            guard let provider = results.first?.itemProvider, provider.canLoadObject(ofClass: UIImage.self) else { return }
            let onPick = parent.onPick
            provider.loadObject(ofClass: UIImage.self) { object, _ in
                guard let image = object as? UIImage else { return }
                DispatchQueue.main.async { onPick(image) }
            }
        }
    }
}

/// A picture of the pad as 내 사진 draws it one way up: upright under its
/// screen, or on its side with a stand-in for the screen between the bands -
/// so the face for the other way up can be seen without turning the phone.
struct PadPhotoPreview: View {
    @ObservedObject var store: PadPhotoStore
    let onItsSide: Bool

    var body: some View {
        GeometryReader { geometry in
            content(geometry.size)
        }
        .frame(height: onItsSide ? 150 : 180)
    }

    private func content(_ size: CGSize) -> some View {
        let layout = PadLayout.standard(onItsSide ? .landscape : .below)
        let look = PadPhotoLook(theme: .photo, store: store, landscape: onItsSide)
        let palette = look?.palette(layout: layout) ?? PadPalette.glass
        let screenWidth = size.height * 240 / 320
        return ZStack {
            PadTray(palette: palette, photo: look, layout: layout, cornerRadius: 10)
            if onItsSide {
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color.black)
                    .overlay(Text("게임 화면").font(.caption2).foregroundColor(.gray))
                    .frame(width: screenWidth, height: size.height)
            }
            PadFaces(layout: layout, size: size, pressed: [], labelScale: 0.8, palette: palette, letters: !onItsSide, photo: look)
        }
    }
}

/// 내 사진's settings: for upright and on its side apart, the pictures - one
/// across the pad or one for each half, each turned, placed and zoomed on
/// its own - and the way they are laid on, over a picture of the pad that
/// follows every change.
struct PadPhotoSettingsView: View {
    @ObservedObject private var store = PadPhotoStore.shared
    @AppStorage(SettingKey.padTheme) private var padTheme = PadTheme.gold
    @State private var onItsSide: Bool
    @State private var slot: Int
    @State private var picking = false

    /// `landscape`: whether the pad is on its side now, the face shown first.
    init(landscape: Bool) {
        _onItsSide = State(initialValue: landscape)
        _slot = State(initialValue: PadPhotoStore.shared.face(landscape).split ? PadPhotoFace.left : PadPhotoFace.whole)
    }

    private var face: PadPhotoFace { store.face(onItsSide) }

    /// A binding to part of the face being edited.
    private func bind<Value>(_ path: WritableKeyPath<PadPhotoFace, Value>) -> Binding<Value> {
        let side = onItsSide
        return Binding(get: { store.face(side)[keyPath: path] }, set: { value in store.update(side) { $0[keyPath: path] = value } })
    }

    private var sideBinding: Binding<Bool> {
        Binding(get: { onItsSide }, set: { value in
            onItsSide = value
            slot = store.face(value).split ? PadPhotoFace.left : PadPhotoFace.whole
        })
    }

    private var splitBinding: Binding<Bool> {
        let side = onItsSide
        return Binding(get: { store.face(side).split }, set: { value in
            store.update(side) { $0.split = value }
            slot = value ? PadPhotoFace.left : PadPhotoFace.whole
        })
    }

    var body: some View {
        Form {
            previewSection
            pictureSection
            if store.image(onItsSide, slot) != nil {
                placingSection
            }
            modeSection
            lookSection
        }
        .navigationTitle("내 사진")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $picking) {
            PadPhotoPicker(isPresented: $picking) { image in
                store.choose(image, onItsSide: onItsSide, slot: slot)
                padTheme = .photo
            }
        }
    }

    private var previewSection: some View {
        Section {
            Picker("화면 방향", selection: sideBinding) {
                Text("세로 화면").tag(false)
                Text("가로 화면").tag(true)
            }
            .pickerStyle(.segmented)
            PadPhotoPreview(store: store, onItsSide: onItsSide)
                .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
            if padTheme != .photo {
                Button("키패드 디자인을 내 사진으로") { padTheme = .photo }
            }
        } footer: {
            Text(onItsSide
                 ? "가로 화면에서는 사진이 게임 화면 양옆에 깔려요. 세로 화면과 따로 기억해요."
                 : "세로 화면과 가로 화면은 사진과 설정을 따로 기억해요.")
        }
    }

    private var pictureSection: some View {
        Section {
            Picker("사진", selection: splitBinding) {
                Text("한 장").tag(false)
                Text("왼쪽·오른쪽 따로").tag(true)
            }
            .pickerStyle(.segmented)
            if face.split {
                Picker("고칠 쪽", selection: $slot) {
                    Text("왼쪽 (방향키 쪽)").tag(PadPhotoFace.left)
                    Text("오른쪽 (숫자키 쪽)").tag(PadPhotoFace.right)
                }
                .pickerStyle(.segmented)
            }
            Button {
                picking = true
            } label: {
                HStack {
                    Text(store.image(onItsSide, slot) == nil ? "사진 고르기" : "사진 바꾸기")
                    Spacer()
                    if store.loading {
                        ProgressView()
                    }
                }
            }
            if store.image(onItsSide, slot) != nil {
                Button("↻ 90° 회전") {
                    let current = slot
                    store.update(onItsSide) { $0.slots[current].turns = ($0.slots[current].turns + 1) % 4 }
                }
                Button("사진 빼기", role: .destructive) {
                    store.remove(onItsSide: onItsSide, slot: slot)
                }
            }
        } header: {
            Text("사진")
        } footer: {
            Text(store.image(onItsSide, slot) == nil
                 ? (face.split ? "이쪽에 사진이 없으면 디자인 바탕이 그대로 보여요." : "사진을 고르기 전까지는 다크 글래스로 보여요.")
                 : "고른 사진은 앱 안에 복사돼서 사진첩에서 지워도 남아요.")
        }
    }

    private var placingSection: some View {
        let current = slot
        return Section {
            slider("가로 위치", value: bind(\PadPhotoFace.slots[current].focusX), range: 0...1,
                   shown: place(face.slots[current].focusX, "왼쪽", "오른쪽"))
            slider("세로 위치", value: bind(\PadPhotoFace.slots[current].focusY), range: 0...1,
                   shown: place(face.slots[current].focusY, "위", "아래"))
            slider("확대", value: bind(\PadPhotoFace.slots[current].zoom), range: 1...3,
                   shown: "\(Int((face.slots[current].zoom * 100).rounded()))%")
        } header: {
            Text("사진 위치")
        }
    }

    private var modeSection: some View {
        Section {
            ForEach(PadPhotoMode.allCases) { mode in
                Button {
                    store.update(onItsSide) { $0.mode = mode }
                } label: {
                    HStack {
                        Text(mode.label).foregroundColor(.primary)
                        Spacer()
                        if mode == face.mode {
                            Image(systemName: "checkmark").foregroundColor(.accentColor)
                        }
                    }
                }
            }
        } header: {
            Text("입히는 방식")
        }
    }

    private var lookSection: some View {
        Section {
            slider("어둡게", value: bind(\PadPhotoFace.dim), range: 0...0.8, shown: "\(Int((face.dim * 100).rounded()))%")
            slider("흐림", value: bind(\PadPhotoFace.blur), range: 0...20, step: 1, shown: "\(Int(face.blur.rounded()))")
            slider("키 투명도 (유리 키·색 뽑기)", value: bind(\PadPhotoFace.glass), range: 0...1,
                   shown: "\(Int((face.glass * 100).rounded()))%")
        } footer: {
            Text("바꾸는 대로 바로 키패드에 보여요.")
        }
    }

    private func place(_ value: Double, _ low: String, _ high: String) -> String {
        value <= 0.1 ? low : value >= 0.9 ? high : "\(Int((value * 100).rounded()))%"
    }

    private func slider(_ title: String, value: Binding<Double>, range: ClosedRange<Double>, step: Double = 0.01, shown: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title).font(.caption).foregroundColor(.secondary)
                Spacer()
                Text(shown).font(.caption.monospacedDigit()).foregroundColor(.secondary)
            }
            Slider(value: value, in: range, step: step)
        }
    }
}
