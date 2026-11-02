// swift-tools-version: 6.0

import PackageDescription

let package = Package(
  name: "Mrow",
  platforms: [.macOS(.v15)],
  products: [
    .executable(name: "Mrow", targets: ["Mrow"]),
  ],
  targets: [
    // The AppKit app, with its content in SwiftUI. Talks to GitLab only through `glab`.
    .executableTarget(
      name: "Mrow",
      swiftSettings: [.swiftLanguageMode(.v5)]
    ),
  ]
)
