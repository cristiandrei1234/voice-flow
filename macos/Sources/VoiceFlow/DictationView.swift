import SwiftUI

/// Sectiunea "Dictation": cum pornesti si ce se intampla cu vocea inainte de transcriere.
struct DictationView: View {
    @Environment(Engine.self) private var engine
    private var store: DictationStore { DictationStore.shared }

    private var hotkeys: [String] { engine.settings?.hotkeys ?? [] }
    private var holdMode: Bool { (engine.settings?.hotkeyMode ?? "hold") == "hold" }
    private var usesWhisper: Bool { engine.settings?.engine.hasPrefix("whisper") ?? false }

    var body: some View {
        Group {
            SectionTitle("Hands-free")
            SettingRow(
                title: "Double-tap to lock",
                subtitle: "Tap twice quickly and dictate without holding the key; one more tap stops, Esc cancels.",
                icon: "lock.fill", color: .red
            ) {
                GlassSwitch(isOn: Binding(get: { store.doubleTapLock }, set: { store.setDoubleTapLock($0) }))
                    .tint(.green)
            }
            if !hotkeys.isEmpty {
                HStack(spacing: 8) {
                    Text("Double-tap").font(.callout).foregroundStyle(.white.opacity(0.6))
                    ForEach(Array(hotkeys.enumerated()), id: \.offset) { i, key in
                        if i > 0 { Text("or").font(.callout).foregroundStyle(.white.opacity(0.6)) }
                        HotkeyView(hotkey: key, compact: true)
                    }
                    Spacer()
                }
                .glassCard(radius: 16, padding: 12)
                .opacity(store.doubleTapLock && holdMode ? 1 : 0.45)
            }
            if holdMode {
                NoteText("If you hold the key, you dictate as before. While locked, the pill shows a red dot.")
            } else {
                NoteText("Locking only works with “Hold” (General > How you dictate). With “Press once” you're already hands-free.")
            }

            SectionTitle("Voice")
            SettingRow(
                title: "Boost quiet speech",
                subtitle: "When you speak very quietly, raises the recording volume before transcription.",
                icon: "waveform.badge.plus", color: .purple
            ) {
                GlassSwitch(isOn: Binding(get: { store.normalizeQuiet }, set: { store.setNormalizeQuiet($0) }))
                    .tint(.green)
            }

            if usesWhisper {
                SettingRow(
                    title: "Whisper language",
                    subtitle: "“Auto” guesses the language for each dictation. Changing it reloads the model.",
                    icon: "globe", color: .blue
                ) {
                    EmptyView()
                }
                HStack {
                    GlassSegmented(
                        options: DictationStore.whisperLanguages,
                        selection: Binding(get: { store.whisperLanguage }, set: { store.setWhisperLanguage($0) })
                    )
                    Spacer()
                }
            }

            SectionTitle("Startup")
            SettingRow(
                title: "Launch at login",
                subtitle: loginSubtitle,
                icon: "power", color: .green,
                tint: store.loginItem == .needsApproval ? .orange.opacity(0.25) : nil
            ) {
                GlassSwitch(isOn: Binding(
                    get: { store.loginItem == .on || store.loginItem == .needsApproval },
                    set: { store.setLaunchAtLogin($0) }
                ))
                .tint(.green)
                .disabled(store.loginItem == .unavailable)
                .opacity(store.loginItem == .unavailable ? 0.45 : 1)
            }
            if store.loginItem == .needsApproval {
                HStack {
                    GlassPillButton(title: "Open Login Items", icon: "gear") {
                        store.openLoginItemsSettings()
                    }
                    Spacer()
                }
            }
            if let error = store.loginError {
                ErrorBanner(message: error) { store.loginError = nil }
            }
        }
        .onAppear { store.refreshLoginItem() }
    }

    private var loginSubtitle: String {
        switch store.loginItem {
        case .on: "On: Voice Flow starts automatically when you log in."
        case .off: "Off."
        case .needsApproval: "Waiting for your approval: allow Voice Flow in System Settings > General > Login Items."
        case .unavailable: "Unavailable: macOS can't find the app. Install it in Applications and open it from there."
        }
    }
}
