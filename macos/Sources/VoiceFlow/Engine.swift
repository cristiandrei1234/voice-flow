import AppKit
import Foundation
import Observation

/// Setarile, asa cum le trimite motorul Rust (config.toml).
struct EngineSettings: Codable, Equatable {
    struct Cleanup: Codable, Equatable {
        var enabled: Bool
        var url: String
        var model: String
        var timeoutSecs: Int
        var manageServer: Bool?
        var prompt: String
    }

    var engine: String
    var modelsDir: String
    var dictionaryPath: String
    var hotkey: String
    var hotkeyMode: String
    var threads: Int
    var hotwordsScore: Double
    var sounds: Bool
    var paste: Bool
    var cleanup: Cleanup

    /// Scurtaturile, separate prin virgula in config.
    var hotkeys: [String] {
        hotkey.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }
}

/// Un model de transcriere din catalog, cu starea lui de pe disc.
struct ModelInfo: Codable, Equatable, Identifiable {
    var id: String
    var title: String
    var languages: String
    var note: String
    var sizeBytes: Int64
    var installed: Bool
    var active: Bool
    var downloadable: Bool
    var downloading: Bool

    var sizeText: String {
        sizeBytes > 0 ? ByteCountFormatter.string(fromByteCount: sizeBytes, countStyle: .file) : ""
    }
}

/// Un termen din dictionarul personal, cu linia lui din fisier.
struct DictEntry: Codable, Equatable, Identifiable {
    var line: Int
    var correct: String
    var aliases: [String]
    var group: String?

    var id: String { "\(line)-\(correct)" }
}

/// Un model de corecturi (LLM local).
struct LlmInfo: Codable, Equatable, Identifiable {
    var repo: String
    var model: String
    var title: String
    var note: String
    var sizeBytes: Int64
    var installed: Bool
    var active: Bool
    var downloading: Bool
    var deletable: Bool

    var id: String { repo }
    var sizeText: String {
        sizeBytes > 0 ? ByteCountFormatter.string(fromByteCount: sizeBytes, countStyle: .file) : ""
    }
}

enum DictationState: String {
    case starting, idle, recording, processing, error
}

/// Legatura cu motorul Rust: porneste `voice-flow daemon`, trimite comenzi JSON pe stdin,
/// citeste evenimente JSON de pe stdout.
@Observable
@MainActor
final class Engine {
    static let barCount = 13

    var state: DictationState = .starting
    var message: String?
    var bars: [Double] = Array(repeating: 0, count: Engine.barCount)
    var settings: EngineSettings?
    var model: String = ""
    var microphone: String?
    var dictionaryTerms = 0
    var configPath: String = ""
    var accessibility = true
    var keyboardReady = false
    var lastText: String?
    var lastTiming: String?
    var isCapturingHotkey = false
    var capturedHotkey: String?
    var lastError: String?
    var models: [ModelInfo] = []
    /// Cat s-a descarcat din fiecare model (0...1), cat timp se descarca.
    var downloadProgress: [String: Double] = [:]
    var dictEntries: [DictEntry] = []
    var dictGroups: [String] = []
    /// Tabul pe care trebuie sa se deschida Setarile (cerut din meniu).
    var requestedSettingsTab: String?
    @ObservationIgnored private var dictionaryReload: DispatchWorkItem?
    /// Ascultatori pentru functiile optionale (vezi Features.swift), pe tip de eveniment.
    @ObservationIgnored private var listeners: [String: [([String: Any]) -> Void]] = [:]
    var llmModels: [LlmInfo] = []
    var llmProgress: [String: Double] = [:]
    /// Serverul de corecturi: stopped / starting / ready / external / error.
    var cleanupServerState = "stopped"
    var cleanupServerMessage: String?

    /// Apelat la fiecare schimbare de stare (pastila se afiseaza / ascunde).
    @ObservationIgnored var onStateChange: ((DictationState) -> Void)?

    @ObservationIgnored private var process: Process?
    @ObservationIgnored private var input: FileHandle?
    @ObservationIgnored private var buffer = Data()
    @ObservationIgnored private var quitting = false
    @ObservationIgnored private var history: [Double] = []
    @ObservationIgnored private var phase: Double = 0
    @ObservationIgnored private var animationTimer: Timer?

    // MARK: Pornire

    /// Unde gasim binarul Rust: in aplicatie (Contents/Resources), in variabila VOICEFLOW_DAEMON
    /// sau in folderul proiectului (pentru dezvoltare).
    private func daemonURL() -> URL? {
        if let env = ProcessInfo.processInfo.environment["VOICEFLOW_DAEMON"] {
            return URL(fileURLWithPath: env)
        }
        if let bundled = Bundle.main.url(forResource: "voice-flow", withExtension: nil) {
            return bundled
        }
        // In dezvoltare (swift run): motorul construit de cargo, cautat pornind de la folderul curent.
        var dir = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        for _ in 0..<4 {
            let candidate = dir.appendingPathComponent("target/release/voice-flow")
            if FileManager.default.isExecutableFile(atPath: candidate.path) { return candidate }
            dir = dir.deletingLastPathComponent()
        }
        return nil
    }

    func start() {
        guard process == nil else { return }
        guard let url = daemonURL() else {
            setState(.error, "Can't find the voice-flow engine. Run scripts/build-app.sh.")
            return
        }
        quitting = false
        let p = Process()
        p.executableURL = url
        p.arguments = ["daemon"]

        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        p.standardInput = stdinPipe
        p.standardOutput = stdoutPipe
        p.standardError = logFileHandle()

        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self?.receive(data)
                }
            }
        }
        p.terminationHandler = { [weak self] _ in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self?.processExited()
                }
            }
        }

        do {
            try p.run()
            process = p
            input = stdinPipe.fileHandleForWriting
            setState(.starting, "Loading model…")
        } catch {
            setState(.error, "Can't start the engine: \(error.localizedDescription)")
        }
    }

    func restart() {
        guard let p = process else {
            start()
            return
        }
        // processExited() il porneste din nou.
        p.terminate()
    }

    func quit() {
        quitting = true
        send(["cmd": "quit"])
        process?.terminate()
    }

    private func processExited() {
        process = nil
        input = nil
        buffer.removeAll()
        if quitting { return }
        keyboardReady = false
        setState(.starting, "Restarting engine…")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            MainActor.assumeIsolated { self?.start() }
        }
    }

    /// Logurile motorului: ~/Library/Logs/VoiceFlow.log
    private func logFileHandle() -> FileHandle? {
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Logs/VoiceFlow.log")
        if !FileManager.default.fileExists(atPath: url.path) {
            FileManager.default.createFile(atPath: url.path, contents: nil)
        }
        let h = try? FileHandle(forWritingTo: url)
        h?.seekToEndOfFile()
        return h
    }

    // MARK: Comenzi

    func send(_ command: [String: Any]) {
        guard let input, let data = try? JSONSerialization.data(withJSONObject: command) else { return }
        try? input.write(contentsOf: data + Data([0x0A]))
    }

    /// Asculta un tip de eveniment de la motor (ex. "ready", "result", sau unul nou al functiei tale).
    /// Ascultatorii sunt apelati pe firul principal, dupa tratarea obisnuita a evenimentului.
    func on(_ type: String, _ handler: @escaping ([String: Any]) -> Void) {
        listeners[type, default: []].append(handler)
    }

    func toggleDictation() { send(["cmd": "toggle"]) }
    func cancelDictation() { send(["cmd": "cancel"]) }

    func captureHotkey() {
        capturedHotkey = nil
        isCapturingHotkey = true
        send(["cmd": "record_hotkey"])
    }

    func setHotkeys(_ keys: [String]) {
        send(["cmd": "set_hotkey", "hotkey": keys.joined(separator: ", ")])
    }

    func setMode(_ mode: String) { send(["cmd": "set_mode", "mode": mode]) }
    func setEngine(_ engine: String) { send(["cmd": "set_engine", "engine": engine]) }
    func setCleanup(_ on: Bool) { send(["cmd": "set_cleanup", "enabled": on]) }
    func setSounds(_ on: Bool) { send(["cmd": "set_sounds", "enabled": on]) }
    func setPaste(_ on: Bool) { send(["cmd": "set_paste", "enabled": on]) }
    func reloadDictionary() { send(["cmd": "reload_dictionary"]) }
    func refreshModels() {
        send(["cmd": "list_models"])
        send(["cmd": "list_llm_models"])
    }
    /// Adauga (line == nil) sau modifica un termen din dictionar.
    func saveEntry(line: Int?, expected: String, correct: String, aliases: [String], group: String?) {
        var cmd: [String: Any] = ["cmd": "save_entry", "correct": correct, "aliases": aliases, "expected": expected]
        if let line { cmd["line"] = line }
        if let group, !group.isEmpty { cmd["group"] = group }
        send(cmd)
    }

    func deleteEntry(_ entry: DictEntry) {
        send(["cmd": "delete_entry", "line": entry.line, "expected": entry.correct])
    }

    /// Motorul citeste dictionarul la pornire: dupa ce te opresti din editat, il reincarcam o data.
    private func scheduleDictionaryReload() {
        dictionaryReload?.cancel()
        let work = DispatchWorkItem { [weak self] in
            MainActor.assumeIsolated { self?.reloadDictionary() }
        }
        dictionaryReload = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: work)
    }

    func downloadLlm(_ repo: String) {
        llmProgress[repo] = 0
        send(["cmd": "download_llm", "repo": repo])
    }
    func cancelLlmDownload(_ repo: String) { send(["cmd": "cancel_llm_download", "repo": repo]) }
    func deleteLlm(_ repo: String) { send(["cmd": "delete_llm", "repo": repo]) }
    func setCleanupModel(_ model: String) { send(["cmd": "set_cleanup_model", "model": model]) }
    func downloadModel(_ id: String) {
        downloadProgress[id] = 0
        send(["cmd": "download_model", "engine": id])
    }
    func cancelDownload(_ id: String) { send(["cmd": "cancel_download", "engine": id]) }
    func deleteModel(_ id: String) { send(["cmd": "delete_model", "engine": id]) }
    func requestPermission() { send(["cmd": "open_permission_settings"]) }

    // MARK: Evenimente

    private func receive(_ data: Data) {
        if data.isEmpty { return }
        buffer.append(data)
        while let newline = buffer.firstIndex(of: 0x0A) {
            let line = buffer.subdata(in: buffer.startIndex..<newline)
            buffer.removeSubrange(buffer.startIndex...newline)
            handle(line)
        }
    }

    private func handle(_ line: Data) {
        guard
            let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
            let type = obj["type"] as? String
        else { return }

        switch type {
        case "ready":
            decodeSettings(obj["settings"])
            model = obj["model"] as? String ?? ""
            microphone = obj["microphone"] as? String
            configPath = obj["config_path"] as? String ?? ""
            dictionaryTerms = obj["dictionary_terms"] as? Int ?? 0
        case "settings":
            decodeSettings(obj["settings"])
        case "permission":
            accessibility = obj["accessibility"] as? Bool ?? false
            keyboardReady = obj["keyboard"] as? Bool ?? false
        case "state":
            let st = DictationState(rawValue: obj["state"] as? String ?? "") ?? .idle
            setState(st, obj["message"] as? String)
        case "level":
            pushLevel(obj["value"] as? Double ?? 0)
        case "result":
            lastText = obj["text"] as? String
            let audio = (obj["audio_ms"] as? Double ?? 0) / 1000
            let asr = (obj["asr_ms"] as? Double ?? 0) / 1000
            lastTiming = String(format: "%.1fs speech · transcribed in %.2fs", audio, asr)
        case "hotkey_recorded":
            isCapturingHotkey = false
            capturedHotkey = obj["hotkey"] as? String
        case "dictionary":
            if let any = obj["entries"], let data = try? JSONSerialization.data(withJSONObject: any),
               let list = try? JSONDecoder().decode([DictEntry].self, from: data) {
                dictEntries = list
                dictionaryTerms = list.count
            }
            dictGroups = obj["groups"] as? [String] ?? []
            if obj["changed"] as? Bool == true { scheduleDictionaryReload() }
        case "llm_models":
            if let any = obj["models"], let data = try? JSONSerialization.data(withJSONObject: any) {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                if let list = try? decoder.decode([LlmInfo].self, from: data) {
                    llmModels = list
                    let active = Set(list.filter(\.downloading).map(\.repo))
                    llmProgress = llmProgress.filter { active.contains($0.key) }
                }
            }
        case "llm_progress":
            if let repo = obj["repo"] as? String, let total = obj["total"] as? Double, total > 0 {
                llmProgress[repo] = (obj["received"] as? Double ?? 0) / total
            }
        case "cleanup_server":
            cleanupServerState = obj["state"] as? String ?? "stopped"
            cleanupServerMessage = obj["message"] as? String
        case "microphone":
            microphone = obj["name"] as? String
        case "models":
            if let any = obj["models"], let data = try? JSONSerialization.data(withJSONObject: any) {
                let decoder = JSONDecoder()
                decoder.keyDecodingStrategy = .convertFromSnakeCase
                if let list = try? decoder.decode([ModelInfo].self, from: data) {
                    models = list
                    // Pastram progresul doar pentru ce inca se descarca.
                    let active = Set(list.filter(\.downloading).map(\.id))
                    downloadProgress = downloadProgress.filter { active.contains($0.key) }
                }
            }
        case "model_progress":
            if let id = obj["id"] as? String, let total = obj["total"] as? Double, total > 0 {
                downloadProgress[id] = (obj["received"] as? Double ?? 0) / total
            }
        case "restart_required":
            restart()
        case "error":
            lastError = obj["message"] as? String
        default:
            break
        }
        for listener in listeners[type] ?? [] { listener(obj) }
    }

    private func decodeSettings(_ any: Any?) {
        guard let any, let data = try? JSONSerialization.data(withJSONObject: any) else { return }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        if let s = try? decoder.decode(EngineSettings.self, from: data) {
            settings = s
        }
    }

    private func setState(_ new: DictationState, _ msg: String?) {
        let changed = new != state
        state = new
        message = msg
        if changed {
            if new == .recording { history.removeAll() }
            updateAnimation()
            onStateChange?(new)
        }
    }

    // MARK: Barele pastilei

    private func pushLevel(_ v: Double) {
        history.insert(v, at: 0)
        if history.count > Engine.barCount { history.removeLast() }
    }

    private func updateAnimation() {
        let animate = state == .recording || state == .processing
        if animate, animationTimer == nil {
            let t = Timer(timeInterval: 1.0 / 30.0, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.tick() }
            }
            RunLoop.main.add(t, forMode: .common)
            animationTimer = t
        } else if !animate {
            animationTimer?.invalidate()
            animationTimer = nil
            bars = Array(repeating: 0, count: Engine.barCount)
        }
    }

    private func tick() {
        let center = Double(Engine.barCount / 2)
        var out = Array(repeating: 0.0, count: Engine.barCount)
        if state == .recording {
            phase += 0.9
            for i in 0..<Engine.barCount {
                let d = abs(Double(i) - center)
                let idx = Int(d)
                let h = idx < history.count ? history[idx] : 0
                let jitter = 0.85 + 0.15 * sin(phase + Double(i) * 1.7)
                out[i] = h * (1 - d * 0.05) * jitter
            }
        } else if state == .processing {
            phase += 0.35
            let span = Double(Engine.barCount) + 4
            let pos = phase.truncatingRemainder(dividingBy: span) - 2
            for i in 0..<Engine.barCount {
                out[i] = 0.28 * max(0, 1 - abs(Double(i) - pos) / 1.8)
            }
        }
        bars = out
    }
}

/// O bucata dintr-o scurtatura: simbol SF (⌘, ⌃, glob) si / sau text ("right", "F13").
struct KeyPart: Hashable {
    var symbol: String?
    var text: String?
}

/// Afisarea unei scurtaturi: "CmdRight" -> ⌘ right, "Ctrl+Opt" -> ⌃ ⌥, "fn" -> glob fn.
enum HotkeyFormat {
    static func parts(_ hotkey: String) -> [KeyPart] {
        hotkey.split(separator: "+").map { token in
            let t = token.lowercased()
                .replacingOccurrences(of: "_", with: "")
                .replacingOccurrences(of: "-", with: "")
            // Modificatori cu parte: "cmdright", "rightcommand", "rcmd", "ralt"...
            let sides: [(String, String)] = [("right", "right"), ("left", "left")]
            for (side, label) in sides {
                let short = String(side.prefix(1))
                for (names, symbol) in modifiers {
                    for n in names where t == n + side || t == side + n || t == short + n {
                        return KeyPart(symbol: symbol, text: label)
                    }
                }
            }
            for (names, symbol) in modifiers where names.contains(t) {
                return KeyPart(symbol: symbol, text: nil)
            }
            switch t {
            case "fn", "function", "globe": return KeyPart(symbol: "globe", text: "fn")
            case "space": return KeyPart(symbol: "space", text: nil)
            case "escape", "esc": return KeyPart(symbol: "escape", text: nil)
            case "return", "enter": return KeyPart(symbol: "return", text: nil)
            case "tab": return KeyPart(symbol: "arrow.right.to.line", text: nil)
            case "delete", "backspace": return KeyPart(symbol: "delete.left", text: nil)
            default:
                return KeyPart(symbol: nil, text: token.count <= 3 ? token.uppercased() : token.capitalized)
            }
        }
    }

    private static let modifiers: [([String], String)] = [
        (["cmd", "command", "meta"], "command"),
        (["opt", "option", "alt"], "option"),
        (["ctrl", "control"], "control"),
        (["shift"], "shift"),
    ]

    /// Text simplu, pentru accesibilitate si tooltip.
    static func text(_ hotkey: String) -> String {
        let names = ["command": "Command", "option": "Option", "control": "Control", "shift": "Shift",
                     "globe": "", "space": "Space", "escape": "Esc", "return": "Return",
                     "arrow.right.to.line": "Tab", "delete.left": "Delete"]
        return parts(hotkey).map { p in
            [p.symbol.flatMap { names[$0] }, p.text].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " ")
        }.joined(separator: " + ")
    }
}
