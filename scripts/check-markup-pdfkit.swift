// macOS PDFKit interchange, not Preview or Acrobat UI automation.
// Generate inputs first:
// corgi test --lib all_created_types_render_independently -- --ignored
// swift scripts/check-markup-pdfkit.swift target/markup-interchange
// Recolor checks below establish stored properties, not regenerated appearance.
// PDFKit currently warns "Unhandled annotation subtype: /Squiggly" on recolor;
// its rewritten Squiggly can lose visible paint despite retaining geometry/color.
// Original Kver APs render correctly. Native Preview editing is a separate check.
import AppKit
import Foundation
import PDFKit

guard CommandLine.arguments.count == 2 else {
    fatalError("Usage: swift scripts/check-markup-pdfkit.swift SPECIMEN_DIRECTORY")
}
let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
for kind in ["Highlight", "Underline", "StrikeOut", "Squiggly"] {
    for rotation in [0, 90, 180, 270] {
        let stem = "\(kind)-\(rotation)"
        let url = directory.appendingPathComponent("\(stem).pdf")
        guard let document = PDFDocument(url: url), let page = document.page(at: 0) else {
            fatalError("Cannot load \(url)")
        }
        precondition(page.rotation == rotation)
        precondition(page.annotations.count == 1)
        let annotation = page.annotations[0]
        precondition(annotation.type == kind)
        precondition(annotation.quadrilateralPoints?.count == 8)
        precondition(annotation.userName == "Kver interchange")
        precondition(annotation.contents == nil || annotation.contents == "")
        let color = annotation.color.usingColorSpace(.deviceRGB)!
        precondition(color.redComponent > 0.99)
        precondition(color.greenComponent < 0.01 && color.blueComponent < 0.01)
        let bounds = annotation.bounds
        let quads = annotation.quadrilateralPoints!
        let image = page.thumbnail(of: NSSize(width: 800, height: 800), for: .cropBox)
        let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
        try bitmap.representation(using: .png, properties: [:])!
            .write(to: directory.appendingPathComponent("pdfkit-\(stem).png"))

        // An independent reader can edit, save, and reopen standard markup.
        annotation.color = NSColor(deviceRed: 0, green: 0.8, blue: 0, alpha: 1)
        let edited = directory.appendingPathComponent("pdfkit-edited-\(stem).pdf")
        precondition(document.write(to: edited))
        let reopened = PDFDocument(url: edited)!
        let editedPage = reopened.page(at: 0)!
        precondition(editedPage.annotations.count == 1)
        let changed = editedPage.annotations[0]
        precondition(changed.type == kind)
        precondition(changed.userName == annotation.userName)
        precondition(changed.bounds == bounds)
        precondition(changed.quadrilateralPoints! == quads)
        let changedColor = changed.color.usingColorSpace(.deviceRGB)!
        precondition(changedColor.redComponent < 0.01)
        precondition(abs(changedColor.greenComponent - 0.8) < 0.01)
        editedPage.removeAnnotation(changed)
        let deleted = directory.appendingPathComponent("pdfkit-deleted-\(stem).pdf")
        precondition(reopened.write(to: deleted))
        precondition(PDFDocument(url: deleted)!.page(at: 0)!.annotations.isEmpty)
        print("PASS PDFKit \(stem): parse, original thumbnail, recolor property readback, delete readback")
    }
}
