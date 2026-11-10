import Foundation
import Observation

/// Un text salvat care apare cand rostesti declansatorul.
struct Snippet: Identifiable, Equatable {
    let id: String
    var trigger: String
    var variants: [String]
    var text: String
    var enabled: Bool

    init(id: String, trigger: String, variants: [String] = [], text: String, enabled: Bool = true) {
        self.id = id
        self.trigger = trigger
        self.variants = variants
        self.text = text
        self.enabled = enabled
    }

    init?(json: [String: Any]) {
        guard let id = json["id"] as? String, let trigger = json["trigger"] as? String,
              let text = json["text"] as? String else { return nil }
        self.init(id: id, trigger: trigger, variants: json["variants"] as? [String] ?? [], text: text,
                  enabled: json["enabled"] as? Bool ?? true)
    }
}

/// Starea ecranului Snippets. Lista vine de la motor (evenimentul `snippets`), care tine si fisierul.
@MainActor
@Observable
final class SnippetsStore {
    static let shared = SnippetsStore()

    var snippets: [Snippet] = []
    /// Eroarea ultimei comenzi trimise de aici (validare, fisier stricat).
    var error: String?

    @ObservationIgnored private weak var engine: Engine?
    /// O comanda a noastra asteapta raspuns: motorul trimite `error` (daca e cazul), apoi `snippets`.
    @ObservationIgnored private var waiting = false
    @ObservationIgnored private var failed = false
    @ObservationIgnored private var completion: ((Bool) -> Void)?

    func attach(_ engine: Engine) {
        self.engine = engine
        engine.on("snippets") { [weak self] obj in
            guard let self else { return }
            let list = obj["snippets"] as? [[String: Any]] ?? []
            self.snippets = list.compactMap(Snippet.init(json:))
            let done = self.completion
            let ok = !self.failed
            self.waiting = false
            self.failed = false
            self.completion = nil
            done?(ok)
        }
        engine.on("error") { [weak self, weak engine] obj in
            // Doar erorile venite ca raspuns la o comanda de-a noastra.
            guard let self, self.waiting, let message = obj["message"] as? String else { return }
            self.error = message
            self.failed = true
            // Altfel ar aparea si pe ecranul Dictionar, unde nu are legatura.
            if engine?.lastError == message { engine?.lastError = nil }
        }
    }

    private func send(_ command: [String: Any], then completion: ((Bool) -> Void)? = nil) {
        waiting = true
        failed = false
        self.completion = completion
        engine?.send(command)
    }

    /// Reciteste fisierul (prinde si ce a fost editat de mana).
    func reload() {
        send(["cmd": "list_snippets"])
    }

    /// Adauga (`id` nil) sau modifica un snippet. `then(true)` daca motorul l-a acceptat.
    func save(id: String?, trigger: String, variants: [String], text: String, enabled: Bool,
              then completion: ((Bool) -> Void)? = nil) {
        error = nil
        var command: [String: Any] = [
            "cmd": "save_snippet", "trigger": trigger, "variants": variants, "text": text, "enabled": enabled,
        ]
        if let id { command["id"] = id }
        send(command, then: completion)
    }

    func setEnabled(_ snippet: Snippet, _ enabled: Bool) {
        // Schimbam pe loc, ca sa nu sara comutatorul; motorul confirma cu lista lui.
        if let i = snippets.firstIndex(where: { $0.id == snippet.id }) { snippets[i].enabled = enabled }
        save(id: snippet.id, trigger: snippet.trigger, variants: snippet.variants, text: snippet.text, enabled: enabled)
    }

    func delete(_ snippet: Snippet) {
        error = nil
        send(["cmd": "delete_snippet", "id": snippet.id])
    }
}
