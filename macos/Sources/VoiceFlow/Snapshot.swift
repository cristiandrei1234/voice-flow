import AppKit
import SwiftUI

/// `VoiceFlow --snapshot <folder>`: deseneaza interfata in fisiere PNG, cu date de proba, fara motor.
/// Pentru lucrul la design: vezi rezultatul fara sa deschizi aplicatia.
@MainActor
enum Snapshot {
    static var folder: String? {
        let args = CommandLine.arguments
        guard let i = args.firstIndex(of: "--snapshot"), i + 1 < args.count else { return nil }
        return args[i + 1]
    }

    static func run(into folder: String) {
        let dir = URL(fileURLWithPath: folder)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        let menu: (Engine) -> AnyView = { e in
            // In aplicatie, fereastra din bara de meniu are deja fundal de sticla de la sistem.
            AnyView(MenuContent().environment(e)
                .background { Rectangle().fill(.clear).liquidGlass(Rectangle()) }
                .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous)))
        }
        render(menu(sample(.idle)), to: dir.appendingPathComponent("menu.png"))
        render(menu(sample(.recording)), to: dir.appendingPathComponent("menu-recording.png"))
        render(menu(sample(.idle, permission: false)), to: dir.appendingPathComponent("menu-permission.png"))
        for tab in SettingsView.Tab.allCases {
            let view = SettingsView(initialTab: tab)
                .environment(sample(.idle, permission: tab != .permissions))
                .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
            render(view, to: dir.appendingPathComponent("settings-\(tab.rawValue).png"))
        }
        for extra in Features.sections {
            let view = SettingsView(initialSection: extra.id)
                .environment(sample(.idle))
                .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
            render(view, to: dir.appendingPathComponent("section-\(extra.id).png"))
        }
        let editing = SettingsView(
            initialTab: .dictionary,
            demoEditor: DictEditor(line: 12, expected: "Next.js", correct: "Next.js", aliases: ["next js", "nekst"], group: "Technical", newAlias: "")
        )
        .environment(sample(.idle))
        .clipShape(RoundedRectangle(cornerRadius: 26, style: .continuous))
        render(editing, to: dir.appendingPathComponent("settings-dictionary-edit.png"))
        let pill = PillView().environment(sample(.recording)).frame(width: 260, height: 110)
        render(pill, to: dir.appendingPathComponent("pill.png"))
    }

    private static func sample(_ state: DictationState, permission: Bool = true) -> Engine {
        let e = Engine()
        e.state = state
        e.model = "Parakeet Ro 110M + hotwords"
        e.microphone = "MacBook Pro Microphone"
        e.dictionaryTerms = 16
        e.configPath = "/Users/you/Library/Application Support/VoiceFlow/config.toml"
        e.accessibility = permission
        e.keyboardReady = permission
        e.settings = EngineSettings(
            engine: "parakeet-ro", modelsDir: "", dictionaryPath: "", hotkey: "fn, CtrlRight",
            hotkeyMode: "hold", threads: 4, hotwordsScore: 2, sounds: true, paste: true,
            cleanup: .init(enabled: true, url: "http://127.0.0.1:8080/v1/chat/completions",
                           model: "a", timeoutSecs: 30, manageServer: true, prompt: "")
        )
        e.models = [
            ModelInfo(id: "parakeet-ro", title: "Parakeet Ro 110M", languages: "Romanian", note: "Best for Romanian, very fast. No punctuation or capitalization.", sizeBytes: 136_000_000, installed: true, active: true, downloadable: false, downloading: false),
            ModelInfo(id: "parakeet-v3", title: "Parakeet v3", languages: "25 European languages", note: "Multilingual, with punctuation. Weaker on Romanian than the dedicated model.", sizeBytes: 670_000_000, installed: true, active: false, downloadable: true, downloading: false),
            ModelInfo(id: "whisper-turbo", title: "Whisper Turbo", languages: "99 languages, including Romanian", note: "With punctuation and capitalization. Slower; up to 30 seconds per dictation.", sizeBytes: 1_036_000_000, installed: false, active: false, downloadable: true, downloading: true),
            ModelInfo(id: "whisper-small", title: "Whisper Small", languages: "99 languages, including Romanian", note: "Small and fast, less accurate. Up to 30 seconds per dictation.", sizeBytes: 375_000_000, installed: false, active: false, downloadable: true, downloading: false),
        ]
        e.downloadProgress = ["whisper-turbo": 0.62]
        e.dictGroups = ["Technical", "Business / projects", "Places"]
        e.dictEntries = [
            DictEntry(line: 11, correct: "NestJS", aliases: ["esges", "ensges", "nes ges", "nest ges", "nest js"], group: "Technical"),
            DictEntry(line: 12, correct: "Next.js", aliases: ["next js", "nekst"], group: "Technical"),
            DictEntry(line: 13, correct: "React", aliases: [], group: "Technical"),
            DictEntry(line: 20, correct: "Google Ads", aliases: ["google adses", "google ladesese", "google ladese", "google eds", "googlese"], group: "Business / projects"),
            DictEntry(line: 21, correct: "Acme", aliases: ["acmi", "ac me"], group: "Business / projects"),
            DictEntry(line: 27, correct: "Castellón", aliases: ["castellon", "castelon"], group: "Places"),
        ]
        e.llmModels = [
            LlmInfo(repo: "mlx-community/Qwen3-4B-4bit", model: "a", title: "Qwen3 4B", note: "Small and fast: adds about half a second. Good for punctuation and hesitations.", sizeBytes: 2_280_000_000, installed: true, active: true, downloading: false, deletable: true),
            LlmInfo(repo: "mlx-community/gemma-3-4b-it-qat-4bit", model: "b", title: "Gemma 3 4B", note: "Small, with good Romanian. An alternative to Qwen3 4B.", sizeBytes: 3_030_000_000, installed: false, active: false, downloading: true, deletable: false),
            LlmInfo(repo: "divinetribe/Hermes-4-14B-abliterated-4bit-mlx", model: "c", title: "Hermes 4 14B", note: "Best at corrections, but uses ~9 GB of memory and adds 1–2 seconds.", sizeBytes: 8_320_000_000, installed: true, active: false, downloading: false, deletable: false),
        ]
        e.llmProgress = ["mlx-community/gemma-3-4b-it-qat-4bit": 0.35]
        e.cleanupServerState = "ready"
        if permission {
            e.lastText = "Salut, voiam să zic că am făcut deploy pe staging aseară și totul merge bine."
            e.lastTiming = "5.2s speech · transcribed in 0.11s"
        }
        if state == .recording {
            e.bars = (0..<Engine.barCount).map { i in
                let d = abs(Double(i) - Double(Engine.barCount / 2))
                return max(0.12, 0.95 - d * 0.14) * (i % 2 == 0 ? 1 : 0.7)
            }
        }
        return e
    }

    /// Pune vederea intr-o fereastra reala, peste un fundal colorat, si fotografiaza zona de ecran.
    /// Doar asa se vede Liquid Glass adevarat (sticla e desenata de sistem, din ce e in spatele ferestrei).
    private static func render<V: View>(_ view: V, to url: URL) {
        guard let screen = NSScreen.main else { return }
        let host = NSHostingView(rootView: view)
        let size = host.fittingSize
        let margin: CGFloat = 60
        let canvas = NSRect(
            x: screen.frame.midX - size.width / 2 - margin, y: screen.frame.midY - size.height / 2 - margin,
            width: size.width + margin * 2, height: size.height + margin * 2
        )

        let backdrop = NSWindow(contentRect: canvas, styleMask: [.borderless], backing: .buffered, defer: false)
        backdrop.level = .floating
        backdrop.contentView = NSHostingView(rootView: Backdrop())
        backdrop.orderFrontRegardless()

        let window = NSWindow(
            contentRect: canvas.insetBy(dx: margin, dy: margin),
            styleMask: [.borderless], backing: .buffered, defer: false
        )
        window.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = true
        window.contentView = host
        window.orderFrontRegardless()
        RunLoop.main.run(until: Date().addingTimeInterval(1.2))

        // Coordonatele de captura au originea sus-stanga, pe ecranul principal.
        let top = (NSScreen.screens.first?.frame.maxY ?? screen.frame.maxY) - canvas.maxY
        let rect = CGRect(x: canvas.minX, y: top, width: canvas.width, height: canvas.height)
        if let image = capture(rect) {
            let rep = NSBitmapImageRep(cgImage: image)
            try? rep.representation(using: .png, properties: [:])?.write(to: url)
        }
        window.orderOut(nil)
        backdrop.orderOut(nil)
    }

    /// Captura unei zone de ecran. Fara permisiunea Screen Recording apar doar ferestrele noastre, ceea ce ne ajunge.
    private static func capture(_ rect: CGRect) -> CGImage? {
        typealias Fn = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
        guard let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "CGWindowListCreateImage") else { return nil }
        let fn = unsafeBitCast(sym, to: Fn.self)
        // optionOnScreenOnly = 1, fara fereastra relativa, imagine la rezolutia ecranului.
        return fn(rect, 1, 0, 0)?.takeRetainedValue()
    }
}

/// Fundal de proba: un "tapet" colorat, ca sticla sa aiba ce refracta.
private struct Backdrop: View {
    var body: some View {
        ZStack {
            LinearGradient(colors: [Color(red: 0.05, green: 0.09, blue: 0.16), Color(red: 0.02, green: 0.03, blue: 0.05)],
                           startPoint: .top, endPoint: .bottom)
            Circle().fill(Color.teal.opacity(0.8)).frame(width: 380).blur(radius: 70).offset(x: -160, y: -140)
            Circle().fill(Color.indigo.opacity(0.9)).frame(width: 320).blur(radius: 80).offset(x: 170, y: 120)
            Circle().fill(Color.white.opacity(0.5)).frame(width: 120).blur(radius: 40).offset(x: 60, y: -200)
        }
    }
}
