// swift-tools-version: 6.1

import Foundation
import PackageDescription

let infoPlistPath = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appendingPathComponent("Sources/VelaApp/Info.plist")
    .path

let package = Package(
    name: "VelaApp",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "VelaAvatar", targets: ["VelaAvatar"]),
        .library(name: "VelaIPC", targets: ["VelaIPC"]),
        .executable(name: "VelaApp", targets: ["VelaApp"]),
    ],
    targets: [
        .target(name: "VelaAvatar", dependencies: ["VelaIPC"]),
        .target(name: "VelaIPC"),
        .executableTarget(
            name: "VelaApp",
            dependencies: ["VelaAvatar", "VelaIPC"],
            exclude: ["Info.plist"],
            linkerSettings: [
                .unsafeFlags([
                    "-Xlinker", "-sectcreate",
                    "-Xlinker", "__TEXT",
                    "-Xlinker", "__info_plist",
                    "-Xlinker", infoPlistPath,
                ]),
            ]
        ),
        .testTarget(name: "VelaAvatarTests", dependencies: ["VelaAvatar"]),
        .testTarget(name: "VelaIPCTests", dependencies: ["VelaAvatar", "VelaIPC"]),
    ],
    swiftLanguageModes: [.v6]
)
