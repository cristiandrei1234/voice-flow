import SwiftUI

/// Starea formularului de adaugare / modificare a unui snippet.
struct SnippetEditor: Equatable {
    var id: String?
    var trigger = ""
    var variants: [String] = []
    var text = ""
    var enabled = true
    var newVariant = ""
}

/// Ecranul Snippets: texte salvate care apar cand rostesti declansatorul. Seamana cu ecranul Dictionar.
struct SnippetsView: View {
    @State private var store = SnippetsStore.shared
    @State private var search = ""
    @State private var editor: SnippetEditor?
    @State private var pendingDelete: Snippet?

    private let spring = Animation.spring(response: 0.3, dampingFraction: 0.85)

    private var filtered: [Snippet] {
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return store.snippets }
        return store.snippets.filter { s in
            s.trigger.lowercased().contains(q) || s.text.lowercased().contains(q)
                || s.variants.contains { $0.lowercased().contains(q) }
        }
    }

    var body: some View {
        Group {
            HStack(spacing: 10) {
                GlassSearchField(placeholder: store.snippets.count == 1 ? "Search 1 snippet" : "Search \(store.snippets.count) snippets", text: $search)
                GlassPillButton(title: "New snippet", icon: "plus", prominent: true) {
                    withAnimation(spring) { editor = SnippetEditor() }
                }
            }

            if let error = store.error {
                ErrorBanner(message: error) { store.error = nil }
            }

            if editor != nil, editor?.id == nil {
                editorCard
            }

            if store.snippets.isEmpty {
                if editor == nil {
                    EmptyStateCard(
                        icon: "text.quote",
                        title: "No snippets yet",
                        subtitle: "Save a text you write often and give it a spoken name. "
                            + "For example, you say “my signature” and your email signature appears, on several lines."
                    )
                }
            } else if filtered.isEmpty {
                EmptyStateCard(icon: "text.quote", title: "No snippets found",
                               subtitle: "Nothing matches “\(search)”.")
            }

            ForEach(filtered) { snippet in
                if editor?.id == snippet.id {
                    editorCard
                } else {
                    card(snippet)
                }
            }

            NoteText("Say the trigger anywhere in a dictation and it's replaced with the saved text. "
                + "If you say only the trigger, the exact text appears, with no capital letter or period added.")
                .padding(.top, 10)
        }
        .onAppear { store.reload() }
        .confirmationDialog(
            "Delete the snippet “\(pendingDelete?.trigger ?? "")”?",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            presenting: pendingDelete
        ) { snippet in
            Button("Delete", role: .destructive) { store.delete(snippet) }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text("The saved text will be gone and can't be recovered.")
        }
    }

    private func card(_ snippet: Snippet) -> some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 8) {
                ChipFlow(spacing: 6) {
                    Text("say")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.5))
                        .frame(height: 22)
                    Chip(snippet.trigger)
                    ForEach(snippet.variants, id: \.self) { variant in
                        Chip(variant).opacity(0.6)
                    }
                }
                Text(snippet.text)
                    .font(.system(size: 13.5))
                    .foregroundStyle(.white.opacity(0.85))
                    .lineLimit(3)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .opacity(snippet.enabled ? 1 : 0.45)
            Spacer(minLength: 8)
            GlassSwitch(isOn: Binding(get: { snippet.enabled }, set: { store.setEnabled(snippet, $0) }))
                .help(snippet.enabled ? "On" : "Off: the trigger stays as plain text")
            GlassIconButton(systemName: "pencil", size: 30, help: "Edit") {
                withAnimation(spring) {
                    editor = SnippetEditor(id: snippet.id, trigger: snippet.trigger, variants: snippet.variants,
                                           text: snippet.text, enabled: snippet.enabled)
                }
            }
            GlassIconButton(systemName: "trash", size: 30, help: "Delete") { pendingDelete = snippet }
        }
        .glassCard(radius: 16, padding: 13)
    }

    /// Formularul de adaugare / modificare, in locul cardului.
    private var editorCard: some View {
        let isNew = editor?.id == nil
        return VStack(alignment: .leading, spacing: 12) {
            Text(isNew ? "New snippet" : "Edit snippet")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white.opacity(0.7))

            FieldLabel("What you say (the trigger)")
            GlassField(placeholder: "e.g. my signature",
                       text: Binding(get: { editor?.trigger ?? "" }, set: { editor?.trigger = $0 }))

            FieldLabel("Other ways the model hears it (optional)")
            if let variants = editor?.variants, !variants.isEmpty {
                ChipFlow(spacing: 6) {
                    ForEach(variants, id: \.self) { variant in
                        HStack(spacing: 5) {
                            Text(variant).font(.system(size: 12))
                            Button {
                                editor?.variants.removeAll { $0 == variant }
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
            GlassField(placeholder: "type a variant and press Return (e.g. my signatures)",
                       text: Binding(get: { editor?.newVariant ?? "" }, set: { editor?.newVariant = $0 }),
                       onSubmit: commitVariant)

            FieldLabel("Text that appears")
            GlassTextEditor(text: Binding(get: { editor?.text ?? "" }, set: { editor?.text = $0 }), minHeight: 110)

            HStack(spacing: 10) {
                GlassPillButton(title: isNew ? "Add" : "Save", icon: "checkmark", prominent: true, action: save)
                GlassPillButton(title: "Cancel") {
                    store.error = nil
                    withAnimation(spring) { editor = nil }
                }
                Spacer()
            }
            .padding(.top, 2)
        }
        .glassCard(radius: 18, padding: 16, tint: Color.accentColor.opacity(0.18))
    }

    private func commitVariant() {
        guard var ed = editor else { return }
        // Poti lipi mai multe deodata, separate prin virgula.
        for part in ed.newVariant.split(separator: ",") {
            let v = part.trimmingCharacters(in: .whitespaces)
            if !v.isEmpty, !ed.variants.contains(where: { $0.lowercased() == v.lowercased() }) {
                ed.variants.append(v)
            }
        }
        ed.newVariant = ""
        editor = ed
    }

    private func save() {
        commitVariant()
        guard let ed = editor else { return }
        // Formularul ramane deschis pana confirma motorul: la o eroare de validare nu pierzi textul scris.
        store.save(id: ed.id, trigger: ed.trigger, variants: ed.variants, text: ed.text, enabled: ed.enabled) { ok in
            guard ok, editor?.id == ed.id else { return }
            withAnimation(spring) { editor = nil }
        }
    }
}
