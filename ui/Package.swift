// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "SwarmWeb", targets: ["SwarmWeb"]),
        .executable(name: "SwarmNotify", targets: ["SwarmNotify"]),
    ],
    targets: [
        .executableTarget(name: "SwarmWeb", swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(name: "SwarmNotify", swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "SwarmWebTests", dependencies: ["SwarmWeb"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
