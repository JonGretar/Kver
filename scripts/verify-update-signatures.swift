// Independently verify generated artifacts against the public key embedded in
// the app. generate_appcast only warns (and exits zero) for mismatched keys.
// Uses macOS CryptoKit; no private key is read by this tool.
import CryptoKit
import Foundation

func verify() throws {
    guard CommandLine.arguments.count == 3,
          let encodedKey = ProcessInfo.processInfo.environment["KVER_UPDATE_PUBLIC_KEY"],
          let keyData = Data(base64Encoded: encodedKey) else {
        throw Failure("Usage: KVER_UPDATE_PUBLIC_KEY=… swift verify-update-signatures.swift appcast.xml archive.zip")
    }
    let key = try Curve25519.Signing.PublicKey(rawRepresentation: keyData)
    let feed = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))
    let archive = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[2]))

    // Sparkle signs the byte prefix preceding its final signature comment.
    let marker = Data("<!-- sparkle-signatures:\n".utf8)
    guard let boundary = feed.range(of: marker, options: .backwards),
          let trailer = String(data: feed[boundary.lowerBound...], encoding: .utf8) else {
        throw Failure("The update feed has no signature.")
    }
    let pattern = #"\A<!-- sparkle-signatures:\nedSignature: ([A-Za-z0-9+/=]+)\nlength: ([0-9]+)\n-->\n?\z"#
    let expression = try NSRegularExpression(pattern: pattern)
    let range = NSRange(trailer.startIndex..., in: trailer)
    guard let match = expression.firstMatch(in: trailer, range: range),
          let signatureRange = Range(match.range(at: 1), in: trailer),
          let lengthRange = Range(match.range(at: 2), in: trailer),
          let signature = Data(base64Encoded: String(trailer[signatureRange])),
          let length = Int(trailer[lengthRange]),
          length == boundary.lowerBound,
          key.isValidSignature(signature, for: feed.prefix(length)) else {
        throw Failure("The update feed signature does not match KVER_UPDATE_PUBLIC_KEY.")
    }

    let xml = try XMLDocument(data: feed.prefix(boundary.lowerBound), options: .nodeLoadExternalEntitiesNever)
    let enclosures = try xml.nodes(forXPath: "/rss/channel/item/enclosure")
    guard enclosures.count == 1,
          let enclosure = enclosures.first as? XMLElement,
          let archiveLength = enclosure.attribute(forName: "length")?.stringValue,
          Int(archiveLength) == archive.count,
          let encodedSignature = enclosure.attribute(
            forLocalName: "edSignature",
            uri: "http://www.andymatuschak.org/xml-namespaces/sparkle"
          )?.stringValue,
          let archiveSignature = Data(base64Encoded: encodedSignature),
          key.isValidSignature(archiveSignature, for: archive) else {
        throw Failure("The update archive signature or length is invalid.")
    }
    print("Verified update feed and archive against KVER_UPDATE_PUBLIC_KEY.")
}

struct Failure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

do {
    try verify()
} catch {
    FileHandle.standardError.write(Data("Update verification failed: \(error)\n".utf8))
    exit(1)
}
