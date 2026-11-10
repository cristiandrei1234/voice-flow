import SwiftUI

// Liquid Glass doar unde Apple il recomanda: elemente care plutesc deasupra continutului
// (pastila, butonul principal din meniu). Continutul (liste, formulare) ramane normal.
// Pe sisteme mai vechi decat macOS 26 se folosesc stilurile clasice.

extension View {
    /// Buton principal: sticla colorata pe macOS 26, buton proeminent altfel.
    @ViewBuilder
    func primaryGlassButton() -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            self.buttonStyle(.glassProminent)
        } else {
            self.buttonStyle(.borderedProminent)
        }
        #else
        self.buttonStyle(.borderedProminent)
        #endif
    }

    /// Buton secundar: sticla transparenta pe macOS 26.
    @ViewBuilder
    func secondaryGlassButton() -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            self.buttonStyle(.glass)
        } else {
            self.buttonStyle(.bordered)
        }
        #else
        self.buttonStyle(.bordered)
        #endif
    }
}

/// O tasta desenata ca un "keycap": simbol SF si / sau text, cu aer in jur.
struct KeyCap: View {
    let part: KeyPart
    var compact = false

    var body: some View {
        HStack(spacing: compact ? 4 : 5) {
            if let symbol = part.symbol {
                Image(systemName: symbol)
                    .font(.system(size: compact ? 11 : 12.5, weight: .semibold))
            }
            if let text = part.text {
                Text(text)
                    .font(.system(size: compact ? 12 : 13, weight: .medium, design: .rounded))
            }
        }
        .foregroundStyle(.white.opacity(0.92))
        .padding(.horizontal, compact ? 9 : 11)
        .frame(minWidth: compact ? 28 : 32, minHeight: compact ? 26 : 30)
        .liquidGlass(RoundedRectangle(cornerRadius: compact ? 9 : 10, style: .continuous))
    }
}

/// O scurtatura completa: keycap-uri puse unul langa altul.
struct HotkeyView: View {
    let hotkey: String
    var compact = false

    var body: some View {
        HStack(spacing: compact ? 4 : 5) {
            ForEach(Array(HotkeyFormat.parts(hotkey).enumerated()), id: \.offset) { _, part in
                KeyCap(part: part, compact: compact)
            }
        }
        .accessibilityElement()
        .accessibilityLabel(HotkeyFormat.text(hotkey))
        .help(HotkeyFormat.text(hotkey))
    }
}

// MARK: Piese comune de design

// MARK: Liquid Glass

extension View {
    /// Liquid Glass (macOS 26+) in forma data; pe sisteme mai vechi, material translucid cu contur.
    @ViewBuilder
    func liquidGlass<S: Shape>(_ shape: S, tint: Color? = nil, interactive: Bool = false) -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            self.glassEffect(Glass.regular.tint(tint).interactive(interactive), in: shape)
        } else {
            self.background(.ultraThinMaterial, in: shape)
                .overlay(shape.stroke(.white.opacity(0.16), lineWidth: 0.8))
        }
        #else
        self.background(.ultraThinMaterial, in: shape)
            .overlay(shape.stroke(.white.opacity(0.16), lineWidth: 0.8))
        #endif
    }

    /// Card de sticla cu colturi rotunjite.
    func glassCard(radius: CGFloat = 18, padding: CGFloat = 14, tint: Color? = nil) -> some View {
        self
            .padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .liquidGlass(RoundedRectangle(cornerRadius: radius, style: .continuous), tint: tint)
    }
}

/// Grupeaza bucati de sticla apropiate, ca sistemul sa le deseneze impreuna (si sa le contopeasca la animatii).
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = 10
    @ViewBuilder var content: Content

    var body: some View {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
        #else
        content
        #endif
    }
}

/// Buton rotund de sticla cu un simbol (inapoi / inchide / setari, ca in iOS 26).
struct GlassIconButton: View {
    let systemName: String
    var size: CGFloat = 32
    var help: String = ""
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: size * 0.42, weight: .semibold))
                .foregroundStyle(.white.opacity(0.9))
                .frame(width: size, height: size)
                .contentShape(Circle())
                .liquidGlass(Circle(), interactive: true)
                .overlay(IridescentRim(shape: Circle(), lineWidth: 1).opacity(0.7))
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(help)
    }
}

/// Comutator cu segmente: pastila de sticla, segmentul ales e alb plin.
struct GlassSegmented<T: Hashable>: View {
    let options: [(value: T, label: String)]
    @Binding var selection: T
    @Namespace private var ns

    var body: some View {
        HStack(spacing: 2) {
            ForEach(Array(options.enumerated()), id: \.offset) { _, option in
                let selected = option.value == selection
                Button {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) { selection = option.value }
                } label: {
                    Text(option.label)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(selected ? Color.black : Color.white.opacity(0.85))
                        .padding(.horizontal, 14)
                        .frame(height: 28)
                        .background {
                            if selected {
                                Capsule().fill(.white).matchedGeometryEffect(id: "sel", in: ns)
                            }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(4)
        .liquidGlass(Capsule())
    }
}

/// Lumina colorata, difuza, in spatele sticlei: ii da sticlei ceva de refractat.
struct AmbientGlow: View {
    var color: Color = .accentColor
    var secondary: Color = .purple

    var body: some View {
        ZStack {
            Circle().fill(color.opacity(0.55)).frame(width: 260, height: 260).blur(radius: 90).offset(x: -90, y: -120)
            Circle().fill(secondary.opacity(0.35)).frame(width: 220, height: 220).blur(radius: 100).offset(x: 130, y: 160)
        }
        .allowsHitTesting(false)
    }
}

/// Iconita in doua straturi: un patrat colorat in spate si, decalat in fata, un patrat de sticla
/// mata cu simbolul alb. Culoarea se vede prin sticla.
struct IconTile: View {
    let systemName: String
    var color: Color = .accentColor
    var size: CGFloat = 30

    var body: some View {
        let tile = size * 0.74
        let radius = tile * 0.3
        ZStack {
            RoundedRectangle(cornerRadius: radius, style: .continuous)
                .fill(LinearGradient(colors: [color.opacity(0.95), color], startPoint: .top, endPoint: .bottom))
                .frame(width: tile, height: tile)
                .offset(x: size * 0.13, y: -size * 0.13)
                .shadow(color: color.opacity(0.6), radius: size * 0.22, x: -size * 0.08, y: size * 0.1)
            RoundedRectangle(cornerRadius: radius, style: .continuous)
                .fill(.white.opacity(0.16))
                .frame(width: tile, height: tile)
                .liquidGlass(RoundedRectangle(cornerRadius: radius, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(
                        LinearGradient(colors: [.white.opacity(0.75), .white.opacity(0.15)],
                                       startPoint: .topLeading, endPoint: .bottomTrailing),
                        lineWidth: 0.8
                    )
                )
                .overlay(
                    Image(systemName: systemName)
                        .font(.system(size: tile * 0.5, weight: .semibold))
                        .foregroundStyle(.white)
                        .shadow(color: .black.opacity(0.2), radius: 1, y: 0.5)
                )
                .offset(x: -size * 0.13, y: size * 0.13)
        }
        .frame(width: size, height: size)
    }
}

/// Contur irizat (curcubeu discret), ca la butoanele de sticla intunecata.
struct IridescentRim<S: InsettableShape>: View {
    let shape: S
    var lineWidth: CGFloat = 1.2

    var body: some View {
        shape.strokeBorder(
            AngularGradient(
                colors: [.white.opacity(0.15), .cyan, .purple, .pink, .orange, .white.opacity(0.15), .green.opacity(0.7), .white.opacity(0.15)],
                center: .center, angle: .degrees(-40)
            ),
            lineWidth: lineWidth
        )
    }
}

/// Buton pastila. Principal: sticla intunecata cu contur irizat si sageata. Secundar: sticla simpla.
struct GlassPillButton: View {
    let title: String
    var icon: String?
    var prominent = false
    var fullWidth = false
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let icon, !prominent { Image(systemName: icon).font(.system(size: 12, weight: .semibold)) }
                Text(title).font(.system(size: 13, weight: prominent ? .semibold : .medium))
                if prominent {
                    Image(systemName: icon ?? "arrow.right")
                        .font(.system(size: 12, weight: .semibold))
                        .offset(x: hovering ? 2 : 0)
                }
            }
            .foregroundStyle(.white.opacity(prominent ? 1 : 0.9))
            .padding(.horizontal, 18)
            .frame(maxWidth: fullWidth ? .infinity : nil)
            .frame(height: 34)
            .background {
                if prominent {
                    Capsule().fill(.black.opacity(hovering ? 0.72 : 0.6))
                }
            }
            .liquidGlass(Capsule(), interactive: true)
            .overlay {
                if prominent {
                    IridescentRim(shape: Capsule(), lineWidth: hovering ? 1.6 : 1.2)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hovering = h } }
    }
}

/// Rand de meniu: iconita conturata si text; sub mouse apare o pastila de sticla.
struct GlassMenuRow: View {
    let title: String
    let icon: String
    var shortcut: String?
    var destructive = false
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: icon)
                    .font(.system(size: 15, weight: .regular))
                    .frame(width: 22)
                Text(title).font(.system(size: 14))
                Spacer()
                if let shortcut {
                    Text(shortcut).font(.system(size: 12)).foregroundStyle(.white.opacity(0.4))
                }
            }
            .foregroundStyle(destructive ? Color(red: 1, green: 0.5, blue: 0.47) : .white.opacity(0.92))
            .padding(.horizontal, 12)
            .frame(height: 36)
            .background {
                if hovering {
                    Capsule()
                        .fill(.white.opacity(0.12))
                        .overlay(
                            Capsule().strokeBorder(
                                LinearGradient(colors: [.white.opacity(0.55), .white.opacity(0.1)],
                                               startPoint: .top, endPoint: .bottom),
                                lineWidth: 0.8
                            )
                        )
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { h in withAnimation(.easeOut(duration: 0.12)) { hovering = h } }
    }
}

/// Bare care urmaresc vocea (aceleasi valori ca pastila).
struct MiniBars: View {
    let bars: [Double]
    var color: Color = .red
    var scale: CGFloat = 16

    var body: some View {
        HStack(spacing: 3) {
            ForEach(Array(bars.enumerated()), id: \.offset) { _, v in
                Capsule(style: .continuous)
                    .fill(color)
                    .frame(width: 3, height: max(3, v * scale))
            }
        }
        .frame(height: scale)
        .animation(.easeOut(duration: 0.06), value: bars)
    }
}

/// Comutator de sticla: sina transparenta cu contur luminos; pornit, butonul e o pastila verde
/// care lumineaza, oprit e doar sticla goala.
struct GlassToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 12) {
            configuration.label
            Button {
                withAnimation(.spring(response: 0.32, dampingFraction: 0.72)) { configuration.isOn.toggle() }
            } label: {
                track(isOn: configuration.isOn)
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(.isToggle)
            .accessibilityValue(configuration.isOn ? "on" : "off")
        }
    }

    private func track(isOn: Bool) -> some View {
        let green = Color(red: 0.2, green: 0.84, blue: 0.29)
        return ZStack(alignment: isOn ? .trailing : .leading) {
            Capsule()
                .fill(isOn ? green.opacity(0.16) : .clear)
                .frame(width: 58, height: 32)
                .liquidGlass(Capsule())
                .overlay(
                    Capsule().strokeBorder(
                        LinearGradient(colors: [.white.opacity(0.75), .white.opacity(0.2), .white.opacity(0.55)],
                                       startPoint: .topLeading, endPoint: .bottomTrailing),
                        lineWidth: 1
                    )
                )
            thumb(isOn: isOn, green: green)
                .padding(.horizontal, 4)
        }
        .frame(width: 58, height: 32)
        .contentShape(Capsule())
    }

    @ViewBuilder
    private func thumb(isOn: Bool, green: Color) -> some View {
        if isOn {
            Capsule()
                .fill(LinearGradient(colors: [green.opacity(0.95), Color(red: 0.1, green: 0.7, blue: 0.2)],
                                     startPoint: .top, endPoint: .bottom))
                .frame(width: 32, height: 24)
                .overlay(
                    // Reflexia de sus, ca la sticla colorata.
                    Capsule()
                        .fill(LinearGradient(colors: [.white.opacity(0.55), .clear], startPoint: .top, endPoint: .center))
                        .padding(2)
                )
                .overlay(Capsule().strokeBorder(.white.opacity(0.45), lineWidth: 0.8))
                .shadow(color: green.opacity(0.75), radius: 8)
        } else {
            Capsule()
                .fill(.white.opacity(0.06))
                .frame(width: 32, height: 24)
                .overlay(
                    Capsule().strokeBorder(
                        LinearGradient(colors: [.white.opacity(0.85), .white.opacity(0.3)],
                                       startPoint: .top, endPoint: .bottom),
                        lineWidth: 1
                    )
                )
        }
    }
}

/// Bara de taburi: pastila intunecata cu iconite, iar tabul ales sta sub o "lupa" de sticla
/// care iese putin din bara si aluneca de la un tab la altul.
struct GlassTabBar<T: Hashable>: View {
    let options: [(value: T, label: String, icon: String)]
    @Binding var selection: T
    @Namespace private var ns

    private let tabWidth: CGFloat = 100
    private let barHeight: CGFloat = 58
    private let lensOverflow: CGFloat = 7

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(options.enumerated()), id: \.offset) { _, option in
                let selected = option.value == selection
                Button {
                    withAnimation(.spring(response: 0.38, dampingFraction: 0.74)) { selection = option.value }
                } label: {
                    VStack(spacing: 5) {
                        Image(systemName: option.icon)
                            .font(.system(size: 17, weight: .semibold))
                            .shadow(color: .white.opacity(selected ? 0.85 : 0), radius: 6)
                        Text(option.label)
                            .font(.system(size: 11.5, weight: selected ? .semibold : .medium))
                    }
                    .foregroundStyle(.white.opacity(selected ? 1 : 0.42))
                    .frame(width: tabWidth, height: barHeight)
                    .background {
                        if selected {
                            // Lupa: mai inalta decat bara, cu sticla adevarata si margine luminoasa.
                            RoundedRectangle(cornerRadius: 24, style: .continuous)
                                .fill(.white.opacity(0.05))
                                .liquidGlass(RoundedRectangle(cornerRadius: 24, style: .continuous), interactive: true)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(
                                        LinearGradient(
                                            colors: [.white.opacity(0.85), .white.opacity(0.15), .white.opacity(0.6)],
                                            startPoint: .topLeading, endPoint: .bottomTrailing
                                        ),
                                        lineWidth: 1
                                    )
                                )
                                .shadow(color: .black.opacity(0.35), radius: 10, y: 4)
                                .padding(.vertical, -lensOverflow)
                                .matchedGeometryEffect(id: "lens", in: ns)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? [.isSelected] : [])
            }
        }
        .background(
            Capsule()
                .fill(.black.opacity(0.62))
                .overlay(Capsule().strokeBorder(.white.opacity(0.1), lineWidth: 0.8))
                .shadow(color: .black.opacity(0.4), radius: 14, y: 6)
        )
        .padding(.vertical, lensOverflow)
    }
}

/// Halou irizat (cyan, roz, violet) care se vede pe dupa marginea unui buton de sticla.
/// `energy` (0...1) il face mai mare si mai intens: il legam de vocea ta cat asculta.
struct IridescentHalo: View {
    var colors: [Color] = [.cyan, .blue, .purple, .pink, .cyan]
    var energy: Double = 0
    var size: CGFloat = 92

    var body: some View {
        Circle()
            .fill(AngularGradient(colors: colors, center: .center, angle: .degrees(210)))
            .frame(width: size, height: size)
            .blur(radius: 16 + energy * 8)
            .opacity(0.75 + energy * 0.25)
            .scaleEffect(1.04 + energy * 0.22)
            .animation(.easeOut(duration: 0.12), value: energy)
    }
}

/// Bara de progres din sticla: sina transparenta, umplutura luminoasa.
struct GlassProgressBar: View {
    var value: Double

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(.white.opacity(0.1))
                    .overlay(Capsule().strokeBorder(.white.opacity(0.25), lineWidth: 0.6))
                Capsule()
                    .fill(LinearGradient(colors: [.cyan, .blue, .purple], startPoint: .leading, endPoint: .trailing))
                    .frame(width: max(8, geo.size.width * min(1, max(0, value))))
                    .shadow(color: .blue.opacity(0.7), radius: 5)
                    .animation(.easeOut(duration: 0.25), value: value)
            }
        }
        .frame(height: 8)
    }
}
