// swift-tools-version: 6.0
import Foundation
import PackageDescription

let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let repoRoot = URL(fileURLWithPath: packageDir).deletingLastPathComponent().deletingLastPathComponent().path
let candidatePaths = [
    ProcessInfo.processInfo.environment["HEX_LIBHEX_PATH"],
    "\(repoRoot)/target/release/libhex.a",
    "/Users/kit/mac-remote.noindex/cargo-target/hex/release/libhex.a",
].compactMap { $0 }
let libhexPath = candidatePaths.first(where: { FileManager.default.fileExists(atPath: $0) })
    ?? "\(repoRoot)/target/release/libhex.a"

let package = Package(
    name: "HexMacPackage",
    platforms: [
        .macOS(.v15),
    ],
    products: [
        .library(
            name: "HexMacFeature",
            targets: ["HexMacFeature"]
        ),
    ],
    targets: [
        .target(
            name: "HexEngineFFI",
            path: "Sources/HexEngineFFI",
            publicHeadersPath: "include",
            linkerSettings: [
                .unsafeFlags([
                    libhexPath,
                    "-L/usr/lib/swift",
                    "-Xlinker", "-rpath",
                    "-Xlinker", "/usr/lib/swift",
                    "-Xlinker", "-weak_framework",
                    "-Xlinker", "FoundationModels",
                ]),
                .linkedLibrary("c++"),
                .linkedFramework("Accelerate"),
                .linkedFramework("Foundation"),
                .linkedFramework("AppKit"),
                .linkedFramework("Metal"),
                .linkedFramework("QuartzCore"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("AudioToolbox"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Carbon"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("UniformTypeIdentifiers"),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("IOSurface"),
                .linkedFramework("AVFAudio"),
                .linkedFramework("Speech"),
            ]
        ),
        .target(
            name: "HexMacFeature",
            dependencies: ["HexEngineFFI"],
            path: "Sources/HexMacFeature",
            swiftSettings: [
                .swiftLanguageMode(.v6),
            ]
        ),
        .testTarget(
            name: "HexEngineClientTests",
            dependencies: ["HexMacFeature", "HexEngineFFI"],
            path: "Tests/HexEngineClientTests",
            swiftSettings: [
                .swiftLanguageMode(.v6),
            ]
        ),
    ]
)
