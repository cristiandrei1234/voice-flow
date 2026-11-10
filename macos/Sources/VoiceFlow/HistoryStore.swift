import AppKit
import Foundation
import Observation

/// O dictare din istoric (o linie din history.jsonl).
struct HistoryItem: Identifiable, Equatable, Decodable {
    var id: String
    /// Secunde Unix.
    var ts: Double
    var text: String
    var raw: String
    var audioMs: Double
    var asrMs: Double
    var cleanupMs: Double?
    var words: Int
    /// Aplicatia in care s-a lipit textul, cand motorul o stie.
    var app: String?

    var date: Date { Date(timeIntervalSince1970: ts) }

    /// Textul de dinainte de corecturi merita aratat doar daca e altul decat cel final.
    var hasDifferentRaw: Bool {
        let r = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        return !r.isEmpty && r != text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// Statisticile calculate de motor din istoric (evenimentul `history_stats`).
struct HistoryStats: Decodable {
    struct Period: Decodable {
        var dictations: Int
        var words: Int
    }

    struct Day: Decodable, Identifiable {
        /// Inceputul zilei (miezul noptii local), secunde Unix.
        var start: Double
        var words: Int
        var dictations: Int

        var id: Double { start }
        var date: Date { Date(timeIntervalSince1970: start) }
    }

    var today: Period
    var week: Period
    var total: Period
    var audioMs: Double
    var wpm: Double
    var savedMs: Double
    var streakDays: Int
    var days: [Day]
    var path: String
}

/// Starea ecranului "History": lista paginata, cautarea, statisticile si setarile istoricului.
@MainActor
@Observable
final class HistoryStore {
    static let shared = HistoryStore()
    static let pageSize = 30

    var items: [HistoryItem] = []
    var hasMore = false
    /// Ce e scris in campul de cautare (cererea pleaca dupa o scurta pauza).
    var searchText = ""
    /// Cautarea pentru care e valabila lista de acum.
    var query = ""
    var stats: HistoryStats?
    var enabled = true
    var retentionDays = 90
    /// A sosit macar un raspuns de la motor (pana atunci nu aratam "gol").
    var loaded = false
    var loadingMore = false

    @ObservationIgnored private weak var engine: Engine?
    /// Cererile de lista inca fara raspuns, in ordine: `true` = pagina urmatoare (se adauga la lista).
    @ObservationIgnored private var pending: [Bool] = []
    @ObservationIgnored private var searchWork: DispatchWorkItem?

    func attach(_ engine: Engine) {
        self.engine = engine
        engine.on("ready") { [weak self] obj in
            guard let self else { return }
            // Motor nou: raspunsurile cerute celui vechi nu mai vin.
            self.pending.removeAll()
            self.loadingMore = false
            self.readSettings(obj)
            self.reload()
            self.refreshStats()
        }
        engine.on("settings") { [weak self] obj in self?.readSettings(obj) }
        engine.on("history") { [weak self] obj in self?.receivePage(obj) }
        engine.on("history_stats") { [weak self] obj in
            if let s: HistoryStats = Self.decode(obj) { self?.stats = s }
        }
        engine.on("history_added") { [weak self] obj in
            guard let self, let item: HistoryItem = Self.decode(obj["item"]) else { return }
            if self.query.isEmpty {
                if !self.items.contains(where: { $0.id == item.id }) { self.items.insert(item, at: 0) }
            } else {
                self.reload()
            }
            self.refreshStats()
        }
    }

    /// Unde e fisierul cu istoricul (spus de motor; pana atunci, dedus din locul setarilor).
    var filePath: String {
        if let p = stats?.path, !p.isEmpty { return p }
        guard let config = engine?.configPath, !config.isEmpty else { return "" }
        return URL(fileURLWithPath: config).deletingLastPathComponent().appendingPathComponent("history.jsonl").path
    }

    // MARK: Comenzi

    /// Prima pagina pentru cautarea curenta.
    func reload() {
        pending.append(false)
        var cmd: [String: Any] = ["cmd": "list_history", "limit": Self.pageSize]
        if !query.isEmpty { cmd["query"] = query }
        engine?.send(cmd)
    }

    func loadMore() {
        guard hasMore, !loadingMore, let last = items.last else { return }
        loadingMore = true
        pending.append(true)
        var cmd: [String: Any] = ["cmd": "list_history", "limit": Self.pageSize, "before": last.id]
        if !query.isEmpty { cmd["query"] = query }
        engine?.send(cmd)
    }

    func refreshStats() {
        engine?.send(["cmd": "get_stats", "utc_offset": TimeZone.current.secondsFromGMT()])
    }

    func setSearch(_ text: String) {
        searchText = text
        searchWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated {
                guard let self else { return }
                let q = text.trimmingCharacters(in: .whitespacesAndNewlines)
                if q != self.query {
                    self.query = q
                    self.reload()
                }
            }
        }
        searchWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + (text.isEmpty ? 0 : 0.25), execute: work)
    }

    func delete(_ item: HistoryItem) {
        items.removeAll { $0.id == item.id }
        engine?.send(["cmd": "delete_history", "id": item.id])
        refreshStats()
    }

    func clear() {
        engine?.send(["cmd": "clear_history"])
        reload()
        refreshStats()
    }

    func setEnabled(_ on: Bool) {
        enabled = on
        engine?.send(["cmd": "set_history_enabled", "enabled": on])
    }

    func setRetention(_ days: Int) {
        retentionDays = days
        engine?.send(["cmd": "set_history_retention", "days": days])
    }

    func copy(_ item: HistoryItem) {
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(item.text, forType: .string)
    }

    func revealInFinder() {
        let path = filePath
        guard !path.isEmpty else { return }
        let url = URL(fileURLWithPath: path)
        if FileManager.default.fileExists(atPath: path) {
            NSWorkspace.shared.activateFileViewerSelecting([url])
        } else {
            // Fisierul apare la prima dictare; pana atunci aratam folderul.
            NSWorkspace.shared.open(url.deletingLastPathComponent())
        }
    }

    // MARK: Evenimente

    private func receivePage(_ obj: [String: Any]) {
        let append = pending.isEmpty ? false : pending.removeFirst()
        if append { loadingMore = false }
        // Raspuns la o cautare intre timp schimbata: il sarim, vine si cel bun.
        guard (obj["query"] as? String ?? "") == query else { return }
        let page: [HistoryItem] = Self.decode(obj["items"]) ?? []
        if append {
            let known = Set(items.map(\.id))
            items.append(contentsOf: page.filter { !known.contains($0.id) })
        } else {
            items = page
        }
        hasMore = obj["has_more"] as? Bool ?? false
        loaded = true
    }

    private func readSettings(_ obj: [String: Any]) {
        guard let settings = obj["settings"] as? [String: Any], let h = settings["history"] as? [String: Any] else { return }
        enabled = h["enabled"] as? Bool ?? true
        retentionDays = h["retention_days"] as? Int ?? 90
    }

    private static func decode<T: Decodable>(_ any: Any?) -> T? {
        guard let any, let data = try? JSONSerialization.data(withJSONObject: any) else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(T.self, from: data)
    }
}
