import AppKit
import Foundation
import Observation

// Notite: un loc in Voice Flow unde dictezi fara sa ai un camp de text deschis in alta aplicatie.
// `NotesLibrary` e logica simpla (fisier, CRUD, sortare, cautare), fara interfata, testabila pe un
// folder temporar. `NotesStore` e starea ecranului: nota deschisa, salvarea cu intarziere, erorile.

/// O notita.
struct Note: Codable, Equatable, Identifiable {
    var id: UUID
    /// Titlul scris de utilizator; gol inseamna "ia-l din prima linie".
    var title: String
    var body: String
    var created: Date
    var updated: Date
    var pinned: Bool

    static let untitled = "Untitled note"
    private static let titleLimit = 60

    /// Prima linie cu text din corp, scurtata.
    var derivedTitle: String? {
        guard let line = Note.firstLine(of: body) else { return nil }
        return line.count > Note.titleLimit ? String(line.prefix(Note.titleLimit)).trimmingCharacters(in: .whitespaces) + "…" : line
    }

    /// Titlul afisat: cel scris, altfel prima linie, altfel un text generic.
    var displayTitle: String {
        let own = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if !own.isEmpty { return own }
        return derivedTitle ?? Note.untitled
    }

    /// Textul de sub titlu in lista, pe o singura linie. Cand titlul vine din prima linie, nu o repetam.
    var preview: String {
        var lines = body.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        if title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, !lines.isEmpty {
            let first = lines.removeFirst()
            // Prima linie prea lunga pentru titlu: restul ei ramane vizibil in previzualizare.
            if first.count > Note.titleLimit { lines.insert(first, at: 0) }
        }
        return lines.joined(separator: " ")
    }

    var wordCount: Int { Note.wordCount(body) }

    /// Fara titlu si fara text: o astfel de nota nu se pastreaza.
    var isEmpty: Bool {
        title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    static func firstLine(of text: String) -> String? {
        text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty }
    }

    static func wordCount(_ text: String) -> Int {
        text.split(whereSeparator: { $0.isWhitespace || $0.isNewline }).count
    }
}

/// De ce nu s-au putut citi sau scrie notitele. `message` e textul aratat utilizatorului.
enum NotesError: Error, Equatable {
    /// Fisierul exista, dar nu a putut fi citit. Nu scriem peste el.
    case unreadable(String)
    /// Fisierul nu era JSON valid; a fost pus deoparte sub numele dat si pornim de la zero.
    case corrupt(backup: String)
    /// Fisierul nu era valid si nici nu a putut fi pus deoparte. Nu scriem peste el.
    case corruptInPlace
    /// Fisier scris de o versiune mai noua a aplicatiei. Nu scriem peste el.
    case newerFormat
    case writeFailed(String)

    var message: String {
        switch self {
        case .unreadable(let why):
            "Notes couldn't be read (\(why)). What you write now stays only in memory until this is fixed."
        case .corrupt(let backup):
            "The notes file was damaged and has been kept as “\(backup)”. Starting with an empty list."
        case .corruptInPlace:
            "The notes file is damaged and couldn't be moved. What you write now stays only in memory."
        case .newerFormat:
            "Notes were saved by a newer version of the app. What you write now stays only in memory."
        case .writeFailed(let why):
            "Notes couldn't be saved (\(why)). The text stays in memory; don't quit the app."
        }
    }
}

/// Notitele de pe disc: un singur fisier JSON (`notes.json`) intr-un folder al aplicatiei.
struct NotesLibrary {
    static let fileName = "notes.json"
    static let formatVersion = 1

    /// `~/Library/Application Support/VoiceFlow/notes/`
    static var defaultDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support")
        return base.appendingPathComponent("VoiceFlow/notes", isDirectory: true)
    }

    let directory: URL
    private(set) var notes: [Note] = []
    /// Adevarat cand pe disc e ceva ce nu am putut citi: salvarea ar distruge acel continut.
    private(set) var writeBlocked = false
    private let now: () -> Date

    private struct Document: Codable {
        var version: Int
        var notes: [Note]
    }

    init(directory: URL, now: @escaping () -> Date = Date.init) {
        self.directory = directory
        self.now = now
    }

    var fileURL: URL { directory.appendingPathComponent(NotesLibrary.fileName) }

    // MARK: Disc

    /// Citeste notitele. Daca fisierul lipseste, lista e goala (nu e o eroare).
    mutating func load() throws {
        let fm = FileManager.default
        notes = []
        writeBlocked = false
        guard fm.fileExists(atPath: fileURL.path) else { return }

        let data: Data
        do {
            data = try Data(contentsOf: fileURL)
        } catch {
            writeBlocked = true
            throw NotesError.unreadable(error.localizedDescription)
        }

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        guard let doc = try? decoder.decode(Document.self, from: data) else {
            // Nu stergem nimic: fisierul stricat ramane alaturi, sub alt nume.
            let stamp = Int(now().timeIntervalSince1970)
            let backup = directory.appendingPathComponent("notes.corrupt-\(stamp).json")
            do {
                try fm.moveItem(at: fileURL, to: backup)
            } catch {
                writeBlocked = true
                throw NotesError.corruptInPlace
            }
            throw NotesError.corrupt(backup: backup.lastPathComponent)
        }
        guard doc.version <= NotesLibrary.formatVersion else {
            writeBlocked = true
            throw NotesError.newerFormat
        }
        notes = doc.notes
    }

    /// Scrie toate notitele: fisier temporar in acelasi folder, apoi redenumire peste cel vechi.
    /// Notele complet goale nu se scriu.
    func save() throws {
        guard !writeBlocked else {
            throw NotesError.writeFailed("the existing file couldn't be read")
        }
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let fm = FileManager.default
        let temp = directory.appendingPathComponent(".notes-\(UUID().uuidString).tmp")
        do {
            let doc = Document(version: NotesLibrary.formatVersion, notes: notes.filter { !$0.isEmpty })
            let data = try encoder.encode(doc)
            try fm.createDirectory(at: directory, withIntermediateDirectories: true)
            try data.write(to: temp)
            // rename(2) inlocuieste destinatia dintr-o singura miscare.
            guard rename(temp.path, fileURL.path) == 0 else {
                throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno),
                              userInfo: [NSLocalizedDescriptionKey: String(cString: strerror(errno))])
            }
        } catch {
            try? fm.removeItem(at: temp)
            throw NotesError.writeFailed(error.localizedDescription)
        }
    }

    // MARK: CRUD

    func note(_ id: UUID) -> Note? { notes.first { $0.id == id } }

    @discardableResult
    mutating func create(title: String = "", body: String = "") -> Note {
        let stamp = now()
        let note = Note(id: UUID(), title: title, body: body, created: stamp, updated: stamp, pinned: false)
        notes.append(note)
        return note
    }

    /// Schimba titlul si / sau corpul. Intoarce `true` doar daca s-a schimbat ceva.
    @discardableResult
    mutating func update(_ id: UUID, title: String? = nil, body: String? = nil) -> Bool {
        guard let i = notes.firstIndex(where: { $0.id == id }) else { return false }
        var note = notes[i]
        if let title { note.title = title }
        if let body { note.body = body }
        guard note != notes[i] else { return false }
        note.updated = now()
        notes[i] = note
        return true
    }

    /// Fixarea nu schimba data modificarii: nota ramane la locul ei intre cele fixate / nefixate.
    @discardableResult
    mutating func setPinned(_ id: UUID, _ pinned: Bool) -> Bool {
        guard let i = notes.firstIndex(where: { $0.id == id }), notes[i].pinned != pinned else { return false }
        notes[i].pinned = pinned
        return true
    }

    @discardableResult
    mutating func delete(_ id: UUID) -> Bool {
        guard let i = notes.firstIndex(where: { $0.id == id }) else { return false }
        notes.remove(at: i)
        return true
    }

    // MARK: Sortare si cautare

    /// Cele fixate primele, apoi cele modificate cel mai recent.
    var sorted: [Note] {
        notes.sorted { a, b in
            if a.pinned != b.pinned { return a.pinned }
            if a.updated != b.updated { return a.updated > b.updated }
            if a.created != b.created { return a.created > b.created }
            return a.id.uuidString < b.id.uuidString
        }
    }

    /// Cauta in titlu si in text, fara sa conteze majusculele sau diacriticele.
    /// Toate cuvintele cautate trebuie sa apara. Cautarea goala intoarce tot.
    func search(_ query: String) -> [Note] {
        let terms = NotesLibrary.fold(query).split(whereSeparator: \.isWhitespace)
        guard !terms.isEmpty else { return sorted }
        return sorted.filter { note in
            let haystack = NotesLibrary.fold(note.title + "\n" + note.body)
            return terms.allSatisfy { haystack.contains($0) }
        }
    }

    static func fold(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "ro_RO"))
    }
}

/// Texte mici pentru ecranul de notite.
enum NoteFormat {
    /// "just now", "5 minutes ago", "yesterday", "3 days ago".
    static func relative(_ date: Date, now: Date = Date()) -> String {
        if now.timeIntervalSince(date) < 60 { return "just now" }
        let formatter = RelativeDateTimeFormatter()
        formatter.locale = Locale(identifier: "en_US")
        formatter.unitsStyle = .full
        formatter.dateTimeStyle = .named
        return formatter.localizedString(for: date, relativeTo: now)
    }

    /// "1 word", "2 words", "20 words".
    static func words(_ count: Int) -> String {
        count == 1 ? "1 word" : "\(count) words"
    }
}

/// Starea ecranului de notite.
@MainActor
@Observable
final class NotesStore {
    static let shared = NotesStore()
    static let sectionID = "notes"

    private(set) var library: NotesLibrary
    /// Nota deschisa in editor; `nil` inseamna lista.
    private(set) var openID: UUID?
    var search = ""
    /// Sunt schimbari care nu au ajuns inca pe disc.
    private(set) var unsaved = false
    /// Mesajul de eroare de la ultima citire / salvare nereusita.
    var errorMessage: String?
    /// Creste de fiecare data cand editorul trebuie sa primeasca focusul.
    private(set) var focusRequest = 0

    @ObservationIgnored private let debounce: Duration
    @ObservationIgnored private var saveTask: Task<Void, Never>?
    @ObservationIgnored private weak var engine: Engine?
    @ObservationIgnored private var observing = false

    init(directory: URL = NotesLibrary.defaultDirectory, debounce: Duration = .milliseconds(700)) {
        self.debounce = debounce
        library = NotesLibrary(directory: directory)
        do {
            try library.load()
        } catch {
            errorMessage = (error as? NotesError)?.message ?? error.localizedDescription
        }
    }

    /// Apelat din `Features.install`: salveaza la inchiderea ferestrei si a aplicatiei.
    func attach(_ engine: Engine) {
        self.engine = engine
        guard !observing else { return }
        observing = true
        let names: [Notification.Name] = [
            NSApplication.willTerminateNotification,
            NSApplication.didResignActiveNotification,
            NSWindow.willCloseNotification,
        ]
        for name in names {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { _ in
                // Sincron: la iesirea din aplicatie nu mai apuca sa ruleze un Task.
                MainActor.assumeIsolated { NotesStore.shared.flush() }
            }
        }
    }

    // MARK: Ce se afiseaza

    var notes: [Note] { library.search(search) }
    var count: Int { library.notes.count }
    var openNote: Note? { openID.flatMap { library.note($0) } }

    // MARK: Actiuni

    /// Deschide o nota noua si cere focus pe editor, ca utilizatorul sa poata dicta imediat.
    /// Seteaza si `engine.requestedSettingsTab`, deci fereastra (odata deschisa) sare la "Notes".
    /// Pentru un rand din meniu: `NotesStore.shared.requestNewNote()`, apoi
    /// `NSApp.activate(ignoringOtherApps: true)` si `openWindow(id: "settings")`.
    func requestNewNote() {
        search = ""
        // O nota goala deja deschisa se refoloseste, ca apasarile repetate sa nu lase note goale.
        if openNote?.isEmpty != true {
            close()
            openID = library.create().id
        }
        focusRequest += 1
        engine?.requestedSettingsTab = NotesStore.sectionID
    }

    func open(_ id: UUID) {
        guard library.note(id) != nil else { return }
        if openID != id { close() }
        openID = id
        focusRequest += 1
    }

    /// Inapoi la lista. O nota ramasa goala dispare; restul se salveaza pe loc.
    func close() {
        if let note = openNote, note.isEmpty { library.delete(note.id) }
        openID = nil
        flush()
    }

    func setTitle(_ title: String) {
        guard let id = openID, library.update(id, title: title) else { return }
        scheduleSave()
    }

    func setBody(_ body: String) {
        guard let id = openID, library.update(id, body: body) else { return }
        scheduleSave()
    }

    func togglePin(_ id: UUID) {
        guard let note = library.note(id), library.setPinned(id, !note.pinned) else { return }
        unsaved = true
        flush()
    }

    func delete(_ id: UUID) {
        guard library.delete(id) else { return }
        if openID == id { openID = nil }
        unsaved = true
        flush()
    }

    // MARK: Salvare

    private func scheduleSave() {
        unsaved = true
        saveTask?.cancel()
        saveTask = Task { [weak self, debounce] in
            try? await Task.sleep(for: debounce)
            guard !Task.isCancelled else { return }
            self?.flush()
        }
    }

    /// Scrie acum ce nu e salvat. Daca nu reuseste, textul ramane in memorie si `unsaved` ramane adevarat.
    func flush() {
        saveTask?.cancel()
        saveTask = nil
        guard unsaved else { return }
        do {
            try library.save()
            unsaved = false
            errorMessage = nil
        } catch {
            errorMessage = (error as? NotesError)?.message ?? error.localizedDescription
        }
    }
}
