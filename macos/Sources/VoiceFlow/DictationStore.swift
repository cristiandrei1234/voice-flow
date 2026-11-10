import AppKit
import Foundation
import Observation
import ServiceManagement

/// Starea sectiunii "Dictation": blocare cu apasare dubla, amplificarea soaptei, limba pentru Whisper
/// si pornirea la logare. Setarile vin de la motor (evenimentele `ready` / `settings`).
@MainActor
@Observable
final class DictationStore {
    static let shared = DictationStore()

    /// Limbile acceptate de motor pentru Whisper ("" = detectie automata).
    static let whisperLanguages: [(value: String, label: String)] = [
        ("", "Auto"), ("ro", "RO"), ("en", "EN"), ("es", "ES"),
        ("fr", "FR"), ("de", "DE"), ("it", "IT"), ("pt", "PT"),
    ]

    /// Starea elementului de logare, asa cum o raporteaza macOS.
    enum LoginItem: Equatable {
        case off, on, needsApproval, unavailable
    }

    var doubleTapLock = true
    var normalizeQuiet = true
    var whisperLanguage = "ro"
    /// Inregistrarea in curs e blocata (fara maini): pastila arata un punct rosu.
    var handsFree = false
    var loginItem: LoginItem = .off
    var loginError: String?

    @ObservationIgnored private weak var engine: Engine?
    @ObservationIgnored private var observer: NSObjectProtocol?

    func attach(_ engine: Engine) {
        self.engine = engine
        let readSettings: ([String: Any]) -> Void = { [weak self] obj in
            self?.read(obj["settings"] as? [String: Any])
        }
        engine.on("ready", readSettings)
        engine.on("settings", readSettings)
        engine.on("hands_free") { [weak self] obj in
            self?.handsFree = obj["active"] as? Bool ?? false
        }
        // Blocarea tine cat inregistrarea: orice alta stare o incheie.
        engine.on("state") { [weak self] obj in
            if obj["state"] as? String != "recording" { self?.handsFree = false }
        }
        refreshLoginItem()
        // Utilizatorul poate aproba / scoate aplicatia din System Settings: recitim cand revine.
        observer = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refreshLoginItem() }
        }
    }

    private func read(_ settings: [String: Any]?) {
        guard let settings else { return }
        if let lang = settings["whisper_language"] as? String { whisperLanguage = lang }
        if let d = settings["dictation"] as? [String: Any] {
            if let v = d["double_tap_lock"] as? Bool { doubleTapLock = v }
            if let v = d["normalize_quiet_audio"] as? Bool { normalizeQuiet = v }
        }
    }

    // MARK: Comenzi catre motor

    func setDoubleTapLock(_ on: Bool) {
        doubleTapLock = on
        engine?.send(["cmd": "set_double_tap_lock", "enabled": on])
    }

    func setNormalizeQuiet(_ on: Bool) {
        normalizeQuiet = on
        engine?.send(["cmd": "set_normalize_quiet", "enabled": on])
    }

    /// Motorul reporneste singur (reincarca modelul Whisper cu limba noua).
    func setWhisperLanguage(_ language: String) {
        guard language != whisperLanguage else { return }
        whisperLanguage = language
        engine?.send(["cmd": "set_whisper_language", "language": language])
    }

    // MARK: Pornire la logare

    func refreshLoginItem() {
        switch SMAppService.mainApp.status {
        case .enabled: loginItem = .on
        case .requiresApproval: loginItem = .needsApproval
        case .notRegistered: loginItem = .off
        case .notFound: loginItem = .unavailable
        @unknown default: loginItem = .unavailable
        }
    }

    func setLaunchAtLogin(_ on: Bool) {
        loginError = nil
        do {
            if on {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            // Cu `.requiresApproval` inregistrarea "esueaza", dar starea spune ce e de facut.
            if SMAppService.mainApp.status != .requiresApproval {
                loginError = on
                    ? "Couldn't add Voice Flow to login items: \(error.localizedDescription)"
                    : "Couldn't remove Voice Flow from login items: \(error.localizedDescription)"
            }
        }
        refreshLoginItem()
    }

    func openLoginItemsSettings() {
        SMAppService.openSystemSettingsLoginItems()
    }
}
