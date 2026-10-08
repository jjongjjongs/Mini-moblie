import SwiftUI
import UniformTypeIdentifiers

struct LibraryView: View {
    @State private var games: [GameFile] = []
    @State private var importing = false
    @State private var playing: GameFile?
    @State private var importError: String?

    var body: some View {
        NavigationView {
            List {
                if games.isEmpty {
                    Text("+ 버튼으로 게임 파일(zip, jar)을 가져오세요.")
                        .foregroundColor(.secondary)
                }
                ForEach(games) { game in
                    Button(game.title) { playing = game }
                }
                .onDelete { offsets in
                    offsets.map { games[$0] }.forEach(Library.delete)
                    reload()
                }
            }
            .navigationTitle("Mini Mobile")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button {
                        importing = true
                    } label: {
                        Image(systemName: "plus")
                    }
                }
            }
            .fileImporter(isPresented: $importing, allowedContentTypes: [.zip, .data, .item], allowsMultipleSelection: true) { result in
                switch result {
                case .success(let urls):
                    for url in urls {
                        do {
                            try Library.importGame(from: url)
                        } catch {
                            importError = "\(url.lastPathComponent): \(error.localizedDescription)"
                        }
                    }
                case .failure(let error):
                    importError = error.localizedDescription
                }
                reload()
            }
            .alert("가져오기 실패", isPresented: Binding(get: { importError != nil }, set: { if !$0 { importError = nil } })) {
                Button("확인", role: .cancel) {}
            } message: {
                Text(importError ?? "")
            }
            .onAppear(perform: reload)
        }
        .navigationViewStyle(.stack)
        .fullScreenCover(item: $playing) { game in
            GameView(game: game)
        }
    }

    private func reload() {
        games = Library.games()
    }
}
