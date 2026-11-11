import SwiftUI

/// Ecranul "History": statistici, cautare, dictarile grupate pe zile si setarile istoricului.
struct HistoryView: View {
    private var store: HistoryStore { .shared }
    @State private var pendingDelete: HistoryItem?
    @State private var confirmClear = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            statsHeader

            GlassSearchField(
                placeholder: "Search history",
                text: Binding(get: { store.searchText }, set: { store.setSearch($0) })
            )
            .padding(.top, 6)

            list

            settingsBlock
        }
        .onAppear {
            // Ziua se poate schimba cat a stat fereastra inchisa.
            store.refreshStats()
        }
        .confirmationDialog(
            "Delete this dictation from history?",
            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
            presenting: pendingDelete
        ) { item in
            Button("Delete", role: .destructive) {
                withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { store.delete(item) }
            }
            Button("Cancel", role: .cancel) {}
        } message: { item in
            Text(RoText.preview(item.text))
        }
        .confirmationDialog("Delete all history?", isPresented: $confirmClear) {
            Button("Delete all history", role: .destructive) { store.clear() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("All saved dictations will be removed from this Mac. This can't be undone.")
        }
    }

    // MARK: Statistici

    @ViewBuilder
    private var statsHeader: some View {
        let s = store.stats
        HStack(spacing: 8) {
            StatTile(icon: "sun.max.fill", color: .orange, value: RoText.number(s?.today.words ?? 0), label: "words today")
            StatTile(icon: "calendar", color: .blue, value: RoText.number(s?.week.words ?? 0), label: "words in the last 7 days")
            StatTile(icon: "hourglass", color: .green, value: RoText.duration(ms: s?.savedMs ?? 0), label: "time saved (estimate)")
            StatTile(icon: "speedometer", color: .purple, value: (s?.wpm ?? 0) > 0 ? "\(Int((s?.wpm ?? 0).rounded()))" : "–", label: "words per minute")
        }

        if let s {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text("Words per day, last 14 days")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.6))
                    Spacer()
                    if s.streakDays > 0 {
                        Label(RoText.count(s.streakDays, "day in a row", "days in a row"), systemImage: "flame.fill")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.orange)
                    }
                }
                DayBars(days: s.days)
                Text(summary(s))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.5))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .glassCard(radius: 16, padding: 12)
        }
    }

    private func summary(_ s: HistoryStats) -> String {
        if s.total.dictations == 0 { return "No dictations saved yet." }
        return "Total: \(RoText.count(s.total.dictations, "dictation", "dictations")) · "
            + "\(RoText.count(s.total.words, "word", "words")) · "
            + "\(RoText.duration(ms: s.audioMs)) of speaking"
    }

    // MARK: Lista

    private var groups: [(day: Date, items: [HistoryItem])] {
        let calendar = Calendar.current
        var out: [(day: Date, items: [HistoryItem])] = []
        // Lista vine deja de la cea mai noua la cea mai veche.
        for item in store.items {
            let day = calendar.startOfDay(for: item.date)
            if let last = out.indices.last, out[last].day == day {
                out[last].items.append(item)
            } else {
                out.append((day, [item]))
            }
        }
        return out
    }

    @ViewBuilder
    private var list: some View {
        if store.loaded && store.items.isEmpty {
            if !store.query.isEmpty {
                EmptyStateCard(icon: "magnifyingglass", title: "No dictations found", subtitle: "Nothing matches “\(store.query)”.")
            } else if store.enabled {
                EmptyStateCard(icon: "clock.arrow.circlepath", title: "You haven't dictated anything yet", subtitle: "Your dictations appear here, so you can search and copy them again.")
            } else {
                EmptyStateCard(icon: "clock.badge.xmark", title: "History is off", subtitle: "Turn it on below to keep your dictations.")
            }
        }

        ForEach(groups, id: \.day) { group in
            SectionTitle(RoText.dayTitle(group.day), count: group.items.count)
            ForEach(group.items) { item in
                HistoryCard(item: item, onCopy: { store.copy(item) }, onDelete: { pendingDelete = item })
            }
        }

        if store.hasMore {
            HStack {
                Spacer()
                GlassPillButton(title: "Show more", icon: "chevron.down") { store.loadMore() }
                    .disabled(store.loadingMore)
                    .opacity(store.loadingMore ? 0.5 : 1)
                Spacer()
            }
            .padding(.top, 6)
        }
    }

    // MARK: Setari

    @ViewBuilder
    private var settingsBlock: some View {
        SectionTitle("History retention")

        SettingRow(
            title: "Keep dictation history",
            subtitle: store.enabled ? "Every dictation is saved on this Mac." : "New dictations are no longer saved.",
            icon: "clock.arrow.circlepath", color: .blue
        ) {
            GlassSwitch(isOn: Binding(get: { store.enabled }, set: { store.setEnabled($0) }))
        }

        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                IconTile(systemName: "calendar.badge.clock", color: .teal, size: 40)
                VStack(alignment: .leading, spacing: 2) {
                    Text("How long to keep it").font(.system(size: 14, weight: .medium))
                    Text("Older dictations are deleted when the app starts.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.6))
                }
            }
            GlassSegmented(
                options: [(30, "30 days"), (90, "90 days"), (365, "365 days"), (0, "Forever")],
                selection: Binding(get: { store.retentionDays }, set: { store.setRetention($0) })
            )
        }
        .glassCard(radius: 16, padding: 12)

        SettingRow(title: "History file", subtitle: store.filePath, icon: "folder.fill", color: .orange) {
            GlassPillButton(title: "Show in Finder", icon: "folder") { store.revealInFinder() }
        }

        let total = store.stats?.total.dictations ?? 0
        SettingRow(
            title: "All dictations",
            subtitle: total == 0 ? "Nothing is saved." : "\(RoText.count(total, "saved dictation", "saved dictations")). Deleting can't be undone.",
            icon: "trash.fill", color: .red
        ) {
            GlassPillButton(title: "Delete all history", icon: "trash") { confirmClear = true }
                .disabled(total == 0)
                .opacity(total == 0 ? 0.4 : 1)
        }

        NoteText("History contains everything you've dictated. It stays only on this Mac, in the file above, and isn't sent anywhere. Time saved is an estimate: it compares speaking with typing at 40 words per minute. Statistics are calculated from the saved dictations.")
    }
}

/// O cifra mare cu explicatie, pe sticla.
private struct StatTile: View {
    let icon: String
    let color: Color
    let value: String
    let label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Image(systemName: icon)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(color)
                .frame(height: 16)
            Text(value)
                .font(.system(size: 21, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            Text(label)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.6))
                .lineLimit(2, reservesSpace: true)
                .fixedSize(horizontal: false, vertical: true)
        }
        .glassCard(radius: 16, padding: 12)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(value) \(label)")
    }
}

/// Grafic mic cu bare: cuvinte pe zi. Ziua de azi e colorata; zilele sunt etichetate din trei in trei.
private struct DayBars: View {
    let days: [HistoryStats.Day]
    private let barHeight: CGFloat = 56

    var body: some View {
        let top = max(days.map(\.words).max() ?? 0, 1)
        HStack(alignment: .bottom, spacing: 6) {
            ForEach(Array(days.enumerated()), id: \.element.id) { index, day in
                let fromEnd = days.count - 1 - index
                VStack(spacing: 4) {
                    RoundedRectangle(cornerRadius: 3, style: .continuous)
                        .fill(fromEnd == 0 ? Color.accentColor : Color.white.opacity(day.words > 0 ? 0.55 : 0.14))
                        .frame(height: day.words > 0 ? max(4, barHeight * CGFloat(day.words) / CGFloat(top)) : 3)
                        .frame(maxWidth: .infinity)
                    Text(fromEnd % 3 == 0 ? (fromEnd == 0 ? "today" : RoText.shortDay(day.date)) : " ")
                        .font(.system(size: 9.5))
                        .foregroundStyle(.white.opacity(fromEnd == 0 ? 0.8 : 0.5))
                        .fixedSize()
                        .frame(width: 1, height: 12)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .contentShape(Rectangle())
                .help("\(RoText.dayTitle(day.date)): \(RoText.count(day.words, "word", "words"))")
            }
        }
        .frame(height: barHeight + 16)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Words per day over the last 14 days")
        .accessibilityValue(days.map { "\(RoText.shortDay($0.date)): \($0.words)" }.joined(separator: ", "))
    }
}

/// Cardul unei dictari: textul, cand si cat, copiere, textul brut si stergere.
private struct HistoryCard: View {
    let item: HistoryItem
    let onCopy: () -> Void
    let onDelete: () -> Void
    @Environment(Engine.self) private var engine
    @State private var expanded = false
    @State private var showRaw = false
    @State private var copied = false
    /// "Fix a word": the words picked from this dictation and what they should have been.
    @State private var fixing = false
    @State private var picked: Set<Int> = []
    @State private var shouldBe = ""
    @State private var taught: String?

    private var words: [String] { item.text.split(whereSeparator: \.isWhitespace).map(String.init) }

    private var heard: String {
        DictionaryTeaching.normalize(picked.sorted().map { words[$0] }.joined(separator: " "))
    }

    private var fixPlan: DictionaryTeaching.Plan? {
        DictionaryTeaching.plan(heard: heard, correct: shouldBe, entries: engine.dictEntries)
    }

    /// Aproximare pentru "nu incape in 4 randuri": arata butonul de extindere.
    private var isLong: Bool {
        item.text.count > 300 || item.text.filter(\.isNewline).count >= 4
    }

    private var meta: String {
        var parts = [RoText.time(item.date), RoText.count(item.words, "word", "words"), RoText.seconds(ms: item.audioMs)]
        if let app = item.app, !app.isEmpty { parts.append(app) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(item.text)
                .font(.system(size: 13.5))
                .lineSpacing(2)
                .textSelection(.enabled)
                .lineLimit(expanded ? nil : 4)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)

            if showRaw {
                VStack(alignment: .leading, spacing: 4) {
                    FieldLabel("Before corrections")
                    Text(item.raw)
                        .font(.system(size: 12.5))
                        .foregroundStyle(.white.opacity(0.75))
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.black.opacity(0.25), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            }

            if fixing { fixPanel }
            if let taught {
                Label(taught, systemImage: "checkmark.circle.fill")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.green)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack(spacing: 10) {
                Text(meta)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.55))
                    .lineLimit(1)
                if isLong {
                    linkButton(expanded ? "Show less" : "Show all") { expanded.toggle() }
                }
                if item.hasDifferentRaw {
                    linkButton(showRaw ? "Hide raw text" : "Raw text") { showRaw.toggle() }
                }
                linkButton(fixing ? "Cancel fix" : "Fix a word") {
                    fixing.toggle()
                    picked = []
                    shouldBe = ""
                    taught = nil
                }
                Spacer(minLength: 8)
                GlassIconButton(systemName: "trash", size: 30, help: "Delete from history", action: onDelete)
                GlassPillButton(title: copied ? "Copied" : "Copy", icon: copied ? "checkmark" : "doc.on.doc") {
                    onCopy()
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) { copied = false }
                }
            }
        }
        .glassCard(radius: 16, padding: 12)
    }

    /// Pick the misheard word(s), type what they should be, and the dictionary learns the pair.
    private var fixPanel: some View {
        VStack(alignment: .leading, spacing: 10) {
            FieldLabel("Click the word (or words) that came out wrong")
            ChipFlow(spacing: 6) {
                ForEach(Array(words.enumerated()), id: \.offset) { index, word in
                    let on = picked.contains(index)
                    Button {
                        if on { picked.remove(index) } else { picked.insert(index) }
                    } label: {
                        Text(word)
                            .font(.system(size: 12.5, weight: on ? .semibold : .regular))
                            .foregroundStyle(on ? Color.black : .white.opacity(0.85))
                            .padding(.horizontal, 9)
                            .frame(height: 24)
                            .background(on ? Color.white : .white.opacity(0.1), in: Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            if !heard.isEmpty {
                FieldLabel("“\(heard)” should be written as")
                HStack(spacing: 10) {
                    GlassField(placeholder: "e.g. review", text: $shouldBe, onSubmit: teach)
                    GlassPillButton(title: "Teach", icon: "checkmark", prominent: true, action: teach)
                        .disabled(fixPlan == nil)
                        .opacity(fixPlan == nil ? 0.45 : 1)
                }
                NoteText("From now on “\(heard)” is replaced everywhere, so only teach words you never mean literally. You can undo it in Dictionary.")
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.black.opacity(0.25), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func teach() {
        guard let plan = fixPlan else { return }
        engine.lastError = nil
        engine.saveEntry(line: plan.line, expected: plan.expected, correct: plan.correct, aliases: plan.aliases, group: nil)
        taught = "Learned: “\(heard)” is now written “\(plan.correct)”."
        withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { fixing = false }
        picked = []
        shouldBe = ""
    }

    private func linkButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button {
            withAnimation(.spring(response: 0.3, dampingFraction: 0.85)) { action() }
        } label: {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(0.85))
                .underline()
                .fixedSize()
        }
        .buttonStyle(.plain)
    }
}

/// Formatari in engleza pentru ecranul de istoric.
enum RoText {
    private static let locale = Locale(identifier: "en_US")

    /// 1234 -> "1,234"
    static func number(_ n: Int) -> String {
        n.formatted(.number.locale(locale))
    }

    /// "1 word", "5 words", "1,234 words"
    static func count(_ n: Int, _ one: String, _ many: String) -> String {
        if n == 1 { return "1 \(one)" }
        return "\(number(n)) \(many)"
    }

    /// Durata lunga: "under 1 min", "12 min", "2 h 05 min"
    static func duration(ms: Double) -> String {
        let minutes = Int(ms / 60_000)
        if ms <= 0 { return "0 min" }
        if minutes < 1 { return "under 1 min" }
        if minutes < 60 { return "\(minutes) min" }
        return String(format: "%d h %02d min", minutes / 60, minutes % 60)
    }

    /// Durata unei dictari: "8 s", "1 min 05 s"
    static func seconds(ms: Double) -> String {
        let s = Int((ms / 1000).rounded())
        return s < 60 ? "\(s) s" : String(format: "%d min %02d s", s / 60, s % 60)
    }

    static func time(_ date: Date) -> String {
        let f = DateFormatter()
        f.locale = locale
        f.dateFormat = "HH:mm"
        return f.string(from: date)
    }

    /// "Today", "Yesterday", "Tuesday, November 3" (cu anul, daca nu e cel curent).
    static func dayTitle(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInYesterday(date) { return "Yesterday" }
        let f = DateFormatter()
        f.locale = locale
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: Date())
        f.dateFormat = sameYear ? "EEEE, MMMM d" : "EEEE, MMMM d, yyyy"
        let s = f.string(from: date)
        return s.prefix(1).uppercased() + s.dropFirst()
    }

    /// "Nov 3"
    static func shortDay(_ date: Date) -> String {
        let f = DateFormatter()
        f.locale = locale
        f.dateFormat = "MMM d"
        return f.string(from: date)
    }

    /// Inceputul unui text, pentru dialogul de confirmare.
    static func preview(_ text: String) -> String {
        let flat = text.split(whereSeparator: \.isNewline).joined(separator: " ")
        return flat.count > 120 ? "“\(flat.prefix(120))…”" : "“\(flat)”"
    }
}
