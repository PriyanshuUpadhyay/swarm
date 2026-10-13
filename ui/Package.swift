// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "Swarm", targets: ["Swarm"]),
        .executable(name: "SwarmWeb", targets: ["SwarmWeb"]),
        .executable(name: "SwarmNotify", targets: ["SwarmNotify"]),
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
        .executableTarget(name: "SwarmWeb", swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(name: "SwarmNotify", swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "SwarmCoreTests", dependencies: ["SwarmCore"], resources: [.copy("Fixtures")], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "SwarmWebTests", dependencies: ["SwarmWeb"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "TranscriptToolTests", dependencies: ["TranscriptTool"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
