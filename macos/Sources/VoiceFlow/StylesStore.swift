import AppKit
import Foundation
import Observation

/// O aplicatie (nume + bundle id), asa cum o trimite si o primeste motorul.
struct StyleApp: Codable, Hashable, Identifiable {
    var bundleId: String
    var name: String

    var id: String { bundleId }
}

/// Un stil de corectare, cu aplicatiile legate de el.
struct DictationStyle: Codable, Equatable, Identifiable {
    var id: String
    var name: String
    var instruction: String
    var defaultInstruction: String
    var customized: Bool
    /// Stilul "Code": textul se lipeste fara corecturi.
    var skipCleanup: Bool
    var apps: [StyleApp]
}

/// Stilul primit de ultima dictare.
struct AppliedStyle: Equatable {
    var styleName: String
    var app: String?
    var cleaned: Bool
}

/// Stiluri: nivelul de corectare, stilul pe aplicatie si aplicatia din fata.
/// Motorul nu vede ce aplicatie e in fata, asa ca i-o spunem de aici la fiecare schimbare.
@MainActor
@Observable
final class StylesStore {
    static let shared = StylesStore()

    /// Voice Flow nu e niciodata tinta dictarii: deschiderea meniului nu schimba aplicatia retinuta.
    static let ownBundleId = "com.automwise.voiceflow"
    private static let maxSeen = 12

    var level = "medium"
    var enabled = true
    var styles: [DictationStyle] = []
    var recentApps: [StyleApp] = []
    var currentApp: StyleApp?
    var currentStyle = "default"
    var lastApplied: AppliedStyle?

    @ObservationIgnored private weak var engine: Engine?
    /// Aplicatiile vazute in fata, cea mai recenta ultima. Le retrimitem motorului dupa o repornire.
    @ObservationIgnored private var seen: [StyleApp] = []
    @ObservationIgnored private var observer: NSObjectProtocol?

    func attach(_ engine: Engine) {
        self.engine = engine
        engine.on("styles") { [weak self] obj in self?.receive(obj) }
        engine.on("style_applied") { [weak self] obj in
            self?.lastApplied = AppliedStyle(
                styleName: obj["style_name"] as? String ?? "",
                app: obj["app"] as? String,
                cleaned: obj["cleaned"] as? Bool ?? true
            )
        }
        // Motorul proaspat pornit nu stie nimic despre aplicatii.
        engine.on("ready") { [weak self] _ in self?.replay() }

        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
            let bundleId = app?.bundleIdentifier
            let name = app?.localizedName
            MainActor.assumeIsolated { self?.activated(bundleId: bundleId, name: name) }
        }
        let front = NSWorkspace.shared.frontmostApplication
        activated(bundleId: front?.bundleIdentifier, name: front?.localizedName)
    }

    // MARK: Aplicatia din fata

    private func isOwn(_ bundleId: String) -> Bool {
        bundleId.caseInsensitiveCompare(Self.ownBundleId) == .orderedSame || bundleId == Bundle.main.bundleIdentifier
    }

    private func activated(bundleId: String?, name: String?) {
        guard let bundleId, !bundleId.isEmpty, !isOwn(bundleId) else { return }
        let app = StyleApp(bundleId: bundleId, name: name ?? bundleId)
        seen.removeAll { $0.bundleId == bundleId }
        seen.append(app)
        if seen.count > Self.maxSeen { seen.removeFirst(seen.count - Self.maxSeen) }
        send(app)
    }

    private func send(_ app: StyleApp) {
        engine?.send(["cmd": "frontmost_app", "bundle_id": app.bundleId, "name": app.name])
    }

    private func replay() {
        for app in seen { send(app) }
    }

    // MARK: Evenimente

    private func receive(_ obj: [String: Any]) {
        level = obj["level"] as? String ?? level
        enabled = obj["enabled"] as? Bool ?? enabled
        currentStyle = obj["current_style"] as? String ?? "default"
        styles = decode([DictationStyle].self, obj["styles"]) ?? styles
        recentApps = decode([StyleApp].self, obj["recent_apps"]) ?? recentApps
        currentApp = decode(StyleApp.self, obj["current_app"])
    }

    private func decode<T: Decodable>(_ type: T.Type, _ any: Any?) -> T? {
        guard let any, !(any is NSNull), let data = try? JSONSerialization.data(withJSONObject: any) else { return nil }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(type, from: data)
    }

    // MARK: Comenzi

    func setLevel(_ level: String) {
        self.level = level
        engine?.send(["cmd": "set_cleanup_level", "level": level])
    }

    func setEnabled(_ on: Bool) {
        enabled = on
        engine?.send(["cmd": "set_styles_enabled", "enabled": on])
    }

    func saveInstruction(_ style: String, _ text: String) {
        engine?.send(["cmd": "save_style", "id": style, "instruction": text])
    }

    func resetInstruction(_ style: String) {
        engine?.send(["cmd": "save_style", "id": style])
    }

    func assign(_ app: StyleApp, to style: String) {
        engine?.send(["cmd": "assign_app", "bundle_id": app.bundleId, "name": app.name, "style": style])
    }

    func unassign(_ app: StyleApp) {
        engine?.send(["cmd": "unassign_app", "bundle_id": app.bundleId])
    }

    // MARK: Pentru ecran

    func styleName(_ id: String) -> String {
        styles.first { $0.id == id }?.name ?? "Default"
    }

    /// Aplicatiile care pot fi adaugate la un stil: cele vazute recent, apoi restul celor deschise acum.
    func candidates(for style: DictationStyle) -> [StyleApp] {
        let taken = Set(style.apps.map { $0.bundleId.lowercased() })
        var list = recentApps
        for running in NSWorkspace.shared.runningApplications where running.activationPolicy == .regular {
            guard let id = running.bundleIdentifier, !isOwn(id) else { continue }
            if !list.contains(where: { $0.bundleId == id }) {
                list.append(StyleApp(bundleId: id, name: running.localizedName ?? id))
            }
        }
        return list.filter { !taken.contains($0.bundleId.lowercased()) }
    }
}
