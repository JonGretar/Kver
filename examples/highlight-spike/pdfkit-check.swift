// macOS PDFKit parsing/rendering interoperability check, not Preview UI automation.
import AppKit
import Foundation
import PDFKit

guard CommandLine.arguments.count == 2 else {
    fatalError("Usage: swift pdfkit-check.swift OUTPUT_DIRECTORY")
}
let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
let name = "kver-spike-selection:"
for (file, count) in [("original", 0), ("highlighted", 4), ("recolored", 4), ("deleted", 0)] {
    let url = directory.appendingPathComponent("\(file).pdf")
    guard let document = PDFDocument(url: url) else { fatalError("Cannot load \(url)") }
    precondition(document.pageCount == 4)
    var found = 0
    for index in 0..<document.pageCount {
        let page = document.page(at: index)!
        let highlights = page.annotations.filter { ($0.contents ?? "").hasPrefix(name) }
        found += highlights.count
        for annotation in highlights {
            precondition(annotation.type == "Highlight")
            precondition(annotation.quadrilateralPoints?.count == 12)
            let color = annotation.color.usingColorSpace(.deviceRGB)!
            let expected: [CGFloat] = file == "recolored" ? [1.0, 0.25, 0.65] : [1.0, 0.85, 0.0]
            precondition(abs(color.redComponent - expected[0]) < 0.01)
            precondition(abs(color.greenComponent - expected[1]) < 0.01)
            precondition(abs(color.blueComponent - expected[2]) < 0.01)
        }
        precondition(page.annotations.contains { $0.type == "Link" })
        precondition(page.annotations.contains { ($0.contents ?? "").hasPrefix("existing-highlight:") })
        let image = page.thumbnail(of: NSSize(width: 600, height: 600), for: .cropBox)
        let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
        let png = bitmap.representation(using: .png, properties: [:])!
        try png.write(to: directory.appendingPathComponent("pdfkit-\(file)-\(index).png"))
    }
    precondition(found == count)
    print("PASS PDFKit \(file): \(found) spike annotations, geometry/color and existing link/highlight")
}

// Let an independent native PDF engine modify/save a Rust-created annotation.
let document = PDFDocument(url: directory.appendingPathComponent("highlighted.pdf"))!
for index in 0..<document.pageCount {
    for annotation in document.page(at: index)!.annotations
    where (annotation.contents ?? "").hasPrefix(name) {
        annotation.color = NSColor(deviceRed: 0.15, green: 0.8, blue: 0.25, alpha: 0.45)
    }
}
let edited = directory.appendingPathComponent("pdfkit-edited.pdf")
precondition(document.write(to: edited))
let reopened = PDFDocument(url: edited)!
precondition(reopened.pageCount == 4)
for index in 0..<reopened.pageCount {
    let annotations = reopened.page(at: index)!.annotations.filter { ($0.contents ?? "").hasPrefix(name) }
    precondition(annotations.count == 1)
    precondition(annotations[0].quadrilateralPoints?.count == 12)
    let color = annotations[0].color.usingColorSpace(.deviceRGB)!
    precondition(abs(color.greenComponent - 0.8) < 0.01)
}
print("PASS PDFKit can recolor, save, and reopen Rust-created highlights")
