import CoreImage
import PhotosUI
import SwiftUI
import UIKit

/// 키패드 디자인 → 내 사진: a picture of the player's own laid on the pad,
/// one of four ways, the same four the Android app offers. The keys keep
/// their places, sizes and shapes; the picture only fills them or lies under
/// them.
enum PadPhotoMode: Int, CaseIterable, Identifiable {
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

/// The 내 사진 picture and its settings, for every title. A change shows on
/// the pad at once, the way a design does.
///
/// The picture is copied into the app's own files when it is chosen -
/// upright, and no bigger than the pad can use - so it stays when the
/// original is deleted from the photo library.
final class PadPhotoStore: ObservableObject {
    static let shared = PadPhotoStore()

    @Published var mode: PadPhotoMode { didSet { save() } }
    /// How much the picture is darkened, 0 to 0.8.
    @Published var dim: Double { didSet { save() } }
    /// How soft it is drawn, 0 to 20 points.
    @Published var blur: Double { didSet { save() } }
    /// How clear the glass keys of 배경 and the tinted keys of 색 뽑기 are, 0
    /// to 1. Fully clear, a glass key is its outline and label over the picture.
    @Published var glass: Double { didSet { save() } }
    /// Which part of a taller picture shows, 0 (its top) to 1 (its bottom).
    /// Upright and on its side the pad is a different shape, so each keeps its own.
    @Published var portraitFocus: Double { didSet { save() } }
    @Published var landscapeFocus: Double { didSet { save() } }
    @Published private(set) var image: UIImage? = nil
    /// Whether a picture is being read in.
    @Published private(set) var loading = false

    private var layers: [String: UIImage] = [:]
    private var layerOrder: [String] = []
    private var tones: (average: Color, vivid: Color)?
    private let context = CIContext(options: [.useSoftwareRenderer: false])

    private static let longest: CGFloat = 1600
    private static let defaults = UserDefaults.standard

    private init() {
        let stored = Self.defaults
        mode = PadPhotoMode(rawValue: stored.integer(forKey: "pad.photo.mode")) ?? .backdrop
        dim = stored.object(forKey: "pad.photo.dim") as? Double ?? 0.35
        blur = stored.object(forKey: "pad.photo.blur") as? Double ?? 2
        glass = stored.object(forKey: "pad.photo.glass") as? Double ?? 0.45
        portraitFocus = stored.object(forKey: "pad.photo.focus.portrait") as? Double ?? 0.5
        landscapeFocus = stored.object(forKey: "pad.photo.focus.landscape") as? Double ?? 0.5
        if let url = Self.fileURL, let data = try? Data(contentsOf: url) {
            image = UIImage(data: data)
        }
    }

    private func save() {
        let stored = Self.defaults
        stored.set(mode.rawValue, forKey: "pad.photo.mode")
        stored.set(dim, forKey: "pad.photo.dim")
        stored.set(blur, forKey: "pad.photo.blur")
        stored.set(glass, forKey: "pad.photo.glass")
        stored.set(portraitFocus, forKey: "pad.photo.focus.portrait")
        stored.set(landscapeFocus, forKey: "pad.photo.focus.landscape")
    }

    private static var fileURL: URL? {
        guard let folder = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return nil
        }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder.appendingPathComponent("keypad_photo.jpg")
    }

    /// Takes a newly chosen picture: drawn upright at no more than 1600
    /// points on its long side, and written into the app's files.
    func choose(_ picked: UIImage) {
        loading = true
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
            if let url = Self.fileURL, let data = upright.jpegData(compressionQuality: 0.9) {
                try? data.write(to: url, options: .atomic)
            }
            DispatchQueue.main.async {
                self.layers.removeAll()
                self.layerOrder.removeAll()
                self.tones = nil
                self.image = upright
                self.loading = false
            }
        }
    }

    /// The picture filling `size` - cropped to that shape, `focus` saying
    /// which part of a taller one shows - softened by `blur` points and
    /// darkened by `dim`. Kept for the next frame, which nearly always asks
    /// for the same.
    func layer(size: CGSize, blur: Double, dim: Double, focus: Double) -> UIImage? {
        guard let source = image?.cgImage, size.width >= 1, size.height >= 1 else { return nil }
        let scale = UIScreen.main.scale
        let width = (size.width * scale).rounded()
        let height = (size.height * scale).rounded()
        let key = "\(Int(width))x\(Int(height))/\(Int((blur * 10).rounded()))/\(Int((dim * 100).rounded()))/\(Int((focus * 100).rounded()))"
        if let cached = layers[key] {
            return cached
        }

        let input = CIImage(cgImage: source)
        let fit = max(width / input.extent.width, height / input.extent.height)
        var picture = input.transformed(by: CGAffineTransform(scaleX: fit, y: fit))
        let drawnWidth = input.extent.width * fit
        let drawnHeight = input.extent.height * fit
        // Core Image counts y up from the bottom: the picture's top shows
        // when the crop sits as high as it goes.
        let crop = CGRect(x: ((drawnWidth - width) / 2).rounded(), y: ((drawnHeight - height) * CGFloat(1 - focus)).rounded(),
                          width: width, height: height)
        if blur >= 0.5 {
            picture = picture.clampedToExtent().applyingGaussianBlur(sigma: blur * Double(scale) / 2)
        }
        picture = picture.cropped(to: crop)
        if dim > 0 {
            let shade = CIImage(color: CIColor(red: 0, green: 0, blue: 0, alpha: CGFloat(min(dim, 1)))).cropped(to: crop)
            picture = shade.composited(over: picture)
        }
        guard let rendered = context.createCGImage(picture, from: crop) else { return nil }
        let made = UIImage(cgImage: rendered, scale: scale, orientation: .up)

        layers[key] = made
        layerOrder.append(key)
        if layerOrder.count > 8 {
            layers.removeValue(forKey: layerOrder.removeFirst())
        }
        return made
    }

    /// Two colours from the picture: its average, and its most vivid one that
    /// is not dark - what 색 뽑기 paints the keys and their accents with.
    func palette() -> (average: Color, vivid: Color) {
        if let tones { return tones }
        let fallback = (average: Color(.sRGB, red: 40 / 255, green: 44 / 255, blue: 56 / 255, opacity: 1),
                        vivid: Color(.sRGB, red: 84 / 255, green: 199 / 255, blue: 214 / 255, opacity: 1))
        guard let source = image?.cgImage else { return fallback }
        let width = 32, height = 20
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

        var red = 0.0, green = 0.0, blue = 0.0
        var best = -1.0
        var vivid = (84.0, 199.0, 214.0)
        for index in stride(from: 0, to: pixels.count, by: 4) {
            let r = Double(pixels[index]), g = Double(pixels[index + 1]), b = Double(pixels[index + 2])
            red += r
            green += g
            blue += b
            let high = max(r, g, b), low = min(r, g, b)
            let saturation = high > 0 ? (high - low) / high : 0
            let value = high / 255
            let score = saturation * value * (value > 0.35 ? 1 : 0.2)
            if score > best {
                best = score
                vivid = (r, g, b)
            }
        }
        let count = Double(width * height)
        let found = (average: Color(.sRGB, red: red / count / 255, green: green / count / 255, blue: blue / count / 255, opacity: 1),
                     vivid: Color(.sRGB, red: vivid.0 / 255, green: vivid.1 / 255, blue: vivid.2 / 255, opacity: 1))
        tones = found
        return found
    }
}

/// What the pad draws with in 내 사진: the picture's settings as they are
/// now, and the palette its keys take for the way it is laid on. Nil - the
/// pad keeps 다크 글래스 - until a picture is chosen.
struct PadPhotoLook {
    let store: PadPhotoStore
    let mode: PadPhotoMode
    /// Whether the pad is laid out on its side, which keeps its own focus.
    let landscape: Bool

    init?(theme: PadTheme, store: PadPhotoStore, landscape: Bool) {
        guard theme == .photo, store.image != nil else { return nil }
        self.store = store
        self.mode = store.mode
        self.landscape = landscape
    }

    var focus: Double { landscape ? store.landscapeFocus : store.portraitFocus }

    /// The tray: the picture, a dark ground or 다크 글래스's, by the way it is laid on.
    func tray(size: CGSize) -> UIImage? {
        switch mode {
        case .backdrop: return store.layer(size: size, blur: store.blur, dim: store.dim, focus: focus)
        case .tint: return store.layer(size: size, blur: store.blur + 10, dim: min(0.85, store.dim + 0.25), focus: focus)
        case .stencil, .disc: return nil
        }
    }

    /// What shows through the keys and the disc, for the two ways that cut
    /// the picture to them.
    func cut(size: CGSize) -> UIImage? {
        switch mode {
        // Frosted, less so the clearer the keys are set: fully clear they
        // show the picture just as the tray does.
        case .backdrop: return store.layer(size: size, blur: store.blur + 8 * (1 - store.glass), dim: store.dim, focus: focus)
        case .stencil: return store.layer(size: size, blur: store.blur, dim: store.dim * 0.8, focus: focus)
        case .tint, .disc: return nil
        }
    }

    /// The disc's own picture, for 방향 원판에만.
    func disc(diameter: CGFloat) -> UIImage? {
        mode == .disc ? store.layer(size: CGSize(width: diameter, height: diameter), blur: store.blur, dim: store.dim, focus: focus) : nil
    }

    private static func rgb(_ red: Double, _ green: Double, _ blue: Double, _ alpha: Double = 1) -> Color {
        Color(.sRGB, red: red / 255, green: green / 255, blue: blue / 255, opacity: alpha)
    }

    private static let accent = rgb(84, 199, 214)
    private static let accentInk = rgb(12, 42, 47)
    private static let backInk = rgb(255, 138, 128)

    var palette: PadPalette {
        let white = Color.white
        let accent = Self.accent
        let glassShare = store.glass
        switch mode {
        case .backdrop:
            let face = white.opacity(0.30 * (1 - glassShare))
            let ok = white.opacity(0.38 * (1 - glassShare))
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
        case .stencil:
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
        case .tint:
            let tones = store.palette()
            let vivid = tones.vivid
            let lift = Self.luminance(vivid) < 0.3
            let tinted = lift ? Self.mix(vivid, .white, 0.45) : vivid
            let dark = Self.mix(Self.rgb(18, 19, 24), tones.average, 0.3)
            let through = 1 - glassShare * 0.85
            let face = Self.mix(dark, vivid, 0.18).opacity(through)
            let ring = Self.mix(Self.mix(dark, vivid, 0.18), .black, 0.1).opacity(through)
            let ok = Self.mix(Self.mix(dark, vivid, 0.18), .white, 0.06).opacity(through)
            let rim = tinted.opacity(0.45)
            let ink = Self.rgb(244, 245, 248)
            return PadPalette(
                trayTop: Self.rgb(17, 18, 22), trayBottom: Self.rgb(17, 18, 22),
                faceTop: face, faceBottom: face, edge: rim,
                ink: ink, subInk: Self.mix(tinted, .white, 0.35),
                pressedTop: tinted.opacity(0.55), pressedBottom: tinted.opacity(0.55), pressedInk: white,
                shadow: nil, radius: 16, pillNumbers: false,
                hollowFunctions: false, functionEdge: rim, functionInk: ink,
                pillFunctions: true, functionRadius: 16,
                saveFill: tinted, saveEdge: tinted, saveInk: Self.luminance(tinted) > 0.55 ? Self.rgb(19, 22, 28) : white,
                backEdge: rim, backInk: Self.backInk,
                ringTop: ring, ringBottom: ring, ringEdge: rim,
                ringSeam: tinted.opacity(0.25), arrow: tinted, arrowPressed: white,
                wedgePressed: tinted.opacity(0.45),
                okTop: ok, okBottom: ok, okEdge: rim, okInk: ink
            )
        case .disc:
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
    }

    private static func components(_ color: Color) -> (Double, Double, Double) {
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        UIColor(color).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (Double(red), Double(green), Double(blue))
    }

    /// `from` moved `amount` (0 to 1) of the way to `to`.
    private static func mix(_ from: Color, _ to: Color, _ amount: Double) -> Color {
        let a = components(from), b = components(to)
        return Color(.sRGB, red: a.0 + (b.0 - a.0) * amount, green: a.1 + (b.1 - a.1) * amount,
                     blue: a.2 + (b.2 - a.2) * amount, opacity: 1)
    }

    /// How light a colour reads, 0 to 1.
    private static func luminance(_ color: Color) -> Double {
        let c = components(color)
        return 0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2
    }
}

/// The tray under the pad: the design's, or 내 사진's picture laid over it.
struct PadTray: View {
    let palette: PadPalette
    let photo: PadPhotoLook?
    var cornerRadius: CGFloat = 14

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                palette.tray
                if let picture = photo?.tray(size: geometry.size) {
                    Image(uiImage: picture)
                        .resizable()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }
}

/// The picture where 내 사진 cuts it to the pad: through every key and the
/// disc (배경, 찍어내기), or on the disc alone (원판에만). Drawn under the
/// keys' own faces, which lay their glass or outline over it.
struct PadPhotoCut: View {
    let photo: PadPhotoLook
    let size: CGSize
    let ring: PadRing?
    /// Each key's rectangle and corner radius, as the keys are drawn.
    let shapes: [(CGRect, CGFloat)]

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let picture = photo.cut(size: size) {
                Image(uiImage: picture)
                    .resizable()
                    .frame(width: size.width, height: size.height)
                    .mask(
                        ZStack(alignment: .topLeading) {
                            ForEach(shapes.indices, id: \.self) { index in
                                let shape = shapes[index]
                                RoundedRectangle(cornerRadius: shape.1, style: .continuous)
                                    .frame(width: shape.0.width, height: shape.0.height)
                                    .position(x: shape.0.midX, y: shape.0.midY)
                            }
                            if let ring {
                                Circle()
                                    .frame(width: ring.radius * 2, height: ring.radius * 2)
                                    .position(x: ring.center.x, y: ring.center.y)
                            }
                        }
                        .frame(width: size.width, height: size.height, alignment: .topLeading)
                    )
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

/// 내 사진's settings: the picture, the way it is laid on, and how dark and
/// soft it is drawn, over a picture of the pad that follows every change.
struct PadPhotoSettingsView: View {
    /// Whether the pad is on its side now, so 사진 위치 moves that way's focus.
    let landscape: Bool

    @ObservedObject private var store = PadPhotoStore.shared
    @AppStorage(SettingKey.padTheme) private var padTheme = PadTheme.gold
    @State private var picking = false

    var body: some View {
        Form {
            Section {
                GeometryReader { geometry in
                    let look = PadPhotoLook(theme: .photo, store: store, landscape: false)
                    let palette = look?.palette ?? PadPalette.glass
                    ZStack {
                        PadTray(palette: palette, photo: look, cornerRadius: 10)
                        PadFaces(layout: PadLayout.standard(.below), size: geometry.size, pressed: [],
                                 labelScale: 0.8, palette: palette, letters: true, photo: look)
                    }
                }
                .frame(height: 180)
                .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))

                Button {
                    picking = true
                } label: {
                    HStack {
                        Text(store.image == nil ? "사진 고르기" : "사진 바꾸기")
                        Spacer()
                        if store.loading {
                            ProgressView()
                        }
                    }
                }
                if padTheme != .photo {
                    Button("키패드 디자인을 내 사진으로") { padTheme = .photo }
                }
            } footer: {
                Text(store.image == nil
                     ? "사진을 고르기 전까지는 다크 글래스로 보여요. 고른 사진은 앱 안에 복사돼서 사진첩에서 지워도 남아요."
                     : "고른 사진은 앱 안에 복사돼서 사진첩에서 지워도 남아요.")
            }

            Section {
                ForEach(PadPhotoMode.allCases) { mode in
                    Button {
                        store.mode = mode
                    } label: {
                        HStack {
                            Text(mode.label).foregroundColor(.primary)
                            Spacer()
                            if mode == store.mode {
                                Image(systemName: "checkmark").foregroundColor(.accentColor)
                            }
                        }
                    }
                }
            } header: {
                Text("입히는 방식")
            }

            Section {
                slider("어둡게", value: $store.dim, range: 0...0.8, shown: "\(Int((store.dim * 100).rounded()))%")
                slider("흐림", value: $store.blur, range: 0...20, step: 1, shown: "\(Int(store.blur.rounded()))")
                slider("키 투명도 (유리 키·색 뽑기)", value: $store.glass, range: 0...1, shown: "\(Int((store.glass * 100).rounded()))%")
                if landscape {
                    slider("사진 위치 (가로 화면)", value: $store.landscapeFocus, range: 0...1, shown: focusText(store.landscapeFocus))
                } else {
                    slider("사진 위치 (세로 화면)", value: $store.portraitFocus, range: 0...1, shown: focusText(store.portraitFocus))
                }
            } footer: {
                Text("바꾸는 대로 바로 키패드에 보여요. 세로·가로 화면은 사진 위치를 따로 기억해요.")
            }
        }
        .navigationTitle("내 사진")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $picking) {
            PadPhotoPicker(isPresented: $picking) { image in
                store.choose(image)
                padTheme = .photo
            }
        }
    }

    private func focusText(_ value: Double) -> String {
        value <= 0.1 ? "위" : value >= 0.9 ? "아래" : "\(Int((value * 100).rounded()))%"
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
