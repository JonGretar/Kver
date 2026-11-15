"""Platform-independent ZIP rejection tests; native success is checked on Windows."""

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
import warnings
import zipfile

spec = importlib.util.spec_from_file_location(
    "windows_package", Path(__file__).parent / "windows/package.py"
)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ArchiveRejectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.archive = Path(self.temp.name) / "candidate.zip"
        self.hashes = {"kver.exe": hashlib.sha256(b"executable").hexdigest()}
        self.sums = f"{self.hashes['kver.exe']}  kver.exe\n".encode()

    def write(self, entries):
        with zipfile.ZipFile(self.archive, "w") as zipped:
            for name, content in entries:
                # Preserve malicious names verbatim: ZipInfo's constructor
                # normalizes backslashes on Windows before the verifier sees them.
                info = zipfile.ZipInfo("placeholder")
                info.filename = name
                zipped.writestr(info, content)

    def reject(self, message):
        with self.assertRaisesRegex(ValueError, message):
            package.verify_archive(self.archive, "0.6.0", self.hashes)

    def test_missing_executable(self):
        self.write([("SHA256SUMS", self.sums)])
        self.reject("Missing or unexpected")

    def test_missing_checksums(self):
        self.write([("kver.exe", b"executable")])
        self.reject("Missing or unexpected")

    def test_unexpected_sidecar(self):
        self.write([("kver.exe", b"executable"), ("SHA256SUMS", self.sums),
                    ("unreviewed.dll", b"sidecar")])
        self.reject("Missing or unexpected")

    def test_corrupted_executable(self):
        self.write([("kver.exe", b"corruption"), ("SHA256SUMS", self.sums)])
        self.reject("Extracted checksum mismatch")

    def test_incorrect_checksum_inventory(self):
        self.write([("kver.exe", b"executable"), ("SHA256SUMS", b"bad")])
        self.reject("Incorrect checksum inventory")

    def test_duplicate_paths(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            self.write([("kver.exe", b"executable"), ("kver.exe", b"other"),
                        ("SHA256SUMS", self.sums)])
        self.reject("Duplicate ZIP paths")

    def test_unsafe_inventory_cannot_escape_extraction(self):
        for path in ("../escaped", "/absolute", r"folder\escaped", "C:escaped"):
            with self.subTest(path=path):
                self.hashes = {path: hashlib.sha256(b"bad").hexdigest()}
                self.write([(path, b"bad"), ("SHA256SUMS", b"irrelevant")])
                self.reject("Unsafe ZIP paths")


if __name__ == "__main__":
    unittest.main()
