// swift-tools-version: 6.1

import PackageDescription

let package = Package(
    name: "VelaApp",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "VelaIPC", targets: ["VelaIPC"]),
        .executable(name: "VelaApp", targets: ["VelaApp"]),
    ],
    targets: [
        .target(name: "VelaIPC"),
        .executableTarget(name: "VelaApp", dependencies: ["VelaIPC"]),
        .testTarget(name: "VelaIPCTests", dependencies: ["VelaIPC"]),
    ],
    swiftLanguageModes: [.v6]
)
