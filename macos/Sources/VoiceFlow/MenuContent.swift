import AppKit
import SwiftUI

/// Fereastra din bara de meniu, in Liquid Glass: sticla fumurie, carduri si butoane de sticla,
/// o lumina colorata in spate care isi schimba culoarea dupa stare.
struct MenuContent: View {
    @Environment(Engine.self) private var engine
    @Environment(\.openWindow) private var openWindow
    @State private var copied = false

    private var needsPermission: Bool { !engine.accessibility || !engine.keyboardReady }
    private var isRecording: Bool { engine.state == .recording }
    private var isBusy: Bool { engine.state == .starting || engine.state == .processing }

    private var accent: Color {
        switch engine.state {
        case .recording: return .red
        case .error: return .orange
        default: return needsPermission ? .orange : .accentColor
        }
    }

    var body: some View {
        GlassGroup(spacing: 10) {
            VStack(spacing: 10) {
                header
                if needsPermission { permissionCard }
                orb
                if let text = engine.lastText { lastResult(text) }
                toggles
                menu
            }
        }
        .padding(16)
        .frame(width: 348)
        .background {
            ZStack {
                Color.black.opacity(0.42)
                AmbientGlow(color: accent, secondary: isRecording ? .orange : .purple)
                    .animation(.easeInOut(duration: 0.5), value: engine.state)
            }
            .ignoresSafeArea()
        }
        .environment(\.colorScheme, .dark)
    }

    // MARK: Antet

    private var header: some View {
        HStack(spacing: 11) {
            LogoMark(size: 38)
                .shadow(color: Color(red: 0.3, green: 0.45, blue: 1.0).opacity(0.5), radius: 8, y: 2)
            VStack(alignment: .leading, spacing: 2) {
                Text("Voice Flow").font(.system(size: 17, weight: .semibold))
                HStack(spacing: 6) {
                    Circle().fill(statusColor).frame(width: 7, height: 7)
                        .shadow(color: statusColor.opacity(0.8), radius: 4)
                    Text(statusText).font(.caption).foregroundStyle(.white.opacity(0.65)).lineLimit(1)
                }
            }
            Spacer(minLength: 8)
        }
        .padding(.horizontal, 4)
    }

    // MARK: Butonul mare

    private var orb: some View {
        VStack(spacing: 14) {
            Button {
                engine.toggleDictation()
            } label: {
                ZStack {
                    // Haloul iese de sub disc; cat asculta, pulseaza dupa voce.
                    IridescentHalo(colors: haloColors, energy: isRecording ? voiceLevel : 0)
                    // Discul: sticla adevarata, cu margine luminoasa si o reflexie sus.
                    Circle()
                        .fill(.white.opacity(0.1))
                        .frame(width: 88, height: 88)
                        .liquidGlass(Circle(), tint: isRecording ? .red.opacity(0.25) : nil, interactive: true)
                        .overlay(
                            Circle().strokeBorder(
                                LinearGradient(colors: [.white.opacity(0.9), .white.opacity(0.15), .white.opacity(0.55)],
                                               startPoint: .topLeading, endPoint: .bottomTrailing),
                                lineWidth: 1.2
                            )
                        )
                        .overlay(
                            Ellipse()
                                .fill(LinearGradient(colors: [.white.opacity(0.35), .clear], startPoint: .top, endPoint: .bottom))
                                .frame(width: 56, height: 26)
                                .offset(y: -24)
                                .blur(radius: 3)
                        )
                    if engine.state == .processing {
                        ProgressView().controlSize(.regular).tint(.white)
                    } else if isRecording {
                        MiniBars(bars: engine.bars, color: .white, scale: 30)
                    } else {
                        Image(systemName: "mic.fill")
                            .font(.system(size: 30, weight: .medium))
                            .foregroundStyle(.white.opacity(0.95))
                            .shadow(color: .white.opacity(0.5), radius: 6)
                    }
                }
                .frame(width: 104, height: 104)
                .opacity(engine.state == .starting ? 0.5 : 1)
                .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .disabled(isBusy)
            .accessibilityLabel(isRecording ? "Stop and type" : "Dictate")

            VStack(spacing: 8) {
                Text(orbTitle).font(.system(size: 19, weight: .semibold))
                orbHint
            }
        }
        .padding(.vertical, 6)
        .frame(maxWidth: .infinity)
        .glassCard(radius: 24, padding: 16)
    }

    /// Cat de tare vorbesti acum (0...1), din barele pastilei.
    private var voiceLevel: Double { min(1, engine.bars.max() ?? 0) }

    private var haloColors: [Color] {
        switch engine.state {
        case .recording: return [.red, .orange, .pink, .purple, .red]
        case .error: return [.orange, .yellow, .red, .orange]
        default:
            return needsPermission ? [.orange, .yellow, .pink, .orange] : [.cyan, .blue, .purple, .pink, .cyan]
        }
    }

    private var orbTitle: String {
        switch engine.state {
        case .recording: return "Listening…"
        case .processing: return "Transcribing…"
        case .starting: return "Starting…"
        default: return "Dictate"
        }
    }

    @ViewBuilder
    private var orbHint: some View {
        let dim = Color.white.opacity(0.6)
        switch engine.state {
        case .recording:
            Text("click the circle to stop and type").font(.caption).foregroundStyle(dim)
        case .processing:
            Text("the text appears at the cursor right away").font(.caption).foregroundStyle(dim)
        case .starting:
            Text(engine.message ?? "Loading model…").font(.caption).foregroundStyle(dim)
        default:
            if let keys = engine.settings?.hotkeys, !keys.isEmpty, !needsPermission {
                HStack(spacing: 8) {
                    Text(engine.settings?.hotkeyMode == "toggle" ? "or press" : "or hold")
                        .font(.caption)
                        .foregroundStyle(dim)
                    ForEach(Array(keys.enumerated()), id: \.offset) { i, k in
                        if i > 0 { Text("or").font(.caption).foregroundStyle(.white.opacity(0.4)) }
                        HotkeyView(hotkey: k, compact: true)
                    }
                }
            } else {
                Text("click the circle to start").font(.caption).foregroundStyle(dim)
            }
        }
    }

    // MARK: Permisiune

    private var permissionCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                IconTile(systemName: "exclamationmark", color: .orange, size: 34)
                Text("The dictation key isn't active").font(.callout.weight(.semibold))
            }
            Text("Voice Flow needs the Accessibility permission to detect the key and paste text. It starts on its own as soon as you enable it.")
                .font(.caption)
                .foregroundStyle(.white.opacity(0.7))
                .fixedSize(horizontal: false, vertical: true)
            GlassPillButton(title: "Open System Settings", prominent: true, fullWidth: true) {
                engine.requestPermission()
            }
        }
        .glassCard(radius: 20, tint: .orange.opacity(0.3))
    }

    // MARK: Ultima dictare

    private func lastResult(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Last dictation").font(.caption.weight(.semibold)).foregroundStyle(.white.opacity(0.55))
                Spacer()
                if let timing = engine.lastTiming {
                    Text(timing).font(.caption2).foregroundStyle(.white.opacity(0.4))
                }
            }
            HStack(alignment: .bottom, spacing: 10) {
                Text(text)
                    .font(.callout)
                    .textSelection(.enabled)
                    .lineLimit(5)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                GlassIconButton(systemName: copied ? "checkmark" : "doc.on.doc", size: 30, help: "Copy") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) { copied = false }
                }
            }
        }
        .glassCard(radius: 20)
    }

    // MARK: Comutatoare

    private var toggles: some View {
        VStack(spacing: 8) {
            toggleRow(
                "Corrections with the local model", icon: "wand.and.sparkles", color: .purple,
                isOn: Binding(
                    get: { engine.settings?.cleanup.enabled ?? false },
                    set: { engine.setCleanup($0) }
                )
            )
            toggleRow(
                "Start and stop sounds", icon: "speaker.wave.2.fill", color: .pink,
                isOn: Binding(
                    get: { engine.settings?.sounds ?? true },
                    set: { engine.setSounds($0) }
                )
            )
        }
    }

    private func toggleRow(_ title: String, icon: String, color: Color, isOn: Binding<Bool>) -> some View {
        Toggle(isOn: isOn) {
            HStack(spacing: 12) {
                IconTile(systemName: icon, color: color, size: 36)
                Text(title).font(.callout)
                Spacer(minLength: 0)
            }
        }
        .toggleStyle(GlassToggleStyle())
        .tint(.green)
        .glassCard(radius: 18, padding: 12)
    }

    // MARK: Meniu

    private var menu: some View {
        VStack(spacing: 2) {
            GlassMenuRow(title: "Settings…", icon: "gearshape", shortcut: "⌘,") {
                NSApp.activate(ignoringOtherApps: true)
                openWindow(id: "settings")
            }
            .keyboardShortcut(",")
            GlassMenuRow(title: "Dictionary", icon: "character.book.closed") {
                engine.requestedSettingsTab = "dictionary"
                NSApp.activate(ignoringOtherApps: true)
                openWindow(id: "settings")
            }
            Rectangle().fill(.white.opacity(0.14)).frame(height: 0.6).padding(.vertical, 5).padding(.horizontal, 6)
            GlassMenuRow(title: "Quit Voice Flow", icon: "power", shortcut: "⌘Q", destructive: true) {
                engine.quit()
                NSApp.terminate(nil)
            }
            .keyboardShortcut("q")
        }
        .glassCard(radius: 20, padding: 8)
    }

    // MARK: Stare

    private var statusText: String {
        if let m = engine.message, engine.state == .error || engine.state == .starting { return m }
        switch engine.state {
        case .starting: return "Starting…"
        case .idle: return engine.keyboardReady ? "Ready to dictate · \(modelBadge)" : "Waiting for permission"
        case .recording: return "Listening…"
        case .processing: return "Transcribing…"
        case .error: return engine.message ?? "Error"
        }
    }

    private var modelBadge: String {
        switch engine.settings?.engine {
        case "parakeet-ro": return "Romanian"
        case "parakeet-v3": return "multilingual"
        default: return engine.model
        }
    }

    private var statusColor: Color {
        switch engine.state {
        case .idle: return engine.keyboardReady ? .green : .orange
        case .recording: return .red
        case .processing: return .blue
        case .starting: return .gray
        case .error: return .orange
        }
    }
}
