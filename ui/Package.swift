// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "Swarm", targets: ["Swarm"]),
        .library(name: "SwarmCore", targets: ["SwarmCore"]),
        .library(name: "TranscriptTool", targets: ["TranscriptTool"]),
    ],
    targets: [
        .target(name: "TranscriptTool", swiftSettings: [.swiftLanguageMode(.v6)]),
        .target(name: "SwarmCore", dependencies: ["TranscriptTool"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(
            name: "Swarm",
            dependencies: ["SwarmCore"],
            resources: [.copy("Resources/DiffViewer")],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(name: "SwarmCoreTests", dependencies: ["SwarmCore"], exclude: ["Fixtures/Skills"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "TranscriptToolTests", dependencies: ["TranscriptTool"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
