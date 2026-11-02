// Draws Mrow's icon in Mdow's style: a paper squircle with a heavy geometric glyph, here an "r"
// with a clay dot for "waiting on you".
//   swift scripts/make-icon.swift Resources/AppIcon.icns
import AppKit

let output = CommandLine.arguments.dropFirst().first ?? "Resources/AppIcon.icns"
let iconset = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("Mrow.iconset")
try? FileManager.default.removeItem(at: iconset)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)

func color(_ hex: UInt32) -> NSColor {
  NSColor(
    srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
    blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

func draw(size: Int) -> Data {
  let s = CGFloat(size)
  let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
  // The macOS icon grid: an 824pt squircle inside 1024.
  let unit = s / 1024
  let plate = NSRect(x: 100 * unit, y: 100 * unit, width: 824 * unit, height: 824 * unit)
  let squircle = NSBezierPath(roundedRect: plate, xRadius: 185 * unit, yRadius: 185 * unit)
  let shadow = NSShadow()
  shadow.shadowColor = NSColor.black.withAlphaComponent(0.18)
  shadow.shadowBlurRadius = 18 * unit
  shadow.shadowOffset = NSSize(width: 0, height: -6 * unit)
  NSGraphicsContext.saveGraphicsState()
  shadow.set()
  color(0xFDFCF9).setFill()
  squircle.fill()
  NSGraphicsContext.restoreGraphicsState()
  color(0x3C2D1E).withAlphaComponent(0.08).setStroke()
  squircle.lineWidth = 2 * unit
  squircle.stroke()

  // The "r": a stem and a shoulder that rounds into an arm, like Mdow's "m" cut in half.
  let ink = color(0x1E1C19)
  ink.setFill()
  let stemX = 360 * unit
  let bottom = 330 * unit
  let top = 650 * unit
  let stroke = 118 * unit
  let r = NSBezierPath()
  r.move(to: NSPoint(x: stemX, y: bottom))
  r.line(to: NSPoint(x: stemX, y: top))
  r.line(to: NSPoint(x: 545 * unit, y: top))
  r.appendArc(
    withCenter: NSPoint(x: 545 * unit, y: top - 90 * unit), radius: 90 * unit, startAngle: 90, endAngle: 0,
    clockwise: true)
  r.line(to: NSPoint(x: 635 * unit, y: top - 140 * unit))
  r.line(to: NSPoint(x: 635 * unit - stroke, y: top - 140 * unit))
  r.line(to: NSPoint(x: 635 * unit - stroke, y: top - 90 * unit))
  r.appendArc(
    withCenter: NSPoint(x: 545 * unit, y: top - 90 * unit), radius: 90 * unit - stroke, startAngle: 0,
    endAngle: 90, clockwise: false)
  r.line(to: NSPoint(x: stemX + stroke, y: top - stroke))
  r.line(to: NSPoint(x: stemX + stroke, y: bottom))
  r.close()
  r.fill()

  // The clay dot.
  color(0xB95A33).setFill()
  NSBezierPath(ovalIn: NSRect(x: 590 * unit, y: bottom, width: 104 * unit, height: 104 * unit)).fill()

  NSGraphicsContext.restoreGraphicsState()
  return rep.representation(using: .png, properties: [:])!
}

for (points, scales) in [(16, [1, 2]), (32, [1, 2]), (128, [1, 2]), (256, [1, 2]), (512, [1, 2])] {
  for scale in scales {
    let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
    try draw(size: points * scale).write(to: iconset.appendingPathComponent(name))
  }
}
let task = Process()
task.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
task.arguments = ["-c", "icns", iconset.path, "-o", output]
try task.run()
task.waitUntilExit()
print("Wrote \(output)")
