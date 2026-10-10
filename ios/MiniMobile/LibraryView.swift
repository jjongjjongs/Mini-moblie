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

/// What the one file importer is open for.
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

struct LibraryView: View {
    @State private var games: [GameFile] = []
    @State private var carriers: [String: String] = [:]
    @State private var search = ""
    @State private var filter = CarrierFilter.all
    @AppStorage(Favorites.key) private var favoritesText = ""

    @State private var editMode = EditMode.inactive
    @State private var selection = Set<String>()
    @State private var confirmingDelete = false
    @State private var erasing: GameFile?

    @State private var importer: ImportTarget?
    @State private var playing: GameFile?
    @State private var sharing: SharedFile?
    @State private var notice: Notice?

    /// The game whose save list is open, and what to do once it closes: tell
    /// the player how an import went, or open the file picker.
    @State private var savesFor: GameFile?
    @State private var afterSaves: String?
    @State private var pickAfterSaves = false

    private var favorites: Set<String> { Favorites.decode(favoritesText) }

    private var shown: [GameFile] {
        games.filter { filter.admits(carriers[$0.name] ?? "") && TitleSearch.matches($0.title, search) }
    }

    var body: some View {
        NavigationView {
            list
                .navigationTitle("Mini Mobile")
                .navigationBarTitleDisplayMode(.inline)
                .searchable(text: $search, prompt: Text("게임 검색 (초성 가능)"))
                .toolbar { toolbar }
                .environment(\.editMode, $editMode)
        }
        .navigationViewStyle(.stack)
        .fileImporter(
            isPresented: Binding(get: { importer != nil }, set: { if !$0 { importer = nil } }),
            allowedContentTypes: importer?.types ?? [.item],
            allowsMultipleSelection: importer == .games
        ) { result in
            let target = importer
            importer = nil
            handleImport(result, target: target)
        }
        .alert(notice?.title ?? "", isPresented: Binding(get: { notice != nil }, set: { if !$0 { notice = nil } })) {
            Button("확인", role: .cancel) {}
        } message: {
            Text(notice?.message ?? "")
        }
        .confirmationDialog(
            "선택한 게임 \(selection.count)개를 지울까요?",
            isPresented: $confirmingDelete,
            titleVisibility: .visible
        ) {
            Button("게임 삭제", role: .destructive, action: deleteSelection)
        } message: {
            Text("세이브는 남습니다. 같은 게임을 다시 넣으면 이어서 할 수 있습니다.")
        }
        .confirmationDialog(
            "\(erasing?.title ?? "") 세이브를 지울까요?",
            isPresented: Binding(get: { erasing != nil }, set: { if !$0 { erasing = nil } }),
            titleVisibility: .visible
        ) {
            Button("세이브 삭제", role: .destructive) {
                if let game = erasing { eraseSave(game) }
            }
        } message: {
            Text("되돌릴 수 없습니다. 먼저 내보내기로 백업해 두세요.")
        }
        .sheet(item: $sharing) { file in
            ShareSheet(items: [file.url])
        }
        .sheet(item: $savesFor, onDismiss: savesClosed) { game in
            SaveListView(
                game: game,
                finished: { afterSaves = $0 },
                pickElsewhere: { pickAfterSaves = true }
            )
        }
        .fullScreenCover(item: $playing) { game in
            GameView(game: game)
        }
        .onAppear(perform: reload)
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
                Button { playing = game } label: { label }
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
        .contextMenu {
            Button { playing = game } label: { Label("실행", systemImage: "play.fill") }
            Button { toggleFavorite(game) } label: {
                Label(favorite ? "즐겨찾기 해제" : "즐겨찾기", systemImage: favorite ? "star.slash" : "star")
            }
            Divider()
            Button { exportSave(game) } label: { Label("세이브 내보내기", systemImage: "square.and.arrow.up") }
            Button { savesFor = game } label: { Label("세이브 불러오기", systemImage: "square.and.arrow.down") }
            Button(role: .destructive) { erasing = game } label: { Label("세이브 삭제", systemImage: "clock.arrow.circlepath") }
            Divider()
            Button(role: .destructive) {
                Library.delete(game)
                reload()
            } label: {
                Label("게임 삭제", systemImage: "trash")
            }
        }
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
        ToolbarItem(placement: .navigationBarTrailing) {
            Menu {
                Button { importer = .games } label: { Label("게임 가져오기", systemImage: "plus") }
                Button { importer = .save } label: { Label("세이브 가져오기", systemImage: "square.and.arrow.down") }
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
                confirmingDelete = true
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

    private func reload() {
        games = Library.games()
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

    private func handleImport(_ result: Result<[URL], Error>, target: ImportTarget?) {
        let urls: [URL]
        switch result {
        case .success(let picked):
            urls = picked
        case .failure(let error):
            notice = Notice(title: "가져오기 실패", message: error.localizedDescription)
            return
        }

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
        case .games, nil:
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
            importer = .save
        }
    }

    private func eraseSave(_ game: GameFile) {
        erasing = nil
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

    var body: some View {
        let (text, color) = Self.style(carrier)
        Text(text)
            .font(.system(size: 11, weight: .bold))
            .foregroundColor(color)
            .frame(width: 38, height: 20)
            .background(RoundedRectangle(cornerRadius: 5).fill(color.opacity(0.14)))
            .overlay(RoundedRectangle(cornerRadius: 5).stroke(color.opacity(0.35), lineWidth: 1))
    }

    private static func style(_ carrier: String?) -> (String, Color) {
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
