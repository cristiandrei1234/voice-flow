import AppKit
import SwiftUI

/// Sectiunea "Notes": lista de notite sau, cand una e deschisa, editorul in care se poate dicta.
/// Motorul lipeste textul dictat in campul care are focusul (Cmd+V), deci e de ajuns ca editorul
/// de aici sa fie primul care raspunde la tastatura.
struct NotesView: View {
    private enum Field: Hashable {
        case title, body
    }

    @Environment(Engine.self) private var engine
    private let store = NotesStore.shared
    @FocusState private var focus: Field?
    @State private var copied = false
    @State private var pendingDelete: Note?
    @State private var pasteFallback = PasteShortcutFallback()

    var body: some View {
        @Bindable var store = store
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                GlassSearchField(placeholder: store.count == 0 ? "Search notes" : store.count == 1 ? "Search 1 note" : "Search \(store.count) notes",
                                 text: $store.search)
                GlassPillButton(title: "New note", icon: "plus", prominent: true) {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { store.requestNewNote() }
                }
            }

            if let message = store.errorMessage {
                ErrorBanner(message: message) { store.errorMessage = nil }
                if store.unsaved {
                    GlassPillButton(title: "Try saving again", icon: "arrow.clockwise") { store.flush() }
                }
            }

            if let note = store.openNote {
                editorCard(note)
            } else {
                list
            }
        }
        .confirmationDialog(
            "Delete “\(pendingDelete?.displayTitle ?? "")”?",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            presenting: pendingDelete
        ) { note in
            Button("Delete note", role: .destructive) {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { store.delete(note.id) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("The note will be permanently deleted. It can't be recovered.")
        }
        // Focusul se poate da abia dupa ce editorul e pe ecran.
        .task(id: store.focusRequest) {
            guard store.openID != nil else { return }
            try? await Task.sleep(for: .milliseconds(80))
            focus = .body
        }
        .onChange(of: store.search) { _, new in
            // Cautarea arata lista: nota deschisa se inchide (si se salveaza).
            if !new.isEmpty, store.openID != nil { store.close() }
        }
        .onChange(of: store.openID) { _, _ in copied = false }
        .onAppear { pasteFallback.start() }
        .onDisappear {
            pasteFallback.stop()
            store.flush()
        }
    }

    // MARK: Lista

    @ViewBuilder
    private var list: some View {
        let notes = store.notes
        if store.count == 0 {
            EmptyStateCard(
                icon: "note.text",
                title: "No notes yet",
                subtitle: "Dictate here without having a text field open in another app. Click “New note”, hold the dictation key and speak: the text stays saved in Voice Flow."
            )
            hotkeyHint
        } else if notes.isEmpty {
            EmptyStateCard(icon: "magnifyingglass", title: "No notes found",
                           subtitle: "Nothing matches “\(store.search)”.")
        } else {
            ForEach(notes) { note in
                noteCard(note)
            }
            NoteText("Notes stay only on this Mac, in the app's folder.")
                .padding(.top, 6)
        }
    }

    private func noteCard(_ note: Note) -> some View {
        Button {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { store.open(note.id) }
        } label: {
            VStack(alignment: .leading, spacing: 5) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    if note.pinned {
                        Image(systemName: "pin.fill")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(.orange)
                            .accessibilityLabel("Pinned")
                    }
                    Text(note.displayTitle)
                        .font(.system(size: 15, weight: .semibold))
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(NoteFormat.relative(note.updated))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.5))
                }
                let preview = note.preview
                if !preview.isEmpty {
                    Text(preview)
                        .font(.system(size: 13))
                        .foregroundStyle(.white.opacity(0.65))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .glassCard(radius: 16, padding: 13)
            .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button(note.pinned ? "Unpin" : "Pin to top") { store.togglePin(note.id) }
            Button("Delete…", role: .destructive) { pendingDelete = note }
        }
    }

    // MARK: Editor

    private func editorCard(_ note: Note) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                GlassIconButton(systemName: "chevron.left", size: 30, help: "Back to list") {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { store.close() }
                }
                Spacer()
                GlassPillButton(title: note.pinned ? "Pinned" : "Pin", icon: note.pinned ? "pin.fill" : "pin") {
                    store.togglePin(note.id)
                }
                .help(note.pinned ? "Unpin" : "Keep the note at the top of the list")
                GlassPillButton(title: copied ? "Copied" : "Copy all", icon: copied ? "checkmark" : "doc.on.doc") {
                    copyAll(note)
                }
                .disabled(note.body.isEmpty)
                GlassIconButton(systemName: "trash", size: 30, help: "Delete note") {
                    // O nota goala nu are ce pierde: dispare direct.
                    if note.isEmpty { store.delete(note.id) } else { pendingDelete = note }
                }
            }

            TextField(note.derivedTitle ?? "Title", text: Binding(get: { store.openNote?.title ?? "" }, set: { store.setTitle($0) }))
                .textFieldStyle(.plain)
                .font(.system(size: 18, weight: .semibold))
                .focused($focus, equals: .title)
                .onSubmit { focus = .body }
                .padding(.horizontal, 4)

            TextEditor(text: Binding(get: { store.openNote?.body ?? "" }, set: { store.setBody($0) }))
                .font(.system(size: 14))
                .lineSpacing(3)
                .scrollContentBackground(.hidden)
                .focused($focus, equals: .body)
                .padding(8)
                .frame(minHeight: 320)
                .background(.black.opacity(0.28), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(.white.opacity(focus == .body ? 0.34 : 0.18), lineWidth: 0.7)
                )
                .accessibilityLabel("Note text")

            hotkeyHint

            HStack(spacing: 6) {
                Text(NoteFormat.words(note.wordCount))
                Text("·")
                Text("edited \(NoteFormat.relative(note.updated))")
                Spacer()
                saveIndicator
            }
            .font(.caption)
            .foregroundStyle(.white.opacity(0.5))
            .padding(.horizontal, 4)
        }
        .glassCard(radius: 18, padding: 16)
    }

    @ViewBuilder
    private var saveIndicator: some View {
        if store.errorMessage != nil, store.unsaved {
            Label("not saved", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
        } else if store.unsaved {
            Label("saving…", systemImage: "arrow.triangle.2.circlepath")
        } else {
            Label("saved", systemImage: "checkmark.circle")
        }
    }

    /// Cum ajunge textul aici, cu tastele de dictare ale utilizatorului.
    private var hotkeyHint: some View {
        let settings = engine.settings
        let hold = settings?.hotkeyMode != "toggle"
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                ForEach(settings?.hotkeys ?? [], id: \.self) { key in
                    HotkeyView(hotkey: key, compact: true)
                }
                Text(hold
                    ? "Hold the dictation key and speak: the text appears here."
                    : "Press the dictation key and speak: the text appears here.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.6))
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            if settings?.paste == false {
                Label("Automatic pasting is off in General, so dictated text won't reach the note.",
                      systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 4)
    }

    private func copyAll(_ note: Note) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(note.body, forType: .string)
        copied = true
        let id = note.id
        Task {
            try? await Task.sleep(for: .seconds(1.6))
            if store.openID == id { copied = false }
        }
    }
}

/// Plasa de siguranta pentru Cmd+V: scurtaturile de editare vin in mod normal din meniul "Edit"
/// al aplicatiei. O aplicatie fara iconita in Dock poate ramane fara acel meniu, iar atunci
/// lipirea simulata de motor nu ar ajunge in editor. Cat timp ecranul de notite e vizibil si
/// meniul NU are deja o comanda pentru tasta respectiva, o trimitem noi campului cu focus.
@MainActor
final class PasteShortcutFallback {
    private var monitor: Any?

    private static let actions: [String: Selector] = [
        "v": #selector(NSText.paste(_:)),
        "c": #selector(NSText.copy(_:)),
        "x": #selector(NSText.cut(_:)),
        "a": #selector(NSText.selectAll(_:)),
    ]

    func start() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            MainActor.assumeIsolated { PasteShortcutFallback.handle(event) } ? nil : event
        }
    }

    func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    /// Intoarce `true` daca evenimentul a fost tratat aici.
    private static func handle(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.intersection([.command, .shift, .option, .control]) == .command,
              let key = event.charactersIgnoringModifiers?.lowercased(),
              let action = actions[key],
              event.window?.firstResponder is NSTextView,
              !menuHandles(key)
        else { return false }
        return NSApp.sendAction(action, to: nil, from: nil)
    }

    private static func menuHandles(_ key: String) -> Bool {
        func search(_ menu: NSMenu?) -> Bool {
            guard let menu else { return false }
            return menu.items.contains { item in
                (item.keyEquivalent.lowercased() == key && item.keyEquivalentModifierMask == .command)
                    || search(item.submenu)
            }
        }
        return search(NSApp.mainMenu)
    }
}
