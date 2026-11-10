import Foundation
import Observation

/// Rezultatul ultimei comenzi: ce s-a cerut, textul selectat si ce a ajuns in locul lui.
struct CommandResult: Equatable {
    var instruction: String
    var before: String
    var after: String
}

/// Starea Command Mode: selectezi text, ții apăsată a doua scurtătură, spui ce vrei
/// si modelul local inlocuieste selectia. Setarile stau in motor (config.toml, sectiunea [command]).
@MainActor
@Observable
final class CommandModeStore {
    static let shared = CommandModeStore()

    /// Exemple de instructiuni, aratate in ecran.
    static let examples = ["make it shorter", "translate to Spanish", "make it more polite"]

    var enabled = false
    var hotkey = "fn+ctrl"
    /// Promptul folosit de motor (cel implicit daca utilizatorul nu l-a schimbat).
    var prompt = ""
    var promptIsDefault = true
    /// De ce scurtatura nu e activa (ex. e aceeasi cu o tasta de dictare).
    var problem: String?
    var isCapturingHotkey = false
    /// Eroarea ultimei schimbari de scurtatura.
    var hotkeyError: String?
    var lastResult: CommandResult?
    /// De ce nu a mers ultima comanda (nimic selectat, modelul nu raspunde...).
    var lastFailure: String?

    @ObservationIgnored private weak var engine: Engine?
    @ObservationIgnored private var awaitingHotkeyAnswer = false

    func attach(_ engine: Engine) {
        self.engine = engine
        engine.on("command_mode") { [weak self] obj in
            guard let self else { return }
            self.enabled = obj["enabled"] as? Bool ?? false
            self.hotkey = obj["hotkey"] as? String ?? self.hotkey
            self.prompt = obj["prompt"] as? String ?? ""
            self.promptIsDefault = obj["prompt_is_default"] as? Bool ?? true
            self.problem = obj["problem"] as? String
            self.awaitingHotkeyAnswer = false
        }
        engine.on("command_hotkey_recorded") { [weak self] obj in
            guard let self else { return }
            self.isCapturingHotkey = false
            if let key = obj["hotkey"] as? String { self.setHotkey(key) }
        }
        engine.on("command_result") { [weak self] obj in
            guard let self else { return }
            self.lastFailure = nil
            self.lastResult = CommandResult(
                instruction: obj["instruction"] as? String ?? "",
                before: obj["before"] as? String ?? "",
                after: obj["after"] as? String ?? ""
            )
        }
        engine.on("command_failed") { [weak self] obj in
            self?.lastFailure = obj["message"] as? String
        }
        // Motorul raspunde la o scurtatura respinsa cu `error`, apoi cu `command_mode`.
        engine.on("error") { [weak self] obj in
            guard let self, self.awaitingHotkeyAnswer else { return }
            self.hotkeyError = obj["message"] as? String
        }
    }

    func setEnabled(_ on: Bool) {
        enabled = on
        engine?.send(["cmd": "set_command_enabled", "enabled": on])
    }

    /// Asteapta urmatoarea combinatie apasata. Foloseste comenzi separate de cele ale dictarii,
    /// ca tasta prinsa sa nu fie adaugata la tastele de dictare.
    func captureHotkey() {
        guard !isCapturingHotkey else { return }
        hotkeyError = nil
        isCapturingHotkey = true
        engine?.send(["cmd": "record_command_hotkey"])
    }

    func setHotkey(_ key: String) {
        hotkeyError = nil
        awaitingHotkeyAnswer = true
        engine?.send(["cmd": "set_command_hotkey", "hotkey": key])
    }

    func savePrompt(_ text: String) {
        engine?.send(["cmd": "set_command_prompt", "prompt": text])
    }

    /// Un prompt gol inseamna, pentru motor, "cel implicit".
    func resetPrompt() { savePrompt("") }

    func openCleanupSection() { engine?.requestedSettingsTab = "cleanup" }

    /// Serverul modelului local, asa cum il vede sectiunea Corrections.
    enum ModelStatus: Equatable {
        case ready
        case starting
        /// Comenzile nu pot merge; textul spune de ce.
        case unavailable(String)
    }

    var modelStatus: ModelStatus {
        guard let engine else { return .ready }
        switch engine.cleanupServerState {
        case "ready", "external":
            return .ready
        case "starting":
            return .starting
        case "error":
            return .unavailable(engine.cleanupServerMessage ?? "The corrections model server didn't start.")
        default:
            if !engine.llmModels.isEmpty, !engine.llmModels.contains(where: { $0.installed }) {
                return .unavailable("Commands use the corrections model, and no model has been downloaded yet.")
            }
            if let active = engine.llmModels.first(where: { $0.active }), !active.installed {
                return .unavailable("The selected corrections model (\(active.title)) isn't downloaded.")
            }
            // Oprit e normal cat timp comenzile sunt dezactivate: motorul il porneste la activare.
            return enabled ? .unavailable("The corrections model server is stopped.") : .ready
        }
    }
}
