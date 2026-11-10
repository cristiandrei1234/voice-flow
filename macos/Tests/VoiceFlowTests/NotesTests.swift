import XCTest
@testable import VoiceFlow

// Toate testele lucreaza intr-un folder temporar; `NotesStore.shared` (folderul real) nu e atins.

/// Ceas controlat: fiecare citire avanseaza cu un pas.
private final class Clock {
    var current: Date
    var step: TimeInterval
    init(_ start: TimeInterval = 1_700_000_000, step: TimeInterval = 10) {
        current = Date(timeIntervalSince1970: start)
        self.step = step
    }
    func tick() -> Date {
        defer { current = current.addingTimeInterval(step) }
        return current
    }
}

final class NotesLibraryTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("vf-notes-\(UUID().uuidString)")
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: dir)
    }

    private func library(_ clock: Clock = Clock()) -> NotesLibrary {
        NotesLibrary(directory: dir, now: clock.tick)
    }

    func testMissingFileLoadsEmptyAndCreatesNothing() throws {
        var lib = library()
        try lib.load()
        XCTAssertTrue(lib.notes.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.path), "citirea nu trebuie sa creeze folderul")
    }

    func testRoundTrip() throws {
        var lib = library()
        let a = lib.create(title: "Listă", body: "lapte\npâine")
        let b = lib.create(body: "Idee de proiect\ncu șțăîâ și emoji 🙂")
        lib.setPinned(b.id, true)
        try lib.save()

        var again = library()
        try again.load()
        XCTAssertEqual(again.notes.count, 2)
        XCTAssertEqual(again.note(a.id), lib.note(a.id))
        XCTAssertEqual(again.note(b.id), lib.note(b.id))
        XCTAssertEqual(again.note(b.id)?.pinned, true)
    }

    func testSaveLeavesOnlyTheNotesFile() throws {
        var lib = library()
        lib.create(body: "ceva")
        try lib.save()
        lib.create(body: "altceva")
        try lib.save()
        let files = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        XCTAssertEqual(files, [NotesLibrary.fileName], "fisierul temporar trebuie sa dispara dupa redenumire")
    }

    func testEmptyNotesAreNotWritten() throws {
        var lib = library()
        lib.create()
        lib.create(title: "  ", body: "\n \n")
        let kept = lib.create(body: "text")
        try lib.save()
        var again = library()
        try again.load()
        XCTAssertEqual(again.notes.map(\.id), [kept.id])
    }

    func testUpdateTouchesTimestampOnlyOnChange() throws {
        var lib = library()
        let note = lib.create(body: "unu")
        XCTAssertFalse(lib.update(note.id, body: "unu"))
        XCTAssertEqual(lib.note(note.id)?.updated, note.updated)
        XCTAssertTrue(lib.update(note.id, body: "unu doi"))
        let changed = try XCTUnwrap(lib.note(note.id))
        XCTAssertEqual(changed.body, "unu doi")
        XCTAssertGreaterThan(changed.updated, note.updated)
        XCTAssertEqual(changed.created, note.created)
        XCTAssertTrue(lib.update(note.id, title: "Titlu"))
        XCTAssertEqual(lib.note(note.id)?.body, "unu doi")
        XCTAssertFalse(lib.update(UUID(), body: "x"))
    }

    func testDelete() {
        var lib = library()
        let a = lib.create(body: "a")
        let b = lib.create(body: "b")
        XCTAssertTrue(lib.delete(a.id))
        XCTAssertFalse(lib.delete(a.id))
        XCTAssertEqual(lib.notes.map(\.id), [b.id])
    }

    func testSortPinnedFirstThenNewest() {
        var lib = library()
        let old = lib.create(body: "vechi")
        let pinnedOld = lib.create(body: "fixat vechi")
        let mid = lib.create(body: "mijloc")
        let pinnedNew = lib.create(body: "fixat nou")
        let new = lib.create(body: "nou")
        lib.setPinned(pinnedOld.id, true)
        lib.setPinned(pinnedNew.id, true)
        XCTAssertEqual(lib.sorted.map(\.id), [pinnedNew.id, pinnedOld.id, new.id, mid.id, old.id])
        // O nota veche modificata urca prima intre cele nefixate; fixarea nu schimba data.
        lib.update(old.id, body: "vechi, dar modificat")
        XCTAssertEqual(lib.sorted.map(\.id), [pinnedNew.id, pinnedOld.id, old.id, new.id, mid.id])
        XCTAssertFalse(lib.setPinned(pinnedNew.id, true))
        lib.setPinned(pinnedNew.id, false)
        XCTAssertEqual(lib.sorted.map(\.id), [pinnedOld.id, old.id, new.id, pinnedNew.id, mid.id])
    }

    func testSearchIgnoresCaseAndDiacritics() {
        var lib = library()
        let a = lib.create(title: "Ședință", body: "Discutăm bugetul pe mâine")
        let b = lib.create(body: "Cumpărături: lapte, pâine")
        XCTAssertEqual(lib.search("sedinta").map(\.id), [a.id])
        XCTAssertEqual(lib.search("PAINE").map(\.id), [b.id])
        XCTAssertEqual(lib.search("bugetul  maine").map(\.id), [a.id], "toate cuvintele trebuie sa apara")
        XCTAssertEqual(lib.search("bugetul lapte"), [])
        XCTAssertEqual(lib.search("   ").map(\.id), lib.sorted.map(\.id))
    }

    func testCorruptFileIsKeptAside() throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let garbage = Data("{ nu e json".utf8)
        try garbage.write(to: dir.appendingPathComponent(NotesLibrary.fileName))

        var lib = library()
        XCTAssertThrowsError(try lib.load()) { error in
            guard case NotesError.corrupt(let backup) = error else { return XCTFail("\(error)") }
            let kept = try? Data(contentsOf: self.dir.appendingPathComponent(backup))
            XCTAssertEqual(kept, garbage, "continutul vechi trebuie pastrat")
        }
        XCTAssertFalse(lib.writeBlocked)
        lib.create(body: "nou")
        try lib.save()
        var again = library()
        try again.load()
        XCTAssertEqual(again.notes.map(\.body), ["nou"])
    }

    func testNewerFormatIsNeverOverwritten() throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let newer = Data(#"{"version": 99, "notes": []}"#.utf8)
        let file = dir.appendingPathComponent(NotesLibrary.fileName)
        try newer.write(to: file)

        var lib = library()
        XCTAssertThrowsError(try lib.load()) { XCTAssertEqual($0 as? NotesError, .newerFormat) }
        XCTAssertTrue(lib.writeBlocked)
        lib.create(body: "text")
        XCTAssertThrowsError(try lib.save())
        XCTAssertEqual(try Data(contentsOf: file), newer)
        XCTAssertEqual(lib.notes.count, 1, "textul ramane in memorie")
    }

    func testSaveFailureReportsErrorAndKeepsNotes() throws {
        // Folderul de notite nu poate fi creat: in locul parintelui e un fisier.
        try Data("x".utf8).write(to: dir)
        var lib = NotesLibrary(directory: dir.appendingPathComponent("notes"))
        lib.create(body: "text")
        XCTAssertThrowsError(try lib.save()) { error in
            guard case NotesError.writeFailed = error else { return XCTFail("\(error)") }
        }
        XCTAssertEqual(lib.notes.map(\.body), ["text"])
    }
}

final class NoteTests: XCTestCase {
    private func note(title: String = "", body: String = "") -> Note {
        Note(id: UUID(), title: title, body: body, created: Date(), updated: Date(), pinned: false)
    }

    func testTitleDerivation() {
        XCTAssertEqual(note(title: "  Al meu ", body: "altceva").displayTitle, "Al meu")
        XCTAssertEqual(note(body: "\n\n   Prima linie  \nA doua").displayTitle, "Prima linie")
        XCTAssertEqual(note(body: "  \n ").displayTitle, Note.untitled)
        let long = String(repeating: "cuvânt ", count: 30)
        let title = note(body: long).displayTitle
        XCTAssertTrue(title.hasSuffix("…"))
        XCTAssertLessThanOrEqual(title.count, 61)
    }

    func testPreview() {
        XCTAssertEqual(note(body: "Prima\n\nA doua\n  A treia ").preview, "A doua A treia")
        XCTAssertEqual(note(title: "Titlu", body: "Prima\nA doua").preview, "Prima A doua")
        XCTAssertEqual(note(body: "Doar o linie").preview, "")
        let long = String(repeating: "cuvânt ", count: 30)
        XCTAssertTrue(note(body: long).preview.hasPrefix("cuvânt cuvânt"), "o prima linie lunga ramane in previzualizare")
    }

    func testEmptyAndWordCount() {
        XCTAssertTrue(note(title: " ", body: "\n").isEmpty)
        XCTAssertFalse(note(title: "x").isEmpty)
        XCTAssertFalse(note(body: "x").isEmpty)
        XCTAssertEqual(note(body: "").wordCount, 0)
        XCTAssertEqual(note(body: "  unu doi\n\ntrei\tpatru  ").wordCount, 4)
    }

    func testWordsLabel() {
        XCTAssertEqual(NoteFormat.words(0), "0 words")
        XCTAssertEqual(NoteFormat.words(1), "1 word")
        XCTAssertEqual(NoteFormat.words(2), "2 words")
        XCTAssertEqual(NoteFormat.words(19), "19 words")
        XCTAssertEqual(NoteFormat.words(20), "20 words")
        XCTAssertEqual(NoteFormat.words(100), "100 words")
        XCTAssertEqual(NoteFormat.words(101), "101 words")
        XCTAssertEqual(NoteFormat.words(120), "120 words")
    }

    func testRelativeDateInEnglish() {
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        XCTAssertEqual(NoteFormat.relative(now.addingTimeInterval(-20), now: now), "just now")
        XCTAssertEqual(NoteFormat.relative(now.addingTimeInterval(-5 * 60), now: now), "5 minutes ago")
        XCTAssertEqual(NoteFormat.relative(now.addingTimeInterval(-2 * 3600), now: now), "2 hours ago")
    }
}

@MainActor
final class NotesStoreTests: XCTestCase {
    private var dir: URL!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("vf-notes-\(UUID().uuidString)")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
    }

    private func saved(in folder: URL? = nil) throws -> [Note] {
        var lib = NotesLibrary(directory: folder ?? dir)
        try lib.load()
        return lib.sorted
    }

    func testNewNoteOpensAndAsksForFocus() {
        let store = NotesStore(directory: dir, debounce: .seconds(60))
        store.search = "ceva"
        store.requestNewNote()
        XCTAssertNotNil(store.openNote)
        XCTAssertEqual(store.search, "")
        XCTAssertEqual(store.focusRequest, 1)
        // A doua cerere refoloseste nota goala.
        let first = store.openID
        store.requestNewNote()
        XCTAssertEqual(store.openID, first)
        XCTAssertEqual(store.count, 1)
        XCTAssertEqual(store.focusRequest, 2)
    }

    func testTypingIsSavedOnFlushAndEmptyNoteIsDropped() throws {
        let store = NotesStore(directory: dir, debounce: .seconds(60))
        store.requestNewNote()
        store.close()
        XCTAssertEqual(store.count, 0, "o nota ramasa goala dispare")
        XCTAssertFalse(FileManager.default.fileExists(atPath: dir.path))

        store.requestNewNote()
        store.setBody("text dictat")
        XCTAssertTrue(store.unsaved)
        store.flush()
        XCTAssertFalse(store.unsaved)
        XCTAssertEqual(try saved().map(\.body), ["text dictat"])

        store.setTitle("Titlu")
        store.close()
        XCTAssertNil(store.openID)
        XCTAssertEqual(try saved().map(\.title), ["Titlu"])
    }

    func testDebouncedAutosave() async throws {
        let store = NotesStore(directory: dir, debounce: .milliseconds(30))
        store.requestNewNote()
        store.setBody("a")
        store.setBody("ab")
        XCTAssertTrue(store.unsaved)
        for _ in 0..<100 where store.unsaved {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertFalse(store.unsaved)
        XCTAssertEqual(try saved().map(\.body), ["ab"])
    }

    func testPinDeleteAndReload() throws {
        let store = NotesStore(directory: dir, debounce: .seconds(60))
        store.requestNewNote()
        store.setBody("prima")
        let first = try XCTUnwrap(store.openID)
        store.requestNewNote()
        store.setBody("a doua")
        let second = try XCTUnwrap(store.openID)
        store.close()

        store.togglePin(first)
        XCTAssertEqual(try saved().first?.id, first)
        store.open(second)
        store.delete(second)
        XCTAssertNil(store.openID)

        let reloaded = NotesStore(directory: dir)
        XCTAssertEqual(reloaded.notes.map(\.id), [first])
        XCTAssertEqual(reloaded.notes.first?.pinned, true)
        XCTAssertNil(reloaded.errorMessage)
    }

    func testFailedSaveKeepsTextAndShowsError() throws {
        try Data("x".utf8).write(to: dir)
        let store = NotesStore(directory: dir.appendingPathComponent("notes"), debounce: .seconds(60))
        store.requestNewNote()
        store.setBody("nu ma pierde")
        store.flush()
        XCTAssertNotNil(store.errorMessage)
        XCTAssertTrue(store.unsaved)
        XCTAssertEqual(store.openNote?.body, "nu ma pierde")

        // Dupa ce piedica dispare, urmatoarea salvare reuseste si eroarea se sterge.
        try FileManager.default.removeItem(at: dir)
        store.flush()
        XCTAssertNil(store.errorMessage)
        XCTAssertFalse(store.unsaved)
        XCTAssertEqual(try saved(in: dir.appendingPathComponent("notes")).map(\.body), ["nu ma pierde"])
    }
}
