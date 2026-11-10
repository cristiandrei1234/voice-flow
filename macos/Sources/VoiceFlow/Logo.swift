import AppKit
import SwiftUI

/// Semnul Voice Flow: un patrat rotunjit de sticla intunecata, cu lumini colorate in spate
/// si o unda de voce din bare albe. Aceeasi desenare e folosita in aplicatie si pentru iconita.
struct LogoMark: View {
    var size: CGFloat = 32

    /// Inaltimile barelor (0...1), simetrice: forma de unda care "curge".
    private static let bars: [CGFloat] = [0.26, 0.52, 0.86, 0.60, 1.0, 0.60, 0.86, 0.52, 0.26]

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.2237, style: .continuous)
        ZStack {
            // Fundalul: bleumarin adanc spre violet.
            shape.fill(
                LinearGradient(
                    colors: [Color(red: 0.07, green: 0.10, blue: 0.28), Color(red: 0.05, green: 0.04, blue: 0.16)],
                    startPoint: .top, endPoint: .bottom
                )
            )
            // Luminile din spatele sticlei.
            ZStack {
                Circle()
                    .fill(Color(red: 0.10, green: 0.75, blue: 1.0))
                    .frame(width: size * 0.78, height: size * 0.78)
                    .offset(x: -size * 0.26, y: -size * 0.22)
                Circle()
                    .fill(Color(red: 0.62, green: 0.30, blue: 1.0))
                    .frame(width: size * 0.74, height: size * 0.74)
                    .offset(x: size * 0.28, y: size * 0.18)
                Circle()
                    .fill(Color(red: 1.0, green: 0.36, blue: 0.72))
                    .frame(width: size * 0.42, height: size * 0.42)
                    .offset(x: size * 0.02, y: size * 0.40)
            }
            .blur(radius: size * 0.17)
            .opacity(0.92)
            // Un strat intunecat usor, ca barele albe sa aiba contrast.
            shape.fill(Color.black.opacity(0.16))

            // Unda de voce.
            HStack(alignment: .center, spacing: size * 0.034) {
                ForEach(Array(Self.bars.enumerated()), id: \.offset) { _, h in
                    Capsule(style: .continuous)
                        .fill(
                            LinearGradient(colors: [.white, Color(red: 0.86, green: 0.93, blue: 1.0)],
                                           startPoint: .top, endPoint: .bottom)
                        )
                        .frame(width: size * 0.052, height: max(size * 0.052, size * 0.50 * h))
                }
            }
            .shadow(color: .white.opacity(0.55), radius: size * 0.035)
            .shadow(color: Color(red: 0.2, green: 0.5, blue: 1.0).opacity(0.5), radius: size * 0.08, y: size * 0.02)

            // Reflexia de sus si marginea luminoasa: aspectul de sticla.
            shape
                .fill(
                    LinearGradient(colors: [.white.opacity(0.26), .white.opacity(0.03), .clear],
                                   startPoint: .top, endPoint: .center)
                )
                .padding(size * 0.012)
            shape.strokeBorder(
                LinearGradient(
                    colors: [.white.opacity(0.75), .white.opacity(0.10), .white.opacity(0.32)],
                    startPoint: .topLeading, endPoint: .bottomTrailing
                ),
                lineWidth: max(0.6, size * 0.012)
            )
        }
        .frame(width: size, height: size)
        .clipShape(shape)
        .accessibilityLabel("Voice Flow")
    }
}

/// Semnul impreuna cu numele, pentru antete.
struct LogoLockup: View {
    var markSize: CGFloat = 30
    var fontSize: CGFloat = 17

    var body: some View {
        HStack(spacing: markSize * 0.32) {
            LogoMark(size: markSize)
                .shadow(color: Color(red: 0.3, green: 0.45, blue: 1.0).opacity(0.45), radius: markSize * 0.22, y: markSize * 0.06)
            Text("Voice Flow").font(.system(size: fontSize, weight: .semibold))
        }
    }
}

/// `VoiceFlow --icon <folder>`: scrie iconita aplicatiei ca PNG de 1024 px (scripts/make-icon.sh
/// face din ea AppIcon.icns). Pe panza de 1024, semnul ocupa 824 px, cu umbra, ca la iconitele macOS.
@MainActor
enum AppIconExport {
    static var folder: String? {
        let args = CommandLine.arguments
        guard let i = args.firstIndex(of: "--icon"), i + 1 < args.count else { return nil }
        return args[i + 1]
    }

    static func run(into folder: String) {
        let dir = URL(fileURLWithPath: folder)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        write(
            ZStack {
                Color.clear
                LogoMark(size: 824).shadow(color: .black.opacity(0.32), radius: 22, y: 12)
            }
            .frame(width: 1024, height: 1024),
            to: dir.appendingPathComponent("icon-1024.png")
        )
        // Semnul singur, fara margini: pentru README si alte materiale.
        write(LogoMark(size: 512), to: dir.appendingPathComponent("logo-512.png"))
    }

    private static func write<V: View>(_ view: V, to url: URL) {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 1
        renderer.isOpaque = false
        guard let cg = renderer.cgImage else { return }
        let rep = NSBitmapImageRep(cgImage: cg)
        try? rep.representation(using: .png, properties: [:])?.write(to: url)
    }
}
