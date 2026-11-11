import AppKit
import SwiftUI

/// Fereastra de setari in Liquid Glass: fereastra transparenta, un panou de sticla fumurie,
/// taburi intr-o pastila de sticla si cate un card de sticla pentru fiecare rand.
struct SettingsView: View {
    enum Tab: String, CaseIterable {
        case general, transcription, cleanup, dictionary, permissions

        var title: String {
            switch self {
            case .general: return "General"
            case .transcription: return "Transcription"
            case .cleanup: return "Corrections"
            case .dictionary: return "Dictionary"
            case .permissions: return "Permissions"
            }
        }

        var icon: String {
            switch self {
            case .general: return "slider.horizontal.3"
            case .transcription: return "waveform"
            case .cleanup: return "wand.and.sparkles"
            case .dictionary: return "character.book.closed.fill"
            case .permissions: return "lock.shield.fill"
            }
        }
    }

    @Environment(Engine.self) private var engine
    /// Sectiunea aleasa: `Tab.rawValue` pentru cele de baza sau `AppSection.id` pentru functii.
    @State private var section: String
    @Namespace private var sidebarNS
    @State private var pendingDelete: ModelInfo?
    @State private var pendingLlmDelete: LlmInfo?
    @State private var pendingEntryDelete: DictEntry?
    @State private var dictSearch = ""
    @State private var editor: DictEditor?

    init(initialTab: Tab = .general, initialSection: String? = nil, demoEditor: DictEditor? = nil) {
        _section = State(initialValue: initialSection ?? initialTab.rawValue)
        _editor = State(initialValue: demoEditor)
    }

    /// Sectiunile din bara laterala: cele de baza, apoi functiile optionale, apoi permisiunile.
    /// Ordinea din bara laterala: intai cum dictezi, apoi ce se intampla cu textul, apoi ce ai strans.
    private static let sidebarOrder = [
        "general", "dictation", "transcription", "cleanup", "styles", "command",
        "dictionary", "snippets", "history", "notes", "permissions",
    ]

    private var sidebarItems: [(id: String, title: String, icon: String)] {
        let builtIn = Tab.allCases.map { (id: $0.rawValue, title: $0.title, icon: $0.icon) }
        let extra = Features.sections.map { (id: $0.id, title: $0.title, icon: $0.icon) }
        let all = builtIn + extra
        let rank: (String) -> Int = { id in Self.sidebarOrder.firstIndex(of: id) ?? Self.sidebarOrder.count }
        // Sortare stabila: o sectiune noua, necunoscuta inca aici, apare la sfarsit.
        return all.enumerated()
            .sorted { (rank($0.element.id), $0.offset) < (rank($1.element.id), $1.offset) }
            .map(\.element)
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 3) {
            LogoLockup(markSize: 30, fontSize: 17)
                .padding(.leading, 8)
                .padding(.top, 36)
                .padding(.bottom, 14)
            ForEach(sidebarItems, id: \.id) { item in
                let selected = item.id == section
                Button {
                    withAnimation(.spring(response: 0.34, dampingFraction: 0.78)) { section = item.id }
                } label: {
                    HStack(spacing: 11) {
                        Image(systemName: item.icon)
                            .font(.system(size: 14, weight: .semibold))
                            .frame(width: 22)
                            .shadow(color: .white.opacity(selected ? 0.8 : 0), radius: 5)
                        Text(item.title).font(.system(size: 13.5, weight: selected ? .semibold : .medium))
                        Spacer(minLength: 0)
                    }
                    .foregroundStyle(.white.opacity(selected ? 1 : 0.55))
                    .padding(.horizontal, 12)
                    .frame(height: 38)
                    .background {
                        if selected {
                            RoundedRectangle(cornerRadius: 13, style: .continuous)
                                .fill(.white.opacity(0.06))
                                .liquidGlass(RoundedRectangle(cornerRadius: 13, style: .continuous), interactive: true)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 13, style: .continuous).strokeBorder(
                                        LinearGradient(colors: [.white.opacity(0.7), .white.opacity(0.12), .white.opacity(0.45)],
                                                       startPoint: .topLeading, endPoint: .bottomTrailing),
                                        lineWidth: 0.9
                                    )
                                )
                                .matchedGeometryEffect(id: "lens", in: sidebarNS)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10)
        .padding(.bottom, 12)
        .frame(width: 196)
        .background(.black.opacity(0.32))
    }

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            ScrollView {
                GlassGroup(spacing: 8) {
                    VStack(alignment: .leading, spacing: 8) {
                        switch Tab(rawValue: section) {
                        case .general: generalTab
                        case .transcription: transcriptionTab
                        case .cleanup: cleanupTab
                        case .dictionary: dictionaryTab
                        case .permissions: permissionsTab
                        case nil:
                            if let extra = Features.sections.first(where: { $0.id == section }) {
                                extra.view()
                            }
                        }
                    }
                    .padding(.horizontal, 22)
                    .padding(.top, 18)
                    .padding(.bottom, 24)
                }
            }
            .scrollIndicators(.never)
        }
        .frame(width: 800, height: 660)
        .background {
            ZStack {
                AmbientGlow(color: .accentColor, secondary: .purple).scaleEffect(1.6)
                Rectangle().fill(.clear).liquidGlass(Rectangle(), tint: .black.opacity(0.55))
            }
            .ignoresSafeArea()
        }
        .environment(\.colorScheme, .dark)
        .background(GlassWindowConfigurator())
        .confirmationDialog(
            "Delete \(pendingDelete?.title ?? "the model") from disk?",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            presenting: pendingDelete
        ) { model in
            Button("Delete (\(model.sizeText))", role: .destructive) { engine.deleteModel(model.id) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("You can download it again anytime.")
        }
        .confirmationDialog(
            "Delete \(pendingLlmDelete?.title ?? "the model") from disk?",
            isPresented: Binding(get: { pendingLlmDelete != nil }, set: { if !$0 { pendingLlmDelete = nil } }),
            presenting: pendingLlmDelete
        ) { model in
            Button("Delete (\(model.sizeText))", role: .destructive) { engine.deleteLlm(model.repo) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("You can download it again anytime.")
        }
        .confirmationDialog(
            "Delete “\(pendingEntryDelete?.correct ?? "")” from the dictionary?",
            isPresented: Binding(get: { pendingEntryDelete != nil }, set: { if !$0 { pendingEntryDelete = nil } }),
            presenting: pendingEntryDelete
        ) { entry in
            Button("Delete", role: .destructive) { engine.deleteEntry(entry) }
            Button("Cancel", role: .cancel) {}
        } message: { entry in
            Text(entry.aliases.isEmpty ? "The term is removed from the dictionary."
                 : entry.aliases.count == 1 ? "Its variant is removed too." : "Its \(entry.aliases.count) variants are removed too.")
        }
        .onChange(of: engine.requestedSettingsTab) { _, new in
            if let new {
                section = new
                engine.requestedSettingsTab = nil
            }
        }
        .onAppear {
            if let r = engine.requestedSettingsTab {
                section = r
                engine.requestedSettingsTab = nil
            }
            engine.refreshModels()
            // Daca lipseste o permisiune, deschidem direct sectiunea unde se rezolva.
            if !engine.accessibility || !engine.keyboardReady { section = Tab.permissions.rawValue }
        }
    }

    // MARK: General

    private var hotkeys: [String] { engine.settings?.hotkeys ?? [] }

    @ViewBuilder
    private var generalTab: some View {
        sectionTitle("Dictation key")
        ForEach(hotkeys, id: \.self) { key in
            HStack {
                HotkeyView(hotkey: key)
                Spacer()
                GlassIconButton(systemName: "minus", size: 28, help: "Remove key") {
                    engine.setHotkeys(hotkeys.filter { $0 != key })
                }
                .disabled(hotkeys.count <= 1)
                .opacity(hotkeys.count <= 1 ? 0.35 : 1)
            }
            .glassCard(radius: 16, padding: 12)
        }

        Button {
            engine.captureHotkey()
        } label: {
            HStack(spacing: 10) {
                if engine.isCapturingHotkey {
                    ProgressView().controlSize(.small)
                    Text("Press the key or combination you want… (Esc cancels)")
                } else {
                    Image(systemName: "plus").font(.system(size: 13, weight: .bold))
                    Text("Add key")
                }
                Spacer()
            }
            .font(.callout.weight(.medium))
            .foregroundStyle(.white.opacity(0.85))
            .glassCard(radius: 16, padding: 14)
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(engine.isCapturingHotkey)
        .onChange(of: engine.capturedHotkey) { _, new in
            guard let new, !hotkeys.contains(new) else { return }
            engine.setHotkeys(hotkeys + [new])
        }

        row("How you dictate", nil, icon: "hand.tap.fill", color: .blue) {
            GlassSegmented(
                options: [("hold", "Hold"), ("toggle", "Press once")],
                selection: Binding(
                    get: { engine.settings?.hotkeyMode ?? "hold" },
                    set: { engine.setMode($0) }
                )
            )
        }
        note("Every key in the list starts dictation; remove the ones you don't want with −. You can use a single key (fn, right ⌘, F13) or a combination (⌃⌥, ⌘⇧Space).")

        sectionTitle("Behavior")
        row("Paste text at the cursor", "Otherwise you'll find it only in the Voice Flow menu.", icon: "text.insert", color: .blue) {
            toggle(get: { engine.settings?.paste ?? true }, set: { engine.setPaste($0) })
        }
        row("Start and stop sounds", nil, icon: "speaker.wave.2.fill", color: .pink) {
            toggle(get: { engine.settings?.sounds ?? true }, set: { engine.setSounds($0) })
        }
    }

    // MARK: Transcriere

    @ViewBuilder
    private var transcriptionTab: some View {
        sectionTitle("Models")
        ForEach(engine.models) { model in
            modelCard(model)
        }
        note("Models run only on your Mac. They're downloaded from Hugging Face; an interrupted download resumes where it left off.")

        sectionTitle("Microphone")
        row("Microphone", nil, icon: "mic.fill", color: .red) {
            value(engine.microphone ?? "unavailable")
        }
    }

    // MARK: Corecturi

    private var cleanupOn: Bool { engine.settings?.cleanup.enabled ?? false }

    @ViewBuilder
    private var cleanupTab: some View {
        sectionTitle("Corrections with a local model")
        row("Correct text after transcription", "Removes hesitations, adds punctuation, fixes dictionary terms.",
            icon: "wand.and.sparkles", color: .purple) {
            toggle(get: { engine.settings?.cleanup.enabled ?? false }, set: { engine.setCleanup($0) })
        }
        row("Server", serverSubtitle, icon: "server.rack", color: serverColor) {
            HStack(spacing: 7) {
                if engine.cleanupServerState == "starting" { ProgressView().controlSize(.small) }
                Text(serverLabel)
                    .font(.callout.weight(.medium))
                    .foregroundStyle(serverColor)
            }
        }
        note("Voice Flow starts the model on its own when you turn corrections on, and stops it when you turn them off or quit the app. While it's running, the model uses about as much memory as its size.")

        sectionTitle("Models")
        ForEach(engine.llmModels) { model in
            llmCard(model)
        }
        note("Models downloaded here are stored in the Voice Flow folder. Models found in the Hugging Face cache can be used, but can't be deleted from here.")
    }

    private var serverLabel: String {
        switch engine.cleanupServerState {
        case "ready": return "Running"
        case "starting": return "Starting…"
        case "external": return "Started by you"
        case "error": return "Error"
        default: return cleanupOn ? "Stopped" : "Stopped"
        }
    }

    private var serverColor: Color {
        switch engine.cleanupServerState {
        case "ready", "external": return .green
        case "starting": return .yellow
        case "error": return .orange
        default: return .gray
        }
    }

    private var serverSubtitle: String? {
        if engine.cleanupServerState == "error" { return engine.cleanupServerMessage }
        if let url = engine.settings?.cleanup.url { return "mlx_lm.server · " + serverShort(url) }
        return nil
    }

    private func llmCard(_ model: LlmInfo) -> some View {
        let progress = engine.llmProgress[model.repo]
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                IconTile(systemName: "brain.fill", color: llmColor(model), size: 40)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(model.title).font(.system(size: 14, weight: .semibold)).lineLimit(1)
                        if model.active {
                            Text("Selected")
                                .font(.system(size: 10.5, weight: .bold))
                                .foregroundStyle(.black)
                                .padding(.horizontal, 7)
                                .frame(height: 17)
                                .background(Color(red: 0.3, green: 0.9, blue: 0.4), in: Capsule())
                        }
                    }
                    Text([model.sizeText, model.installed ? "on disk" : ""].filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.7))
                }
                Spacer(minLength: 10)
                llmActions(model)
            }
            Text(model.note)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.55))
                .fixedSize(horizontal: false, vertical: true)
            if model.downloading {
                HStack(spacing: 10) {
                    GlassProgressBar(value: progress ?? 0)
                    Text("\(Int((progress ?? 0) * 100))%")
                        .font(.system(size: 11, weight: .medium).monospacedDigit())
                        .foregroundStyle(.white.opacity(0.75))
                        .frame(width: 34, alignment: .trailing)
                }
            }
        }
        .glassCard(radius: 18, padding: 14, tint: model.active ? Color.green.opacity(0.14) : nil)
    }

    @ViewBuilder
    private func llmActions(_ model: LlmInfo) -> some View {
        if model.downloading {
            GlassPillButton(title: "Cancel") { engine.cancelLlmDownload(model.repo) }
        } else if model.active {
            EmptyView()
        } else if model.installed {
            HStack(spacing: 8) {
                if model.deletable {
                    GlassIconButton(systemName: "trash", size: 32, help: "Delete model from disk") {
                        pendingLlmDelete = model
                    }
                }
                GlassPillButton(title: "Select", prominent: true) { engine.setCleanupModel(model.model) }
            }
        } else {
            GlassPillButton(title: "Download", icon: "arrow.down", prominent: true) {
                engine.downloadLlm(model.repo)
            }
        }
    }

    private func llmColor(_ model: LlmInfo) -> Color {
        let palette: [Color] = [.purple, .pink, .indigo, .orange, .teal, .blue]
        let index = engine.llmModels.firstIndex(of: model) ?? 0
        return palette[index % palette.count]
    }

    // MARK: Modele

    private func modelCard(_ model: ModelInfo) -> some View {
        let progress = engine.downloadProgress[model.id]
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 12) {
                IconTile(systemName: modelIcon(model), color: modelColor(model), size: 40)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 8) {
                        Text(model.title).font(.system(size: 14, weight: .semibold))
                        if model.active {
                            Text("Active")
                                .font(.system(size: 10.5, weight: .bold))
                                .foregroundStyle(.black)
                                .padding(.horizontal, 7)
                                .frame(height: 17)
                                .background(Color(red: 0.3, green: 0.9, blue: 0.4), in: Capsule())
                        }
                    }
                    Text([model.languages, model.sizeText].filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.7))
                }
                Spacer(minLength: 10)
                modelActions(model)
            }
            Text(model.note)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.55))
                .fixedSize(horizontal: false, vertical: true)
            if model.downloading {
                HStack(spacing: 10) {
                    GlassProgressBar(value: progress ?? 0)
                    Text("\(Int((progress ?? 0) * 100))%")
                        .font(.system(size: 11, weight: .medium).monospacedDigit())
                        .foregroundStyle(.white.opacity(0.75))
                        .frame(width: 34, alignment: .trailing)
                }
            }
        }
        .glassCard(radius: 18, padding: 14, tint: model.active ? Color.green.opacity(0.14) : nil)
    }

    @ViewBuilder
    private func modelActions(_ model: ModelInfo) -> some View {
        if model.downloading {
            GlassPillButton(title: "Cancel") { engine.cancelDownload(model.id) }
        } else if model.active {
            EmptyView()
        } else if model.installed {
            HStack(spacing: 8) {
                if model.downloadable {
                    GlassIconButton(systemName: "trash", size: 32, help: "Delete model from disk") {
                        pendingDelete = model
                    }
                }
                GlassPillButton(title: "Use", prominent: true) { engine.setEngine(model.id) }
            }
        } else if model.downloadable {
            GlassPillButton(title: "Download", icon: "arrow.down", prominent: true) {
                engine.downloadModel(model.id)
            }
        } else {
            Text("missing")
                .font(.caption.weight(.medium))
                .foregroundStyle(.orange)
                .help("This model can't be downloaded; export it locally with scripts/export-parakeet-ro.py")
        }
    }

    private func modelIcon(_ model: ModelInfo) -> String {
        model.id.hasPrefix("whisper") ? "globe" : (model.id == "parakeet-ro" ? "star.fill" : "bird.fill")
    }

    private func modelColor(_ model: ModelInfo) -> Color {
        switch model.id {
        case "parakeet-ro": return .yellow
        case "parakeet-v3": return .indigo
        case "parakeet-v2": return .blue
        case "whisper-turbo": return .teal
        default: return .mint
        }
    }

    // MARK: Dictionar

    private var filteredEntries: [DictEntry] {
        let q = dictSearch.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return engine.dictEntries }
        return engine.dictEntries.filter { e in
            e.correct.lowercased().contains(q) || e.aliases.contains { $0.lowercased().contains(q) }
        }
    }

    /// Termenii, pe grupuri, in ordinea din fisier.
    private var groupedEntries: [(group: String, entries: [DictEntry])] {
        var out: [(group: String, entries: [DictEntry])] = []
        for e in filteredEntries {
            let g = e.group ?? ""
            if let i = out.firstIndex(where: { $0.group == g }) {
                out[i].entries.append(e)
            } else {
                out.append((g, [e]))
            }
        }
        return out
    }

    @ViewBuilder
    private var dictionaryTab: some View {
        HStack(spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").foregroundStyle(.white.opacity(0.5))
                TextField("Search \(engine.dictEntries.count) terms", text: $dictSearch)
                    .textFieldStyle(.plain)
                    .font(.system(size: 14))
                if !dictSearch.isEmpty {
                    Button { dictSearch = "" } label: {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.white.opacity(0.5))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 14)
            .frame(height: 38)
            .liquidGlass(Capsule())

            GlassPillButton(title: "New term", icon: "plus", prominent: true) {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                    editor = DictEditor(group: engine.dictGroups.first ?? "")
                }
            }
        }

        if let err = engine.lastError {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                Text(err).font(.callout).fixedSize(horizontal: false, vertical: true)
                Spacer()
                Button { engine.lastError = nil } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
            }
            .glassCard(radius: 14, padding: 12, tint: .orange.opacity(0.25))
        }

        if let ed = editor, ed.line == nil {
            editorCard
        }

        if engine.dictEntries.isEmpty {
            emptyState("The dictionary is empty", "Add names, brands, and technical terms the model gets wrong.")
        } else if filteredEntries.isEmpty {
            emptyState("No terms found", "Nothing matches “\(dictSearch)”.")
        }

        ForEach(groupedEntries, id: \.group) { section in
            HStack {
                Text(section.group.isEmpty ? "No group" : section.group)
                    .font(.system(size: 15, weight: .semibold))
                Text("\(section.entries.count)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.7))
                    .padding(.horizontal, 7)
                    .frame(height: 18)
                    .background(.white.opacity(0.12), in: Capsule())
                Spacer()
            }
            .padding(.leading, 4)
            .padding(.top, 14)
            .padding(.bottom, 2)

            ForEach(section.entries) { entry in
                if editor?.line == entry.line {
                    editorCard
                } else {
                    entryCard(entry)
                }
            }
        }

        HStack(spacing: 10) {
            note("Variants are what the model writes when it mishears the term, and they're replaced automatically. A foreign word usually needs one: the Romanian model writes “riviu” or “liviu” for “review”, so the correct form alone won't fix it. The easiest way to add them is “Fix a word” in History. Don't add ordinary everyday words as variants.")
            GlassPillButton(title: "File", icon: "doc.text") {
                if let path = engine.settings?.dictionaryPath { openInTextEdit(path) }
            }
            .help("Open dictionary.txt in TextEdit")
        }
        .padding(.top, 10)
    }

    private func entryCard(_ entry: DictEntry) -> some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 7) {
                Text(entry.correct).font(.system(size: 15, weight: .semibold))
                if entry.aliases.isEmpty {
                    Text("no variants yet: add how the model writes it when it gets it wrong")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.45))
                } else {
                    ChipFlow(spacing: 6) {
                        ForEach(entry.aliases, id: \.self) { alias in
                            Text(alias)
                                .font(.system(size: 12))
                                .foregroundStyle(.white.opacity(0.85))
                                .padding(.horizontal, 9)
                                .frame(height: 22)
                                .background(.white.opacity(0.1), in: Capsule())
                                .overlay(Capsule().strokeBorder(.white.opacity(0.16), lineWidth: 0.6))
                        }
                    }
                }
            }
            Spacer(minLength: 8)
            GlassIconButton(systemName: "pencil", size: 30, help: "Edit") {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) {
                    editor = DictEditor(line: entry.line, expected: entry.correct, correct: entry.correct,
                                        aliases: entry.aliases, group: entry.group ?? "")
                }
            }
            GlassIconButton(systemName: "trash", size: 30, help: "Delete") { pendingEntryDelete = entry }
        }
        .glassCard(radius: 16, padding: 13)
    }

    /// Formularul de adaugare / modificare, in locul cardului.
    private var editorCard: some View {
        let isNew = editor?.line == nil
        return VStack(alignment: .leading, spacing: 12) {
            Text(isNew ? "New term" : "Edit term")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white.opacity(0.7))

            fieldLabel("Correct form")
            glassField("e.g. NestJS", text: Binding(get: { editor?.correct ?? "" }, set: { editor?.correct = $0 }))

            fieldLabel("How the model mishears it (optional)")
            if let aliases = editor?.aliases, !aliases.isEmpty {
                ChipFlow(spacing: 6) {
                    ForEach(aliases, id: \.self) { alias in
                        HStack(spacing: 5) {
                            Text(alias).font(.system(size: 12))
                            Button {
                                editor?.aliases.removeAll { $0 == alias }
                            } label: {
                                Image(systemName: "xmark").font(.system(size: 8, weight: .bold))
                            }
                            .buttonStyle(.plain)
                            .help("Remove variant")
                        }
                        .foregroundStyle(.white.opacity(0.9))
                        .padding(.leading, 10)
                        .padding(.trailing, 8)
                        .frame(height: 24)
                        .background(.white.opacity(0.14), in: Capsule())
                        .overlay(Capsule().strokeBorder(.white.opacity(0.22), lineWidth: 0.6))
                    }
                }
            }
            glassField("type a variant and press Return (e.g. nest js)",
                       text: Binding(get: { editor?.newAlias ?? "" }, set: { editor?.newAlias = $0 }),
                       onSubmit: commitAlias)

            if isNew, !engine.dictGroups.isEmpty {
                fieldLabel("Group")
                ChipFlow(spacing: 6) {
                    ForEach(engine.dictGroups, id: \.self) { g in
                        let selected = editor?.group == g
                        Button { editor?.group = g } label: {
                            Text(g)
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(selected ? Color.black : .white.opacity(0.85))
                                .padding(.horizontal, 11)
                                .frame(height: 26)
                                .background(selected ? Color.white : .white.opacity(0.1), in: Capsule())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }

            HStack(spacing: 10) {
                GlassPillButton(title: isNew ? "Add" : "Save", icon: "checkmark", prominent: true, action: saveEditor)
                GlassPillButton(title: "Cancel") {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { editor = nil }
                }
                Spacer()
            }
            .padding(.top, 2)
        }
        .glassCard(radius: 18, padding: 16, tint: Color.accentColor.opacity(0.18))
    }

    private func commitAlias() {
        guard var ed = editor else { return }
        // Poti lipi mai multe deodata, separate prin virgula.
        for part in ed.newAlias.split(separator: ",") {
            let a = part.trimmingCharacters(in: .whitespaces)
            if !a.isEmpty, !ed.aliases.contains(where: { $0.lowercased() == a.lowercased() }) {
                ed.aliases.append(a)
            }
        }
        ed.newAlias = ""
        editor = ed
    }

    private func saveEditor() {
        commitAlias()
        guard let ed = editor, !ed.correct.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        engine.lastError = nil
        engine.saveEntry(line: ed.line, expected: ed.expected, correct: ed.correct, aliases: ed.aliases,
                         group: ed.line == nil ? ed.group : nil)
        withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { editor = nil }
    }

    private func fieldLabel(_ text: String) -> some View {
        Text(text).font(.caption.weight(.medium)).foregroundStyle(.white.opacity(0.6))
    }

    private func glassField(_ placeholder: String, text: Binding<String>, onSubmit: @escaping () -> Void = {}) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain)
            .font(.system(size: 14))
            .onSubmit(onSubmit)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background(.black.opacity(0.28), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white.opacity(0.18), lineWidth: 0.7))
    }

    private func emptyState(_ title: String, _ subtitle: String) -> some View {
        VStack(spacing: 6) {
            Image(systemName: "character.book.closed").font(.system(size: 26)).foregroundStyle(.white.opacity(0.5))
            Text(title).font(.system(size: 15, weight: .semibold))
            Text(subtitle).font(.callout).foregroundStyle(.white.opacity(0.6)).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 18)
        .glassCard(radius: 18)
    }

    // MARK: Permisiuni

    @ViewBuilder
    private var permissionsTab: some View {
        sectionTitle("Permissions")
        permissionRow(
            "Accessibility", "To detect the dictation key and paste text.",
            icon: "keyboard.fill", color: .blue, ok: engine.accessibility && engine.keyboardReady
        ) { engine.requestPermission() }
        permissionRow(
            "Microphone", "Listens only while you hold the key. Everything stays on your Mac.",
            icon: "mic.fill", color: .red, ok: engine.microphone != nil
        ) { openURL("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone") }
        if let err = engine.lastError {
            note(err).foregroundStyle(.orange)
        } else {
            note("Voice Flow starts on its own as soon as you enable a permission; no restart needed.")
        }

        if !engine.configPath.isEmpty {
            sectionTitle("Files")
            row("Settings", engine.configPath, icon: "doc.text.fill", color: .gray) {
                pillButton("Show", prominent: false) {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: engine.configPath)])
                }
            }
            row("Log", "~/Library/Logs/VoiceFlow.log", icon: "list.bullet.rectangle.fill", color: .gray) {
                pillButton("Show", prominent: false) {
                    let log = FileManager.default.homeDirectoryForCurrentUser
                        .appendingPathComponent("Library/Logs/VoiceFlow.log")
                    NSWorkspace.shared.activateFileViewerSelecting([log])
                }
            }
        }
    }

    private func permissionRow(
        _ title: String, _ subtitle: String, icon: String, color: Color, ok: Bool,
        open: @escaping () -> Void
    ) -> some View {
        row(title, subtitle, icon: icon, color: color, tint: ok ? nil : .orange.opacity(0.25)) {
            if ok {
                Label("Enabled", systemImage: "checkmark.circle.fill")
                    .font(.callout.weight(.semibold))
                    .foregroundStyle(.green)
            } else {
                pillButton("Enable…", prominent: true, action: open)
            }
        }
    }

    // MARK: Piese

    private func sectionTitle(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 15, weight: .semibold))
            .padding(.leading, 4)
            .padding(.top, 14)
            .padding(.bottom, 2)
    }

    /// Un rand: card de sticla cu iconita, titlu, explicatie optionala si un control in dreapta.
    private func row<Trailing: View>(
        _ title: String, _ subtitle: String?, icon: String, color: Color, tint: Color? = nil,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(spacing: 12) {
            IconTile(systemName: icon, color: color, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .medium))
                if let subtitle {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.6))
                        .lineLimit(2)
                        .truncationMode(.middle)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 12)
            trailing()
        }
        .glassCard(radius: 16, padding: 12, tint: tint)
    }

    private func toggle(get: @escaping () -> Bool, set: @escaping (Bool) -> Void) -> some View {
        Toggle("", isOn: Binding(get: get, set: set))
            .labelsHidden()
            .toggleStyle(GlassToggleStyle())
            .tint(.green)
    }

    private func value(_ text: String) -> some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(.white.opacity(0.6))
            .lineLimit(1)
            .truncationMode(.middle)
    }

    private func pillButton(_ title: String, prominent: Bool, action: @escaping () -> Void) -> some View {
        GlassPillButton(title: title, prominent: prominent, action: action)
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(0.5))
            .multilineTextAlignment(.leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 4)
            .padding(.top, 2)
    }

    /// "http://127.0.0.1:8080/v1/chat/completions" -> "127.0.0.1:8080"
    private func serverShort(_ url: String) -> String {
        guard let u = URL(string: url), let host = u.host else { return url }
        return u.port.map { "\(host):\($0)" } ?? host
    }

    private func openInTextEdit(_ path: String) {
        let url = URL(fileURLWithPath: path)
        let textEdit = URL(fileURLWithPath: "/System/Applications/TextEdit.app")
        NSWorkspace.shared.open([url], withApplicationAt: textEdit, configuration: NSWorkspace.OpenConfiguration())
    }

    private func openURL(_ s: String) {
        if let url = URL(string: s) { NSWorkspace.shared.open(url) }
    }
}

/// Starea formularului de adaugare / modificare a unui termen.
struct DictEditor: Equatable {
    var line: Int?
    var expected = ""
    var correct = ""
    var aliases: [String] = []
    var group = ""
    var newAlias = ""
}

/// Aseaza etichete una dupa alta si trece pe randul urmator cand nu mai incap.
struct ChipFlow: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
        return CGSize(width: proposal.width ?? rows.width, height: rows.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrange(width: bounds.width, subviews: subviews)
        for (i, point) in result.points.enumerated() {
            subviews[i].place(at: CGPoint(x: bounds.minX + point.x, y: bounds.minY + point.y), proposal: .unspecified)
        }
    }

    private func arrange(width: CGFloat, subviews: Subviews) -> (points: [CGPoint], width: CGFloat, height: CGFloat) {
        var points: [CGPoint] = []
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0, maxX: CGFloat = 0
        for v in subviews {
            let size = v.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            maxX = max(maxX, x - spacing)
        }
        return (points, maxX, y + rowHeight)
    }
}

/// Face fereastra transparenta, ca sticla sa lase sa se vada ce e in spatele ei,
/// si ascunde bara de titlu (raman doar butoanele colorate).
struct GlassWindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { [weak view] in
            guard let window = view?.window else { return }
            window.isOpaque = false
            window.backgroundColor = .clear
            window.titlebarAppearsTransparent = true
            window.titleVisibility = .hidden
            window.styleMask.insert(.fullSizeContentView)
            window.isMovableByWindowBackground = true
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}
