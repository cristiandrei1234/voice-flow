import AppKit
import SwiftUI

/// Panou plutitor care NU devine niciodata fereastra activa: aplicatia in care dictezi
/// ramane in fata, iar textul se lipeste acolo. Apare pe toate desktopurile si peste
/// aplicatiile in ecran complet; click-urile trec prin el.
final class PillPanel: NSPanel {
    convenience init<Content: View>(rootView: Content) {
        self.init(
            contentRect: NSRect(x: 0, y: 0, width: 260, height: 110),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .statusBar
        backgroundColor = .clear
        isOpaque = false
        hasShadow = false
        ignoresMouseEvents = true
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]

        let host = NSHostingView(rootView: rootView)
        host.frame = NSRect(x: 0, y: 0, width: 260, height: 110)
        contentView = host
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Afiseaza / ascunde pastila dupa starea motorului.
@MainActor
final class PillController {
    private let panel: PillPanel
    private weak var engine: Engine?

    init(engine: Engine) {
        self.engine = engine
        panel = PillPanel(rootView: PillView().environment(engine))
    }

    func update(for state: DictationState) {
        switch state {
        case .recording, .processing, .error:
            if !panel.isVisible { position() }
            panel.orderFrontRegardless()
        case .idle, .starting:
            // Lasam animatia de disparitie din SwiftUI sa se termine.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self] in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    // Nu ascundem daca intre timp a inceput o noua dictare.
                    let st = self.engine?.state ?? .idle
                    if st == .idle || st == .starting {
                        self.panel.orderOut(nil)
                    }
                }
            }
        }
    }

    /// Jos, pe centru, pe ecranul unde e mouse-ul, deasupra Dock-ului.
    private func position() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }
        let size = panel.frame.size
        panel.setFrameOrigin(NSPoint(x: frame.midX - size.width / 2, y: frame.minY - 4))
    }
}

/// Pastila: capsula cu bare care urmaresc vocea. Pe macOS 26 foloseste Liquid Glass
/// (sticla intunecata, ca barele albe sa se vada pe orice fundal); altfel capsula neagra.
struct PillView: View {
    @Environment(Engine.self) private var engine

    private var visible: Bool {
        engine.state == .recording || engine.state == .processing || engine.state == .error
    }

    private var level: Double { min(1, engine.bars.max() ?? 0) }

    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<Engine.barCount, id: \.self) { i in
                Capsule(style: .continuous)
                    .fill(.white)
                    .frame(width: 3, height: max(3, engine.bars[i] * 18))
                    .shadow(color: .white.opacity(0.6), radius: 3)
            }
        }
        .frame(width: 120, height: 36)
        .overlay(alignment: .leading) {
            // Fara maini (blocat cu apasare dubla): punct rosu in stanga.
            if DictationStore.shared.handsFree && engine.state == .recording {
                Circle()
                    .fill(Color(red: 1, green: 0.27, blue: 0.23))
                    .frame(width: 7, height: 7)
                    .shadow(color: .red.opacity(0.8), radius: 3)
                    .padding(.leading, 10)
                    .transition(.scale.combined(with: .opacity))
            }
        }
        .modifier(PillBackground(isError: engine.state == .error))
        .background {
            // Halou irizat sub pastila, care creste cu vocea.
            Capsule()
                .fill(LinearGradient(
                    colors: engine.state == .error ? [.red, .orange] : [.cyan, .blue, .purple, .pink],
                    startPoint: .leading, endPoint: .trailing
                ))
                .frame(width: 112, height: 30)
                .blur(radius: 10 + level * 6)
                .opacity(0.55 + level * 0.4)
                .scaleEffect(1 + level * 0.12)
                .animation(.easeOut(duration: 0.12), value: level)
        }
        .scaleEffect(visible ? 1 : 0.8)
        .opacity(visible ? 1 : 0)
        .animation(.spring(response: 0.28, dampingFraction: 0.78), value: visible)
        .animation(.easeOut(duration: 0.06), value: engine.bars)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct PillBackground: ViewModifier {
    var isError: Bool

    private var border: Color { isError ? Color(red: 1, green: 0.27, blue: 0.23) : .white.opacity(0.9) }

    func body(content: Content) -> some View {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            content
                .glassEffect(.regular.tint(.black.opacity(0.3)).interactive(), in: .capsule)
                .overlay(
                    Capsule().strokeBorder(
                        LinearGradient(colors: [border.opacity(0.9), border.opacity(0.2), border.opacity(0.6)],
                                       startPoint: .topLeading, endPoint: .bottomTrailing),
                        lineWidth: 1
                    )
                )
        } else {
            fallback(content)
        }
        #else
        fallback(content)
        #endif
    }

    private func fallback(_ content: Content) -> some View {
        content
            .background(Capsule().fill(.black))
            .overlay(Capsule().strokeBorder(border, lineWidth: 1.5))
            .shadow(color: .black.opacity(0.25), radius: 8, y: 3)
    }
}
