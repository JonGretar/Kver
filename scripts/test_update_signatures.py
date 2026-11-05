import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

# Signed fixtures from the published RFC 8032 test vector. Only public keys and
# signatures are stored here; the verifier never needs a private signing key.
PUBLIC_KEY = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo="
ARCHIVE_SIGNATURE = (
    "XbAGconLxJ/8vGNg2WL/GE2wbgEI4kBuQQ0Ci1Aobcf0JUa24c/dezMzcZtkzjkG"
    "sjV6gDUTcg5kUGvqUxDjAQ=="
)
FEED = (
    '<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">'
    '<channel><item><enclosure length="13" sparkle:edSignature="'
    + ARCHIVE_SIGNATURE
    + '"/></item></channel></rss>'
).encode()
TRAILER = (
    "<!-- sparkle-signatures:\n"
    "edSignature: f3ohswaBm/8ixrjV4Afl4Umtj+f2ogqy9sNlBPaAI08Qr5fivxlQ9B6aNTf59XG5"
    "IJjHyPQk1uKuaqtEXxduAA==\n"
    "length: 246\n-->\n"
).encode()


@unittest.skipUnless(sys.platform == "darwin" and shutil.which("xcrun"), "Requires macOS CryptoKit")
class UpdateSignatureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="kver-signature-tests-")
        cls.addClassCleanup(cls.directory.cleanup)
        cls.root = pathlib.Path(cls.directory.name)
        cls.verifier = cls.root / "verify-update-signatures"
        subprocess.run(
            [
                "xcrun",
                "swiftc",
                pathlib.Path(__file__).with_name("verify-update-signatures.swift"),
                "-o",
                cls.verifier,
            ],
            check=True,
        )

    def verify(self, feed=FEED + TRAILER, archive=b"test archive\n", key=PUBLIC_KEY):
        (self.root / "appcast.xml").write_bytes(feed)
        (self.root / "archive.zip").write_bytes(archive)
        return subprocess.run(
            [self.verifier, self.root / "appcast.xml", self.root / "archive.zip"],
            env={**os.environ, "KVER_UPDATE_PUBLIC_KEY": key},
            capture_output=True,
            text=True,
        )

    def test_accepts_matching_feed_and_archive(self):
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_mismatched_public_key(self):
        result = self.verify(key="AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("feed signature", result.stderr)

    def test_rejects_tampered_or_unsigned_feeds(self):
        for feed in (
            FEED,
            FEED.replace(b"13", b"14") + TRAILER,
            FEED + TRAILER.replace(b"246", b"245"),
            FEED + TRAILER + b"unsigned data",
        ):
            with self.subTest(feed=feed):
                self.assertNotEqual(self.verify(feed=feed).returncode, 0)

    def test_rejects_tampered_archives(self):
        for archive in (b"test archivf\n", b"test archive\nextra", b""):
            with self.subTest(archive=archive):
                result = self.verify(archive=archive)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("archive signature or length", result.stderr)


if __name__ == "__main__":
    unittest.main()
