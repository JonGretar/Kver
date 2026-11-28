"""Exercise rejection at the real tar extraction seam, without pretending to build Linux."""

import hashlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "linux_package", Path(__file__).parents[1] / "linux/package.py"
)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class ArchiveRejectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.archive = Path(self.temp.name) / "candidate.tar.gz"
        self.prefix = "Kver-v0.6.0-linux-x86_64/"
        self.files = {name: b"content" for name in package.REQUIRED}
        self.files["licenses/rust/licenses/MIT"] = b"license"
        self.hashes = {name: hashlib.sha256(data).hexdigest()
                       for name, data in self.files.items()}
        self.files["SHA256SUMS"] = "".join(
            f"{checksum}  {name}\n" for name, checksum in sorted(self.hashes.items())
        ).encode()

    def write(self, files=None, mode=None, link=None):
        with tarfile.open(self.archive, "w:gz") as tar:
            for name, data in (files or self.files).items():
                info = tarfile.TarInfo(self.prefix + name)
                info.size = len(data)
                info.mode = mode if mode is not None else (0o755 if name == "kver" else 0o644)
                if name == link:
                    info.type = tarfile.SYMTYPE
                    info.linkname = "/tmp/escape"
                tar.addfile(info, io.BytesIO(data))

    def reject(self, message):
        with self.assertRaisesRegex(ValueError, message):
            package.verify_archive(self.archive, "0.6.0", self.hashes)

    def test_missing_executable(self):
        del self.files["kver"]
        self.write()
        self.reject("Missing or unexpected")

    def test_incomplete_expected_inventory(self):
        del self.hashes["LICENSE"]
        self.write()
        self.reject("Missing required inventory")

    def test_missing_rust_licenses(self):
        del self.hashes["licenses/rust/licenses/MIT"]
        self.write()
        self.reject("Missing Rust license")

    def test_unexpected_sidecar(self):
        self.files["unreviewed.so"] = b"library"
        self.write()
        self.reject("Missing or unexpected")

    def test_missing_checksums(self):
        del self.files["SHA256SUMS"]
        self.write()
        self.reject("Missing or unexpected")

    def test_corrupted_executable(self):
        self.files["kver"] = b"corrupt"
        self.write()
        self.reject("Extracted checksum mismatch")

    def test_incorrect_checksums(self):
        self.files["SHA256SUMS"] = b"bad"
        self.write()
        self.reject("Incorrect checksum inventory")

    def test_wrong_version_root(self):
        self.prefix = "Kver-v9.9.9-linux-x86_64/"
        self.write()
        self.reject("Missing or unexpected")

    def test_missing_executable_permission(self):
        self.write(mode=0o644)
        self.reject("Incorrect permissions")

    def test_special_permissions(self):
        self.write(mode=0o4755)
        self.reject("Incorrect permissions")

    def test_symlink(self):
        self.write(link="kver")
        self.reject("Unsafe tar member")

    def test_unsafe_paths(self):
        for prefix in ("../escape/", "/absolute/", "folder\\escape/", "C:escape/"):
            with self.subTest(prefix=prefix):
                self.prefix = prefix
                self.write()
                self.reject("Unsafe tar member")

    def test_duplicate_paths(self):
        with tarfile.open(self.archive, "w:gz") as tar:
            for _ in range(2):
                tar.addfile(tarfile.TarInfo(self.prefix + "kver"))
        self.reject("Duplicate tar paths")

    def test_non_elf_executable(self):
        executable = Path(self.temp.name) / "kver"
        executable.write_bytes(b"not an ELF executable")
        with self.assertRaisesRegex(ValueError, "ELF64 x86-64"):
            package.inspect_elf(executable)


if __name__ == "__main__":
    unittest.main()
