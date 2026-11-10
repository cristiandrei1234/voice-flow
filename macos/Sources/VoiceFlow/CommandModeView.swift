import SwiftUI

/// Ecranul "Commands": Command Mode (transforma textul selectat dupa o instructiune rostita).
struct CommandModeView: View {
    private var store: CommandModeStore { CommandModeStore.shared }
    @State private var draft = ""
    @State private var confirmReset = false

    private var draftChanged: Bool {
        draft.trimmingCharacters(in: .whitespacesAndNewlines) != store.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        Group {
            SectionTitle("Commands on selected text")
            SettingRow(
                title: "Transform selected text with your voice",
                subtitle: "Select some text, hold the shortcut below and say what you want done with it.",
                icon: "text.badge.checkmark", color: .indigo
            ) {
                GlassSwitch(isOn: Binding(get: { store.enabled }, set: { store.setEnabled($0) }))
            }
            modelWarning

            SectionTitle("Shortcut")
            hotkeyRow
            if let problem = store.problem {
                ErrorBanner(message: problem) { store.problem = nil }
            }
            if let error = store.hotkeyError {
                ErrorBanner(message: error) { store.hotkeyError = nil }
            }
            NoteText("The shortcut must be different from the dictation keys. Hold it while you speak; Esc cancels.")

            SectionTitle("How it works")
            howItWorks

            SectionTitle("Last command")
            lastCommand

            SectionTitle("Prompt")
            promptEditor
        }
        .onAppear { draft = store.prompt }
        .onChange(of: store.prompt) { _, new in draft = new }
    }

    // MARK: Model

    @ViewBuilder
    private var modelWarning: some View {
        switch store.modelStatus {
        case .ready:
            EmptyView()
        case .starting:
            HStack(spacing: 10) {
                ProgressView().controlSize(.small)
                Text("The local model is starting… Commands will work as soon as it's ready.")
                    .font(.callout)
                    .foregroundStyle(.white.opacity(0.8))
                Spacer()
            }
            .glassCard(radius: 14, padding: 12)
        case .unavailable(let reason):
            HStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                VStack(alignment: .leading, spacing: 2) {
                    Text(reason).font(.callout).fixedSize(horizontal: false, vertical: true)
                    Text("Choose and download a model in the Corrections section.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.6))
                }
                Spacer(minLength: 8)
                GlassPillButton(title: "Corrections", icon: "wand.and.sparkles") { store.openCleanupSection() }
            }
            .glassCard(radius: 14, padding: 12, tint: .orange.opacity(0.25))
        }
    }

    // MARK: Scurtatura

    private var hotkeyRow: some View {
        HStack(spacing: 12) {
            if store.isCapturingHotkey {
                ProgressView().controlSize(.small)
                Text("Press the shortcut you want… (Esc cancels)")
                    .font(.callout.weight(.medium))
                    .foregroundStyle(.white.opacity(0.85))
            } else {
                HotkeyView(hotkey: store.hotkey)
            }
            Spacer()
            GlassPillButton(title: "Change", icon: "keyboard") { store.captureHotkey() }
                .disabled(store.isCapturingHotkey)
                .opacity(store.isCapturingHotkey ? 0.4 : 1)
        }
        .glassCard(radius: 16, padding: 12)
    }

    // MARK: Explicatie

    private var howItWorks: some View {
        VStack(alignment: .leading, spacing: 10) {
            step(1, "Select the text, in any app.")
            step(2, "Hold the shortcut and say the instruction.")
            step(3, "Release: the selected text is replaced with the result.")
            Text("Example instructions")
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(0.6))
                .padding(.top, 2)
            ChipFlow(spacing: 6) {
                ForEach(CommandModeStore.examples, id: \.self) { example in
                    Chip("“\(example)”")
                }
            }
            Text("Everything runs on your Mac, using the model from the Corrections section. Your clipboard is restored after each command.")
                .font(.caption)
                .foregroundStyle(.white.opacity(0.5))
                .fixedSize(horizontal: false, vertical: true)
        }
        .glassCard(radius: 16, padding: 14)
    }

    private func step(_ number: Int, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text("\(number)")
                .font(.system(size: 11, weight: .bold, design: .rounded))
                .frame(width: 20, height: 20)
                .background(.white.opacity(0.14), in: Circle())
            Text(text).font(.callout).fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: Ultima comanda

    @ViewBuilder
    private var lastCommand: some View {
        if let failure = store.lastFailure {
            ErrorBanner(message: failure) { store.lastFailure = nil }
        }
        if let result = store.lastResult {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 8) {
                    Image(systemName: "waveform").foregroundStyle(.white.opacity(0.6))
                    Text("“\(result.instruction)”").font(.system(size: 14, weight: .medium))
                }
                resultBlock("Before", result.before, dimmed: true)
                resultBlock("After", result.after, dimmed: false)
            }
            .glassCard(radius: 16, padding: 14)
        } else if store.lastFailure == nil {
            EmptyStateCard(
                icon: "text.badge.checkmark",
                title: "No commands yet",
                subtitle: "The last instruction appears here, with the text before and after."
            )
        }
    }

    private func resultBlock(_ label: String, _ text: String, dimmed: Bool) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            FieldLabel(label)
            Text(text.trimmingCharacters(in: .whitespacesAndNewlines))
                .font(.system(size: 13))
                .foregroundStyle(.white.opacity(dimmed ? 0.6 : 0.95))
                .lineLimit(8)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
                .padding(10)
                .background(.black.opacity(0.22), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
    }

    // MARK: Prompt

    @ViewBuilder
    private var promptEditor: some View {
        VStack(alignment: .leading, spacing: 10) {
            GlassTextEditor(text: $draft, minHeight: 170)
            HStack(spacing: 8) {
                if !store.promptIsDefault {
                    GlassPillButton(title: "Reset to default prompt", icon: "arrow.uturn.backward") {
                        confirmReset = true
                    }
                }
                Spacer()
                if draftChanged {
                    GlassPillButton(title: "Cancel") { draft = store.prompt }
                    GlassPillButton(title: "Save", icon: "checkmark", prominent: true) { store.savePrompt(draft) }
                }
            }
        }
        .glassCard(radius: 16, padding: 14)
        .confirmationDialog("Reset to the default prompt?", isPresented: $confirmReset) {
            Button("Reset to default", role: .destructive) { store.resetPrompt() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Your prompt for commands will be replaced.")
        }
        NoteText("The prompt tells the model how to handle the selected text and the instruction. After it, the model receives the text and what you said.")
    }
}
