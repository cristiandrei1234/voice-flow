import SwiftUI

// Piese comune pentru ecranele din fereastra Voice Flow. Toate presupun fundal intunecat de sticla.
// Mai sunt in Glass.swift: liquidGlass / glassCard, GlassGroup, GlassIconButton, GlassPillButton,
// GlassSegmented, GlassToggleStyle, GlassProgressBar, IconTile, KeyCap / HotkeyView, ChipFlow (SettingsView.swift).

/// Titlul unei sectiuni dintr-un ecran.
struct SectionTitle: View {
    let text: String
    var count: Int?

    init(_ text: String, count: Int? = nil) {
        self.text = text
        self.count = count
    }

    var body: some View {
        HStack(spacing: 8) {
            Text(text).font(.system(size: 15, weight: .semibold))
            if let count {
                Text("\(count)")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.7))
                    .padding(.horizontal, 7)
                    .frame(height: 18)
                    .background(.white.opacity(0.12), in: Capsule())
            }
            Spacer()
        }
        .padding(.leading, 4)
        .padding(.top, 14)
        .padding(.bottom, 2)
    }
}

/// Un rand de setare: card de sticla cu iconita, titlu, explicatie optionala si un control in dreapta.
struct SettingRow<Trailing: View>: View {
    let title: String
    var subtitle: String?
    let icon: String
    var color: Color = .accentColor
    var tint: Color?
    @ViewBuilder var trailing: () -> Trailing

    var body: some View {
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
}

/// Comutator de sticla fara eticheta, pentru partea dreapta a unui `SettingRow`.
struct GlassSwitch: View {
    @Binding var isOn: Bool

    var body: some View {
        Toggle("", isOn: $isOn).labelsHidden().toggleStyle(GlassToggleStyle())
    }
}

/// Nota explicativa de sub o sectiune.
struct NoteText: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(0.5))
            .multilineTextAlignment(.leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 4)
            .padding(.top, 2)
    }
}

/// Eticheta mica de deasupra unui camp.
struct FieldLabel: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.caption.weight(.medium)).foregroundStyle(.white.opacity(0.6))
    }
}

/// Camp de text pe sticla intunecata (o linie).
struct GlassField: View {
    let placeholder: String
    @Binding var text: String
    var onSubmit: () -> Void = {}

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: 14))
            .onSubmit(onSubmit)
            .padding(.horizontal, 12)
            .frame(height: 34)
            .background(.black.opacity(0.28), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white.opacity(0.18), lineWidth: 0.7))
    }
}

/// Camp de text pe mai multe linii (prompturi, texte salvate, notite).
struct GlassTextEditor: View {
    @Binding var text: String
    var minHeight: CGFloat = 90

    var body: some View {
        TextEditor(text: $text)
            .font(.system(size: 13.5))
            .scrollContentBackground(.hidden)
            .padding(8)
            .frame(minHeight: minHeight)
            .background(.black.opacity(0.28), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white.opacity(0.18), lineWidth: 0.7))
    }
}

/// Camp de cautare in forma de pastila.
struct GlassSearchField: View {
    let placeholder: String
    @Binding var text: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.white.opacity(0.5))
            TextField(placeholder, text: $text).textFieldStyle(.plain).font(.system(size: 14))
            if !text.isEmpty {
                Button { text = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.white.opacity(0.5))
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
        .liquidGlass(Capsule())
    }
}

/// Card pentru "nu e nimic aici inca".
struct EmptyStateCard: View {
    let icon: String
    let title: String
    let subtitle: String

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 26)).foregroundStyle(.white.opacity(0.5))
            Text(title).font(.system(size: 15, weight: .semibold))
            Text(subtitle).font(.callout).foregroundStyle(.white.opacity(0.6)).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 18)
        .glassCard(radius: 18)
    }
}

/// Mesaj de eroare (portocaliu), cu buton de inchidere.
struct ErrorBanner: View {
    let message: String
    var onDismiss: () -> Void = {}

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text(message).font(.callout).fixedSize(horizontal: false, vertical: true)
            Spacer()
            Button(action: onDismiss) { Image(systemName: "xmark") }.buttonStyle(.plain)
        }
        .glassCard(radius: 14, padding: 12, tint: .orange.opacity(0.25))
    }
}

/// Eticheta mica (varianta de dictionar, cuvant-cheie, nume de aplicatie).
struct Chip: View {
    let text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(.white.opacity(0.85))
            .padding(.horizontal, 9)
            .frame(height: 22)
            .background(.white.opacity(0.1), in: Capsule())
            .overlay(Capsule().strokeBorder(.white.opacity(0.16), lineWidth: 0.6))
    }
}
