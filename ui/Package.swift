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
    dependencies: [
        // The terminal panes. The upper bound is not tidiness: SwiftTerm tags 1.20.0 as a
        // pre-release ("one last before 2.0"), and SwiftPM cannot see that flag because the tag
        // carries no semver pre-release identifier, so a bare `from:` would resolve to it. 1.19.0
        // is what upstream marks as the release, and what upstream says comes next is 2.0 with
        // breaking changes, which this range would have to be opened by hand for anyway.
        .package(url: "https://github.com/migueldeicaza/SwiftTerm", "1.19.0" ..< "1.20.0"),
    ],
    targets: [
        .target(name: "TranscriptTool", swiftSettings: [.swiftLanguageMode(.v6)]),
        .target(name: "SwarmCore", dependencies: ["TranscriptTool"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .executableTarget(
            name: "Swarm",
            dependencies: ["SwarmCore", .product(name: "SwiftTerm", package: "SwiftTerm")],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(name: "SwarmCoreTests", dependencies: ["SwarmCore"], swiftSettings: [.swiftLanguageMode(.v6)]),
        .testTarget(name: "TranscriptToolTests", dependencies: ["TranscriptTool"], swiftSettings: [.swiftLanguageMode(.v6)]),
    ]
)
