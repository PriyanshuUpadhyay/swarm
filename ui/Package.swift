// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "SwarmNotify", targets: ["SwarmNotify"]),
    ],
    targets: [
        .executableTarget(name: "SwarmNotify", swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
