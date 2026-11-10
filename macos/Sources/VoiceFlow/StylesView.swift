import SwiftUI

/// Ecranul "Styles": cat de mult corecteaza modelul si ce stil primeste fiecare aplicatie.
struct StylesView: View {
    @Environment(Engine.self) private var engine
    @State private var store = StylesStore.shared
    /// Instructiunile in curs de editare, pe id de stil (pana la "Save").
    @State private var drafts: [String: String] = [:]

    private struct LevelInfo {
        let title: String
        let explanation: String
        let after: String
    }

    private static let exampleBefore = "um so let's meet Tuesday no actually Wednesday at ten"

    private static let levels: [String: LevelInfo] = [
        "light": LevelInfo(
            title: "Light",
            explanation: "Only punctuation, capitalization and diacritics. No word is removed or moved.",
            after: "Um, so let's meet Tuesday, no, actually Wednesday, at ten."),
        "medium": LevelInfo(
            title: "Medium",
            explanation: "Also removes hesitations and repetitions and fixes terms from the dictionary.",
            after: "Let's meet Tuesday, no, actually Wednesday, at ten."),
        "high": LevelInfo(
            title: "Strong",
            explanation: "Also keeps only the final version when you correct yourself, turns enumerations into lists and splits the text into paragraphs.",
            after: "Let's meet Wednesday at ten."),
    ]

    private var cleanupOn: Bool { engine.settings?.cleanup.enabled ?? false }

    var body: some View {
        Group {
            SectionTitle("Correction level")
            levelCard
            if !cleanupOn {
                NoteText("Corrections are off right now, so text is pasted as it was transcribed. The level and styles apply once you turn them on in the Corrections section.")
            }

            SectionTitle("Style per app")
            SettingRow(title: "Different style per app",
                       subtitle: "Messages come out short and informal, emails get paragraphs, and in terminals and editors the text is left untouched.",
                       icon: "paintbrush.pointed.fill", color: .purple) {
                GlassSwitch(isOn: Binding(get: { store.enabled }, set: { store.setEnabled($0) }))
            }
            currentAppRow

            ForEach(store.styles) { style in
                styleCard(style)
            }
            NoteText("Apps that aren't listed under any style get the Default style. A style's instruction is added to the rules of the level chosen above.")
        }
    }

    // MARK: Nivel

    private var levelCard: some View {
        let info = Self.levels[store.level] ?? Self.levels["medium"]!
        return VStack(alignment: .leading, spacing: 12) {
            GlassSegmented(
                options: [("light", "Light"), ("medium", "Medium"), ("high", "Strong")],
                selection: Binding(get: { store.level }, set: { store.setLevel($0) })
            )
            Text(info.explanation)
                .font(.callout)
                .foregroundStyle(.white.opacity(0.8))
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 6) {
                exampleLine("Said", Self.exampleBefore, dimmed: true)
                exampleLine("Typed", info.after, dimmed: false)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.black.opacity(0.22), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .glassCard(radius: 16, padding: 14)
    }

    private func exampleLine(_ label: String, _ text: String, dimmed: Bool) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(0.5))
                .frame(width: 36, alignment: .leading)
            Text(text)
                .font(.system(size: 13))
                .foregroundStyle(.white.opacity(dimmed ? 0.6 : 0.95))
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    // MARK: Aplicatia din fata

    private var currentAppRow: some View {
        let subtitle: String
        if let app = store.currentApp {
            if !store.enabled {
                subtitle = "\(app.name) · per-app styles are off, so it gets Default"
            } else {
                subtitle = "\(app.name) · gets the \(store.styleName(store.currentStyle)) style"
            }
        } else {
            subtitle = "No app detected yet. Switch to the app you dictate in, then come back."
        }
        return SettingRow(title: "App detected now", subtitle: subtitle, icon: "macwindow", color: .blue) {
            if let last = store.lastApplied {
                VStack(alignment: .trailing, spacing: 2) {
                    Text("Last dictation").font(.caption2).foregroundStyle(.white.opacity(0.5))
                    Text(last.cleaned ? last.styleName : "\(last.styleName), no corrections")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.85))
                }
            }
        }
    }

    // MARK: Stiluri

    private func icon(_ id: String) -> (name: String, color: Color) {
        switch id {
        case "messages": return ("bubble.left.and.bubble.right.fill", .green)
        case "email": return ("envelope.fill", .blue)
        case "code": return ("chevron.left.forwardslash.chevron.right", .orange)
        default: return ("text.alignleft", .gray)
        }
    }

    private func summary(_ id: String) -> String {
        switch id {
        case "messages": return "Informal and short, with no period at the end of a single sentence."
        case "email": return "Full sentences, polite tone, paragraphs."
        case "code": return "No corrections: text is pasted exactly as transcribed."
        default: return "All other apps."
        }
    }

    private func styleCard(_ style: DictationStyle) -> some View {
        let tile = icon(style.id)
        let isCurrent = store.enabled && store.currentApp != nil && store.currentStyle == style.id
        return VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                IconTile(systemName: tile.name, color: tile.color, size: 36)
                VStack(alignment: .leading, spacing: 2) {
                    Text(style.name).font(.system(size: 15, weight: .semibold))
                    Text(summary(style.id)).font(.caption).foregroundStyle(.white.opacity(0.6))
                }
                Spacer(minLength: 8)
                if isCurrent { Chip("active now") }
            }

            if style.id != "default" || !style.apps.isEmpty {
                FieldLabel(style.id == "default" ? "Apps removed from the other styles" : "Apps")
            }
            ChipFlow(spacing: 6) {
                ForEach(style.apps) { app in
                    appChip(app)
                }
                addAppMenu(style)
            }

            if style.skipCleanup {
                NoteText("In these apps the corrections model isn't used at all; only the dictionary is applied.")
            } else {
                instructionEditor(style)
            }
        }
        .glassCard(radius: 18, padding: 14, tint: isCurrent ? Color.accentColor.opacity(0.14) : nil)
        .opacity(store.enabled || style.id == "default" ? 1 : 0.55)
    }

    private func appChip(_ app: StyleApp) -> some View {
        HStack(spacing: 5) {
            Text(app.name).font(.system(size: 12))
            Button { store.unassign(app) } label: {
                Image(systemName: "xmark").font(.system(size: 8, weight: .bold))
            }
            .buttonStyle(.plain)
            .help("Remove the app from this style")
            .accessibilityLabel("Remove \(app.name)")
        }
        .foregroundStyle(.white.opacity(0.9))
        .padding(.leading, 10)
        .padding(.trailing, 8)
        .frame(height: 24)
        .background(.white.opacity(0.14), in: Capsule())
        .overlay(Capsule().strokeBorder(.white.opacity(0.22), lineWidth: 0.6))
        .help(app.bundleId)
    }

    private func addAppMenu(_ style: DictationStyle) -> some View {
        let candidates = store.candidates(for: style)
        return Menu {
            if candidates.isEmpty {
                Text("Open the app, then come back here")
            }
            ForEach(candidates) { app in
                Button(app.name) { store.assign(app, to: style.id) }
            }
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "plus").font(.system(size: 9, weight: .bold))
                Text("Add app").font(.system(size: 12, weight: .medium))
            }
            .foregroundStyle(.white.opacity(0.85))
            .padding(.horizontal, 10)
            .frame(height: 24)
            .overlay(Capsule().strokeBorder(.white.opacity(0.3), style: StrokeStyle(lineWidth: 0.8, dash: [3, 2])))
            .contentShape(Capsule())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Choose from recently used apps")
    }

    private func instructionEditor(_ style: DictationStyle) -> some View {
        let draft = drafts[style.id] ?? style.instruction
        let changed = draft.trimmingCharacters(in: .whitespacesAndNewlines) != style.instruction
        return VStack(alignment: .leading, spacing: 8) {
            FieldLabel(style.id == "default" ? "Instruction for the model (optional)" : "Instruction for the model")
            GlassTextEditor(
                text: Binding(get: { drafts[style.id] ?? style.instruction }, set: { drafts[style.id] = $0 }),
                minHeight: style.id == "default" ? 46 : 72
            )
            if changed || style.customized {
                HStack(spacing: 10) {
                    if changed {
                        GlassPillButton(title: "Save", icon: "checkmark", prominent: true) {
                            store.saveInstruction(style.id, draft)
                            drafts[style.id] = nil
                        }
                        GlassPillButton(title: "Cancel") { drafts[style.id] = nil }
                    }
                    if style.customized {
                        GlassPillButton(title: "Reset to default", icon: "arrow.uturn.backward") {
                            drafts[style.id] = nil
                            store.resetInstruction(style.id)
                        }
                    }
                    Spacer()
                }
            }
        }
    }
}
