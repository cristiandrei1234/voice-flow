import SwiftUI

/// O sectiune din fereastra Voice Flow (bara laterala din stanga).
struct AppSection: Identifiable {
    let id: String
    let title: String
    let icon: String
    let view: () -> AnyView

    init<V: View>(id: String, title: String, icon: String, @ViewBuilder view: @escaping () -> V) {
        self.id = id
        self.title = title
        self.icon = icon
        self.view = { AnyView(view()) }
    }
}

/// Registrul functiilor optionale. Fiecare functie noua sta in fisierele ei si adauga AICI
/// cate o linie in `install` (daca are stare proprie) si una in `sections` (daca are ecran).
///
/// Tiparul unei functii:
///   - `XStore.swift`: `@MainActor @Observable final class XStore { static let shared = XStore() ... }`
///     cu `func attach(_ engine: Engine)` care apeleaza `engine.on("tip_eveniment") { obj in ... }`
///     si trimite comenzi cu `engine.send(["cmd": "..."])`.
///   - `XView.swift`: ecranul, construit din piesele din Components.swift si Glass.swift.
@MainActor
enum Features {
    /// Apelat o singura data, inainte de pornirea motorului.
    static func install(engine: Engine) {
        NotesStore.shared.attach(engine)
        SnippetsStore.shared.attach(engine)
        HistoryStore.shared.attach(engine)
        DictationStore.shared.attach(engine)
        StylesStore.shared.attach(engine)
        CommandModeStore.shared.attach(engine)
    }

    /// Sectiunile functiilor optionale. Ordinea din bara laterala e in `SettingsView.sidebarOrder`.
    static var sections: [AppSection] {
        [
            AppSection(id: NotesStore.sectionID, title: "Notes", icon: "note.text") { NotesView() },
            AppSection(id: "snippets", title: "Snippets", icon: "text.quote") { SnippetsView() },
            AppSection(id: "history", title: "History", icon: "clock.arrow.circlepath") { HistoryView() },
            AppSection(id: "dictation", title: "Dictation", icon: "mic.badge.plus") { DictationView() },
            AppSection(id: "styles", title: "Styles", icon: "paintbrush.pointed.fill") { StylesView() },
            AppSection(id: "command", title: "Commands", icon: "text.badge.checkmark") { CommandModeView() },
        ]
    }
}
