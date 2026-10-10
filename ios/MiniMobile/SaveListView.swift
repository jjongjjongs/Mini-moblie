import SwiftUI

/// One save zip in Documents/Saves.
struct SaveZip: Identifiable {
    /// What an export taken just before an import is marked with.
    static let beforeImport = " (가져오기 전)"

    let url: URL
    let modified: Date
    let files: Int
    let bytes: Int64
    /// Whether it holds the game's own saves rather than another's.
    let ours: Bool

    var id: String { url.path }
    var name: String { url.deletingPathExtension().lastPathComponent }

    /// Whether it is the copy taken by itself just before an import.
    var isBeforeImport: Bool { name.hasSuffix(Self.beforeImport) }

    /// The title it was named for: what comes before " 세이브" (or the older
    /// "_세이브_"), else the whole name.
    var title: String {
        for marker in [" 세이브", "_세이브"] {
            if let range = name.range(of: marker, options: .backwards), range.lowerBound > name.startIndex {
                return String(name[..<range.lowerBound])
            }
        }
        return name
    }

    /// 10월 10일 (토) 09:02
    static func day(_ date: Date) -> String {
        dayFormatter.string(from: date)
    }

    /// 2026-10-10 09:02
    static func stamp(_ date: Date) -> String {
        stampFormatter.string(from: date)
    }

    /// 방금 전, 5분 전, 9시간 전, 어제, 12일 전, 1년 전.
    static func ago(_ date: Date) -> String {
        let elapsed = max(0, Date().timeIntervalSince(date))
        if elapsed < 60 {
            return "방금 전"
        }
        if elapsed < 3600 {
            return "\(Int(elapsed / 60))분 전"
        }
        if elapsed < 86400 {
            return "\(Int(elapsed / 3600))시간 전"
        }
        let days = Int(elapsed / 86400)
        if days == 1 {
            return "어제"
        }
        return days < 365 ? "\(days)일 전" : "\(days / 365)년 전"
    }

    /// 48 KB, 1.2 MB
    static func size(_ bytes: Int64) -> String {
        if bytes >= 1024 * 1024 {
            return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
        }
        return String(format: "%.0f KB", max(1, Double(bytes) / 1024))
    }

    private static let dayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "ko_KR")
        formatter.dateFormat = "M월 d일 (E) HH:mm"
        return formatter
    }()

    private static let stampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy-MM-dd HH:mm"
        return formatter
    }()
}

/// 세이브 불러오기 for one game: the exported saves to pick one from - the
/// game's own first, newest first and by when each was taken, then other
/// games' - and, by a long press or a swipe, to share or remove one. A save
/// kept anywhere else is still reached through the file picker at the foot.
struct SaveListView: View {
    let game: GameFile
    /// A save was put in place; what to tell the player once the list is gone.
    let finished: (String) -> Void
    /// The player asked for the file picker instead.
    let pickElsewhere: () -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var saves: [SaveZip] = []
    @State private var loaded = false
    @State private var confirming: SaveZip?
    @State private var deleting: SaveZip?
    @State private var sharing: SharedFile?
    @State private var failure: String?

    private static let green = Color(red: 46 / 255, green: 139 / 255, blue: 87 / 255)
    private static let amber = Color(red: 154 / 255, green: 116 / 255, blue: 0)

    var body: some View {
        let ours = saves.filter(\.ours)
        let others = saves.filter { !$0.ours }
        let latest = ours.first(where: { !$0.isBeforeImport })?.id

        NavigationView {
            List {
                Section {
                    if loaded && saves.isEmpty {
                        Text("아직 꺼낸 세이브가 없어요.\n게임을 길게 눌러 ‘세이브 내보내기’로 만들 수 있어요.")
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                    }
                } header: {
                    Text("\(game.title) · 파일 앱 › Mini Mobile › Saves")
                        .textCase(nil)
                }
                if !ours.isEmpty {
                    Section(header: Text("이 게임의 세이브 · \(ours.count)개").foregroundColor(Self.green)) {
                        ForEach(ours) { row($0, latest: $0.id == latest) }
                    }
                }
                if !others.isEmpty {
                    Section(header: Text("다른 게임의 세이브 · \(others.count)개")) {
                        ForEach(others) { row($0, latest: false) }
                    }
                }
                Section {
                    Button {
                        pickElsewhere()
                        dismiss()
                    } label: {
                        Label("다른 위치에서 찾기…", systemImage: "folder")
                            .font(.body.weight(.semibold))
                    }
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("세이브 불러오기")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("취소") { dismiss() }
                }
            }
        }
        .navigationViewStyle(.stack)
        .onAppear(perform: reload)
        .alert(
            "이 세이브를 불러올까요?",
            isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
            presenting: confirming
        ) { zip in
            Button("취소", role: .cancel) {}
            Button("불러오기") { restore(zip) }
        } message: { zip in
            Text(confirmation(zip))
        }
        .confirmationDialog(
            "이 세이브 파일을 지울까요?",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible,
            presenting: deleting
        ) { zip in
            Button("지우기", role: .destructive) { delete(zip) }
        } message: { zip in
            Text("\(zip.name).zip\n지운 파일은 되돌릴 수 없습니다.")
        }
        .alert(
            "세이브",
            isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })
        ) {
            Button("확인", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
        .sheet(item: $sharing) { file in
            ShareSheet(items: [file.url])
        }
    }

    private func row(_ zip: SaveZip, latest: Bool) -> some View {
        Button {
            confirming = zip
        } label: {
            HStack(spacing: 12) {
                Text(zip.isBeforeImport ? "🛟" : "📦")
                    .font(.system(size: 20))
                    .frame(width: 40, height: 40)
                    .background(
                        RoundedRectangle(cornerRadius: 11)
                            .fill(zip.isBeforeImport ? Color.yellow.opacity(0.2) : Self.green.opacity(0.12))
                    )
                    .opacity(zip.ours ? 1 : 0.6)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(zip.ours ? SaveZip.day(zip.modified) : zip.title)
                            .font(.body.weight(zip.ours ? .bold : .regular))
                            .foregroundColor(zip.ours ? .primary : .secondary)
                            .lineLimit(1)
                        if latest {
                            tag("최신", color: Self.green)
                        } else if zip.isBeforeImport {
                            tag("가져오기 전", color: Self.amber)
                        }
                    }
                    Text(subtitle(zip))
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 4)
                Text(SaveZip.ago(zip.modified))
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(latest ? Self.green.opacity(0.08) : nil)
        .contextMenu {
            Button { confirming = zip } label: { Label("불러오기", systemImage: "square.and.arrow.down") }
            Button { sharing = SharedFile(url: zip.url) } label: { Label("공유하기", systemImage: "square.and.arrow.up") }
            Button(role: .destructive) { deleting = zip } label: { Label("이 세이브 파일 지우기", systemImage: "trash") }
        }
        .swipeActions(edge: .trailing) {
            Button(role: .destructive) { deleting = zip } label: { Label("지우기", systemImage: "trash") }
            Button { sharing = SharedFile(url: zip.url) } label: { Label("공유", systemImage: "square.and.arrow.up") }
                .tint(.blue)
        }
    }

    private func tag(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.system(size: 10.5, weight: .bold))
            .foregroundColor(color)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Capsule().fill(color.opacity(0.15)))
    }

    private func subtitle(_ zip: SaveZip) -> String {
        if !zip.ours {
            return SaveZip.day(zip.modified)
        }
        if zip.isBeforeImport {
            return "불러오기 직전에 자동으로 남긴 백업"
        }
        return "\(SaveZip.stamp(zip.modified)) · 파일 \(zip.files)개 · \(SaveZip.size(zip.bytes))"
    }

    private func confirmation(_ zip: SaveZip) -> String {
        let what = zip.ours ? SaveZip.day(zip.modified) : "\(zip.title) · \(SaveZip.day(zip.modified))"
        let detail = "파일 \(zip.files)개 · \(SaveZip.size(zip.bytes)) · \(SaveZip.ago(zip.modified))"
        if zip.ours {
            return "\(what)\n\(detail)\n\n지금 저장된 내용을 이 세이브로 바꿉니다.\n🛟 지금 세이브는 ‘가져오기 전’ 백업으로 목록에 남겨둬서, 언제든 다시 되돌릴 수 있어요."
        }
        return "\(what)\n\(detail)\n\n‘\(zip.title)’ 게임의 세이브예요. 그 게임에 저장된 내용을 이 세이브로 바꿉니다.\n되돌릴 수 없으니, 필요하면 그 게임에서 먼저 세이브를 꺼내 두세요."
    }

    private func reload() {
        saves = Library.saves(for: game)
        loaded = true
    }

    private func restore(_ zip: SaveZip) {
        do {
            let (restored, backup) = try Library.importSave(zip, into: game)
            finished("파일 \(restored)개를 복원했습니다. 게임을 다시 시작하면 적용됩니다."
                + (backup == nil ? "" : "\n이전 세이브는 ‘가져오기 전’으로 남겨뒀어요."))
            dismiss()
        } catch {
            failure = error.localizedDescription
        }
    }

    private func delete(_ zip: SaveZip) {
        do {
            try Library.deleteSave(zip)
        } catch {
            failure = error.localizedDescription
        }
        reload()
    }
}
