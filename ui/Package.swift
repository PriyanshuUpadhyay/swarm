// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "Swarm",
    platforms: [.macOS(.v26)],
    products: [
        .executable(name: "Swarm", targets: ["Swarm"]),
        .executable(name: "swarm-bridge", targets: ["swarm-bridge"]),
        .executable(name: "swarm-sleep-helper", targets: ["swarm-sleep-helper"]),
        .library(name: "SwarmCore", targets: ["SwarmCore"]),
    ],
    dependencies: [
        // Native live Markdown editing for workspace notes. Pin the pre-1.0 API we integrate.
        .package(url: "https://github.com/nodes-app/swift-markdown-engine", exact: "0.12.0"),
        // The terminal panes. The upper bound is not tidiness: SwiftTerm tags 1.20.0 as a
        // pre-release ("one last before 2.0"), and SwiftPM cannot see that flag because the tag
        // carries no semver pre-release identifier, so a bare `from:` would resolve to it. 1.19.0
        // is what upstream marks as the release, and what upstream says comes next is 2.0 with
        // breaking changes, which this range would have to be opened by hand for anyway.
        .package(url: "https://github.com/migueldeicaza/SwiftTerm", "1.19.0" ..< "1.20.0"),
    ],
    targets: [
        .target(
            name: "SwarmCore",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .executableTarget(
            name: "Swarm",
            dependencies: [
                "SwarmCore",
                .product(name: "MarkdownEngine", package: "swift-markdown-engine"),
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        // The MCP stdio shim an agent CLI launches, which forwards to the running app over a unix
        // domain socket. Its own file is three lines: `Tools/test-core.sh` mirrors only SwarmCore
        // and its tests into the package it runs, so an executable target is invisible to the
        // suite and everything worth testing lives in `BridgeShim` instead.
        .executableTarget(
            name: "swarm-bridge",
            dependencies: ["SwarmCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        // The privileged daemon that turns the system sleep switch off for a Keep Awake session,
        // which is the only way to hold a Mac open with the lid shut. No dependencies on purpose:
        // it runs as root, so the less of Swarm is inside it the better. See its own file.
        .executableTarget(
            name: "swarm-sleep-helper",
            path: "Sources/swarm-sleep-helper",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "SwarmCoreTests",
            dependencies: ["SwarmCore"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
