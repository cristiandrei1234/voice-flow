import AppKit
import SwiftUI

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let engine = Engine()
    private var pill: PillController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Aplicatie de bara de meniu (si LSUIElement in Info.plist): fara iconita in Dock.
        NSApp.setActivationPolicy(.accessory)
        if let folder = AppIconExport.folder {
            AppIconExport.run(into: folder)
            exit(0)
        }
        if let folder = Snapshot.folder {
            Snapshot.run(into: folder)
            exit(0)
        }
        // Daca motorul se opreste brusc, scrierea in pipe nu trebuie sa inchida aplicatia.
        signal(SIGPIPE, SIG_IGN)
        let pill = PillController(engine: engine)
        engine.onStateChange = { [weak pill] state in pill?.update(for: state) }
        self.pill = pill
        Features.install(engine: engine)
        engine.start()
    }

    func applicationWillTerminate(_ notification: Notification) {
        engine.quit()
    }
}

@main
struct VoiceFlowApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent()
                .environment(delegate.engine)
        } label: {
            MenuBarIcon(engine: delegate.engine)
        }
        .menuBarExtraStyle(.window)

        Window("Voice Flow", id: "settings") {
            SettingsView()
                .environment(delegate.engine)
        }
        .windowResizability(.contentSize)
        .defaultPosition(.center)
    }
}

/// Iconita din bara de meniu: forma de unda; plina si rosie cat timp asculta.
struct MenuBarIcon: View {
    let engine: Engine

    var body: some View {
        switch engine.state {
        case .recording:
            Image(systemName: "waveform.circle.fill")
                .symbolRenderingMode(.palette)
                .foregroundStyle(.white, .red)
        case .processing:
            Image(systemName: "ellipsis.circle")
        case .error:
            Image(systemName: "exclamationmark.triangle")
        default:
            Image(systemName: engine.keyboardReady ? "waveform" : "mic.slash")
        }
    }
}
