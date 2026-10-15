// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "SwarmNotify", targets: ["SwarmNotify"]),
        .executable(name: "SwarmHUD", targets: ["SwarmHUD"]),
    ],
    targets: [
        .executableTarget(name: "SwarmNotify", swiftSettings: [.swiftLanguageMode(.v6)]),
        .target(name: "SwarmHUDCore", swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(name: "SwarmHUD", dependencies: ["SwarmHUDCore"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "SwarmHUDTests", dependencies: ["SwarmHUDCore"], exclude: ["fixtures"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
