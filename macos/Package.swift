// swift-tools-version: 5.10
// Aplicatia macOS Voice Flow: SwiftUI (meniu, setari, pastila cu Liquid Glass).
// Motorul (transcriere, microfon, tastatura) e binarul Rust `voice-flow daemon`.
import PackageDescription

let package = Package(
    name: "VoiceFlow",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "VoiceFlow",
            path: "Sources/VoiceFlow"
        ),
        .testTarget(
            name: "VoiceFlowTests",
            dependencies: ["VoiceFlow"],
            path: "Tests/VoiceFlowTests"
        ),
    ]
)
