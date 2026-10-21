import SwiftUI
import UniformTypeIdentifiers

/// The carrier filter above the list. 기타 takes what no carrier claims, DRM
/// downloads included, as on Android.
private enum CarrierFilter: String, CaseIterable, Identifiable {
    case all = "전체"
    case skt = "SKT"
    case ktf = "KTF"
    case lgt = "LGT"
    case other = "기타"

    var id: String { rawValue }

    func admits(_ carrier: String) -> Bool {
        switch self {
        case .all: return true
        case .skt: return carrier == "SKT"
        case .ktf: return carrier == "KTF"
        case .lgt: return carrier == "LGT"
        case .other: return !["SKT", "KTF", "LGT"].contains(carrier)
        }
    }
}

/// What the file picker is open for.
private enum ImportTarget {
    case games
    case save

    var types: [UTType] {
        switch self {
        case .games: return [.zip, .data, .item]
        case .save: return [.zip]
        }
    }
}

/// A file handed to the share sheet.
struct SharedFile: Identifiable {
    let id = UUID()
    let url: URL
}

/// A message for the one alert the screen shows.
private struct Notice {
    let title: String
    let message: String
}

/// A question asked on a card before something that cannot be taken back.
private struct PendingConfirm {
    let title: String
    let message: String
    let action: String
    let danger: Bool
    let run: () -> Void
}

struct LibraryView: View {
    @State private var games: [GameFile] = []
    @State private var carriers: [String: String] = [:]
    /// Each game's icon once read, for its cover in the grid.
    @State private var icons: [String: UIImage] = [:]
    @State private var search = ""
    @State private var filter = CarrierFilter.all
    @AppStorage(Favorites.key) private var favoritesText = ""
    /// The list in its dark look rather than its light one: the player's
    /// choice from the moon/sun button, kept between runs and never the
    /// device's own setting. The game screen and pad keep their own look.
    @AppStorage("library.dark") private var dark = false
    /// Covers in a grid rather than rows, and the order: the player's choice,
    /// kept between runs.
    @AppStorage("library.grid") private var grid = false
    @AppStorage("library.sort") private var sortRaw = LibrarySort.recent.rawValue

    @State private var editMode = EditMode.inactive
    @State private var selection = Set<String>()
    /// The game whose action card is up, and the question a card is asking.
    @State private var menuFor: GameFile?
    @State private var pending: PendingConfirm?

    @State private var playing: GameFile?
    @State private var sharing: SharedFile?
    @State private var notice: Notice?

    /// The game whose save list is open, and what to do once it closes: tell
    /// the player how an import went, or open the file picker.
    @State private var savesFor: GameFile?
    @State private var afterSaves: String?
    @State private var pickAfterSaves = false

    private var favorites: Set<String> { Favorites.decode(favoritesText) }

    private var sort: LibrarySort { LibrarySort(rawValue: sortRaw) ?? .recent }

    private var shown: [GameFile] {
        sort.sorted(games.filter { filter.admits(carriers[$0.name] ?? "") && TitleSearch.matches($0.title, search) })
    }

    var body: some View {
        NavigationView {
            Group {
                // Picking games to act on is done in the list.
                if grid && !editMode.isEditing {
                    gridView
                } else {
                    list
                }
            }
                .navigationTitle("Mini Mobile")
                .navigationBarTitleDisplayMode(.inline)
                .searchable(text: $search, prompt: Text("게임 검색 (초성 가능)"))
                .toolbar { toolbar }
                .environment(\.editMode, $editMode)
        }
        .navigationViewStyle(.stack)
        .alert(notice?.title ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
            Button("확인", role: .cancel) {}
        } message: {
            Text(notice?.message ?? "")
        }
        .sheet(item: $sharing) { file in
            ShareSheet(items: [file.url])
                .preferredColorScheme(dark ? .dark : .light)
        }
        .sheet(item: $savesFor, onDismiss: savesClosed) { game in
            SaveListView(
                game: game,
                finished: { afterSaves = $0 },
                pickElsewhere: { pickAfterSaves = true }
            )
            .preferredColorScheme(dark ? .dark : .light)
        }
        .fullScreenCover(item: $playing, onDismiss: reload) { game in
            GameView(game: game)
        }
        .overlay(cards)
        .preferredColorScheme(dark ? .dark : .light)
        .onAppear {
            PlayHistory.dropOldCovers()
            reload()
        }
    }

    // MARK: - Cards

    /// The action card of a long-pressed game, or the question one asked.
    @ViewBuilder
    private var cards: some View {
        let palette = LibraryPalette.of(dark: dark)
        if let ask = pending {
            LibraryCardOverlay(onDismiss: closeCards) {
                ConfirmCard(
                    title: ask.title, message: ask.message, action: ask.action, danger: ask.danger,
                    palette: palette, dark: dark,
                    onConfirm: {
                        closeCards()
                        ask.run()
                    },
                    onCancel: closeCards
                )
            }
            .transition(.opacity)
        } else if let game = menuFor {
            LibraryCardOverlay(onDismiss: closeCards) {
                TileMenuCard(
                    title: game.title, tiles: tiles(for: game), dangers: dangers(for: game), palette: palette, onClose: closeCards
                ) {
                    GameCover(game: game, icon: icons[game.name])
                } meta: {
                    HStack(spacing: 6) {
                        CarrierBadge(carrier: carriers[game.name])
                            .scaleEffect(0.85, anchor: .leading)
                            .frame(width: 34, alignment: .leading)
                        Text(game.sizeText)
                            .font(.system(size: 11.5))
                            .foregroundColor(palette.muted)
                    }
                }
            }
            .transition(.opacity)
        }
    }

    private func tiles(for game: GameFile) -> [TileAction] {
        let favorite = favorites.contains(game.name)
        return [
            TileAction(icon: "play.fill", title: "실행", note: "바로 시작") { choose { playing = game } },
            TileAction(icon: favorite ? "star.slash" : "star", title: favorite ? "즐겨찾기 해제" : "즐겨찾기", note: "목록 맨 위에") {
                choose { toggleFavorite(game) }
            },
            TileAction(icon: "square.and.arrow.up", title: "세이브 꺼내기", note: ".zip 으로 보내기") { choose { exportSave(game) } },
            TileAction(icon: "square.and.arrow.down", title: "세이브 불러오기", note: "목록에서 고르기") { choose { savesFor = game } },
        ]
    }

    private func dangers(for game: GameFile) -> [TileAction] {
        [
            TileAction(icon: "arrow.counterclockwise", title: "데이터 초기화", note: "") {
                pending = PendingConfirm(
                    title: "\(game.title.withObject) 초기화할까요?",
                    message: "이 게임이 저장한 세이브를 모두 지워요. 되돌릴 수 없어요.\n남겨두려면 먼저 \"세이브 꺼내기\"로 백업하세요.",
                    action: "초기화", danger: true
                ) { eraseSave(game) }
            },
            TileAction(icon: "trash", title: "목록에서 삭제", note: "") {
                pending = PendingConfirm(
                    title: "\(game.title.withObject) 목록에서 삭제할까요?",
                    message: "저장한 세이브는 그대로 남아요. 같은 게임을 다시 넣으면 이어서 할 수 있어요.",
                    action: "삭제", danger: true
                ) {
                    Library.delete(game)
                    reload()
                }
            },
        ]
    }

    /// Closes the card, then does what was chosen on it - a sheet or the game
    /// opens over the list once the card is gone.
    private func choose(_ action: () -> Void) {
        closeCards()
        action()
    }

    private func closeCards() {
        withAnimation(.easeOut(duration: 0.15)) {
            menuFor = nil
            pending = nil
        }
    }

    private func openMenu(_ game: GameFile) {
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        withAnimation(.easeOut(duration: 0.15)) { menuFor = game }
    }

    // MARK: - List

    private var list: some View {
        let visible = self.shown
        let starred = visible.filter { favorites.contains($0.name) }
        let rest = visible.filter { !favorites.contains($0.name) }

        return List(selection: $selection) {
            if !editMode.isEditing {
                filterBar
            }
            if games.isEmpty {
                Text("오른쪽 위 + 버튼으로 게임 파일(zip, jar)을 가져오세요.\n'파일' 앱의 Mini Mobile/Games 폴더에 넣어도 됩니다.")
                    .foregroundColor(.secondary)
            } else if visible.isEmpty {
                Text("맞는 게임이 없습니다.")
                    .foregroundColor(.secondary)
            }
            if !starred.isEmpty {
                Section(header: Text("즐겨찾기")) {
                    ForEach(starred) { row($0) }
                }
            }
            if !rest.isEmpty {
                Section(header: Text(starred.isEmpty ? "게임 \(rest.count)" : "나머지 \(rest.count)")) {
                    ForEach(rest) { row($0) }
                }
            }
        }
        .listStyle(.insetGrouped)
        .refreshable { reload() }
        .safeAreaInset(edge: .bottom) {
            if editMode.isEditing {
                editBar
            }
        }
    }

    private var filterBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(CarrierFilter.allCases) { option in
                    let count = games.filter { option.admits(carriers[$0.name] ?? "") }.count
                    Button {
                        filter = option
                    } label: {
                        Text("\(option.rawValue) \(count)")
                            .font(.subheadline.weight(.semibold))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .background(
                                Capsule().fill(filter == option ? Color.accentColor : Color.secondary.opacity(0.15))
                            )
                            .foregroundColor(filter == option ? .white : .primary)
                    }
                    .buttonStyle(.plain)
                }
                Button {
                    sortRaw = (sort == .recent ? LibrarySort.name : LibrarySort.recent).rawValue
                } label: {
                    HStack(spacing: 3) {
                        Text(sort.label)
                        Image(systemName: "chevron.down").font(.caption2.weight(.bold))
                    }
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .foregroundColor(.accentColor)
                    .overlay(Capsule().stroke(Color.accentColor.opacity(0.5), lineWidth: 1))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("정렬: \(sort.label)")
            }
        }
        .listRowInsets(EdgeInsets(top: 6, leading: 12, bottom: 6, trailing: 12))
        .listRowBackground(Color.clear)
    }

    @ViewBuilder
    private func row(_ game: GameFile) -> some View {
        let favorite = favorites.contains(game.name)
        let label = HStack(spacing: 10) {
            CarrierBadge(carrier: carriers[game.name])
            Text(game.title)
                .foregroundColor(.primary)
                .lineLimit(2)
            Spacer(minLength: 0)
            if favorite {
                Image(systemName: "star.fill")
                    .foregroundColor(.yellow)
            }
        }

        Group {
            if editMode.isEditing {
                label
            } else {
                label
                    .contentShape(Rectangle())
                    .onTapGesture { playing = game }
                    .onLongPressGesture(minimumDuration: 0.45) { openMenu(game) }
            }
        }
        .swipeActions(edge: .leading) {
            Button { toggleFavorite(game) } label: {
                Label(favorite ? "해제" : "즐겨찾기", systemImage: favorite ? "star.slash" : "star")
            }
            .tint(.yellow)
        }
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) {
                Library.delete(game)
                reload()
            } label: {
                Label("삭제", systemImage: "trash")
            }
        }
    }

    // MARK: - Grid

    /// The games as covers, three to a row, each its icon over its title.
    private var gridView: some View {
        let visible = self.shown
        let starred = visible.filter { favorites.contains($0.name) }
        let rest = visible.filter { !favorites.contains($0.name) }

        return ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                filterBar
                if games.isEmpty {
                    Text("오른쪽 위 + 버튼으로 게임 파일(zip, jar)을 가져오세요.\n'파일' 앱의 Mini Mobile/Games 폴더에 넣어도 됩니다.")
                        .foregroundColor(.secondary)
                } else if visible.isEmpty {
                    Text("맞는 게임이 없습니다.")
                        .foregroundColor(.secondary)
                }
                if !starred.isEmpty {
                    gridSection("즐겨찾기", starred)
                }
                if !rest.isEmpty {
                    gridSection(starred.isEmpty ? "게임 \(rest.count)" : "나머지 \(rest.count)", rest)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 20)
        }
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }

    private func gridSection(_ title: String, _ games: [GameFile]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.footnote.weight(.semibold))
                .foregroundColor(.secondary)
                .textCase(.uppercase)
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), alignment: .leading, spacing: 14) {
                ForEach(games) { tile($0) }
            }
        }
    }

    private func tile(_ game: GameFile) -> some View {
        let favorite = favorites.contains(game.name)
        let shape = RoundedRectangle(cornerRadius: 12, style: .continuous)
        return VStack(alignment: .leading, spacing: 3) {
            Color.clear
                .aspectRatio(1, contentMode: .fit)
                .overlay(GameCover(game: game, icon: icons[game.name]))
                .clipShape(shape)
                .overlay(alignment: .topLeading) {
                    CarrierBadge(carrier: carriers[game.name])
                        .scaleEffect(0.85, anchor: .topLeading)
                        .background(RoundedRectangle(cornerRadius: 5).fill(Color(.systemBackground).opacity(0.85)))
                        .padding(6)
                }
                .overlay(alignment: .topTrailing) {
                    if favorite {
                        Image(systemName: "star.fill")
                            .foregroundColor(.yellow)
                            .shadow(color: .black.opacity(0.5), radius: 2, y: 1)
                            .padding(6)
                    }
                }
                .shadow(color: .black.opacity(0.18), radius: 4, y: 2)
            Text(game.title)
                .font(.footnote.weight(.semibold))
                .foregroundColor(.primary)
                .lineLimit(1)
                .padding(.top, 3)
        }
        .contentShape(Rectangle())
        .onTapGesture { playing = game }
        .onLongPressGesture(minimumDuration: 0.45) { openMenu(game) }
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .navigationBarLeading) {
            Button(editMode.isEditing ? "완료" : "선택") {
                withAnimation {
                    editMode = editMode.isEditing ? .inactive : .active
                    selection.removeAll()
                }
            }
            .disabled(games.isEmpty && !editMode.isEditing)
        }
        ToolbarItemGroup(placement: .navigationBarTrailing) {
            Button {
                withAnimation { grid.toggle() }
            } label: {
                Image(systemName: grid ? "list.bullet" : "square.grid.3x2")
            }
            .accessibilityLabel(grid ? "목록으로 보기" : "격자로 보기")
            .disabled(editMode.isEditing)
            Button {
                withAnimation { dark.toggle() }
            } label: {
                Image(systemName: dark ? "sun.max.fill" : "moon.fill")
            }
            .accessibilityLabel(dark ? "라이트 모드" : "다크 모드")
            Menu {
                Button { pick(.games) } label: { Label("게임 가져오기", systemImage: "plus") }
                Button { pick(.save) } label: { Label("세이브 가져오기", systemImage: "square.and.arrow.down") }
            } label: {
                Image(systemName: "plus")
            }
        }
    }

    /// Select-all, star and delete for the games picked in edit mode.
    private var editBar: some View {
        let visible = self.shown
        let everything = !visible.isEmpty && selection.count == visible.count

        return HStack {
            Button(everything ? "선택 해제" : "모두 선택") {
                selection = everything ? [] : Set(visible.map(\.id))
            }
            Spacer()
            Button {
                let chosen = games.filter { selection.contains($0.id) }
                var names = favorites
                let starring = chosen.contains { !names.contains($0.name) }
                for game in chosen {
                    if starring { names.insert(game.name) } else { names.remove(game.name) }
                }
                favoritesText = Favorites.encode(names)
            } label: {
                Label("즐겨찾기", systemImage: "star")
            }
            .disabled(selection.isEmpty)
            Spacer()
            Button(role: .destructive) {
                let count = selection.count
                withAnimation(.easeOut(duration: 0.15)) {
                    pending = PendingConfirm(
                        title: "\(count)개 게임을 삭제할까요?",
                        message: "목록에서만 빠지고, 저장한 세이브는 그대로 남아요.\n같은 게임을 다시 넣으면 이어서 할 수 있어요.",
                        action: "삭제", danger: true, run: deleteSelection
                    )
                }
            } label: {
                Label("삭제 \(selection.count)", systemImage: "trash")
            }
            .disabled(selection.isEmpty)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
        .background(.bar)
    }

    // MARK: - Actions

    /// Fills in the icons: those already read at once, the rest as their
    /// archives are read in the background.
    private func loadIcons() {
        var unread: [GameFile] = []
        for game in games where icons[game.name] == nil {
            switch GameIcons.known(game) {
            case .some(let image?): icons[game.name] = image
            case .some(nil): break
            case nil: unread.append(game)
            }
        }
        guard !unread.isEmpty else { return }
        DispatchQueue.global(qos: .utility).async {
            for game in unread {
                if let image = GameIcons.read(game) {
                    DispatchQueue.main.async { icons[game.name] = image }
                }
            }
        }
    }

    private func reload() {
        games = Library.games()
        loadIcons()
        let pending = games.filter { carriers[$0.name] == nil }
        guard !pending.isEmpty else { return }

        // Reading a game to tell its carrier can take a moment for a large
        // one; the badges fill in as they are found.
        DispatchQueue.global(qos: .userInitiated).async {
            for game in pending {
                let carrier = Library.carrier(of: game)
                DispatchQueue.main.async { carriers[game.name] = carrier }
            }
        }
    }

    private func toggleFavorite(_ game: GameFile) {
        var names = favorites
        if names.contains(game.name) {
            names.remove(game.name)
        } else {
            names.insert(game.name)
        }
        favoritesText = Favorites.encode(names)
    }

    private func deleteSelection() {
        games.filter { selection.contains($0.id) }.forEach(Library.delete)
        selection.removeAll()
        editMode = .inactive
        reload()
    }

    /// Opens the file picker for games (several at once) or a save zip.
    private func pick(_ target: ImportTarget) {
        DocumentPicker.present(types: target.types, multiple: target == .games) { urls in
            handleImport(urls, target: target)
        }
    }

    private func handleImport(_ urls: [URL], target: ImportTarget) {
        switch target {
        case .save:
            guard let url = urls.first else { return }
            do {
                let restored = try Library.importSave(from: url)
                notice = restored > 0
                    ? Notice(title: "세이브 가져오기", message: "파일 \(restored)개를 복원했습니다.")
                    : Notice(title: "세이브 가져오기", message: "이 zip에는 세이브가 없습니다.")
            } catch {
                notice = Notice(title: "세이브 가져오기 실패", message: error.localizedDescription)
            }
        case .games:
            var failures: [String] = []
            for url in urls {
                do {
                    try Library.importGame(from: url)
                } catch {
                    failures.append("\(url.lastPathComponent): \(error.localizedDescription)")
                }
            }
            if !failures.isEmpty {
                notice = Notice(title: "가져오기 실패", message: failures.joined(separator: "\n"))
            }
            reload()
        }
    }

    private func exportSave(_ game: GameFile) {
        do {
            if let url = try Library.exportSave(game) {
                sharing = SharedFile(url: url)
            } else {
                notice = Notice(title: "세이브 내보내기", message: "\(game.title)은(는) 아직 저장한 데이터가 없습니다.")
            }
        } catch {
            notice = Notice(title: "세이브 내보내기 실패", message: error.localizedDescription)
        }
    }

    /// After the save list: a picker asked for opens only once the sheet is
    /// gone, as two cannot be up at once, and the same for the notice.
    private func savesClosed() {
        if let message = afterSaves {
            afterSaves = nil
            notice = Notice(title: "세이브 불러오기", message: message)
        }
        if pickAfterSaves {
            pickAfterSaves = false
            pick(.save)
        }
    }

    private func eraseSave(_ game: GameFile) {
        do {
            let removed = try Library.eraseSave(game)
            notice = Notice(title: "세이브 삭제", message: removed > 0 ? "파일 \(removed)개를 지웠습니다." : "지울 세이브가 없습니다.")
        } catch {
            notice = Notice(title: "세이브 삭제 실패", message: error.localizedDescription)
        }
    }
}

/// The carrier as a small colored tag, in the Android library's colors.
struct CarrierBadge: View {
    let carrier: String?
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let (text, color) = Self.style(carrier, dark: scheme == .dark)
        Text(text)
            .font(.system(size: 11, weight: .bold))
            .foregroundColor(color)
            .frame(width: 38, height: 20)
            .background(RoundedRectangle(cornerRadius: 5).fill(color.opacity(0.14)))
            .overlay(RoundedRectangle(cornerRadius: 5).stroke(color.opacity(0.35), lineWidth: 1))
    }

    private static func style(_ carrier: String?, dark: Bool) -> (String, Color) {
        if dark {
            // Lighter inks that read on the dark list, as on Android.
            switch carrier {
            case "SKT": return ("SKT", Color(red: 240 / 255, green: 138 / 255, blue: 75 / 255))
            case "KTF": return ("KTF", Color(red: 106 / 255, green: 165 / 255, blue: 255 / 255))
            case "LGT": return ("LGT", Color(red: 224 / 255, green: 122 / 255, blue: 203 / 255))
            case "DRM": return ("DRM", Color(red: 224 / 255, green: 122 / 255, blue: 122 / 255))
            case nil: return ("···", .secondary)
            default: return ("기타", Color(red: 138 / 255, green: 156 / 255, blue: 144 / 255))
            }
        }
        switch carrier {
        case "SKT": return ("SKT", Color(red: 194 / 255, green: 65 / 255, blue: 12 / 255))
        case "KTF": return ("KTF", Color(red: 29 / 255, green: 95 / 255, blue: 191 / 255))
        case "LGT": return ("LGT", Color(red: 163 / 255, green: 38 / 255, blue: 143 / 255))
        case "DRM": return ("DRM", Color(red: 153 / 255, green: 57 / 255, blue: 57 / 255))
        case nil: return ("···", .secondary)
        default: return ("기타", Color(red: 100 / 255, green: 117 / 255, blue: 104 / 255))
        }
    }
}

/// The system share sheet: save to Files, AirDrop, send to another app.
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
