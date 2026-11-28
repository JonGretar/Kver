"""Run the packaged installer, OpenSSL and the update helper in a disposable HOME."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

LINUX = Path(__file__).resolve().parents[1] / "linux"
if sys.platform == "linux":
    spec = importlib.util.spec_from_file_location("update", LINUX / "update.py")
    update = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(update)


@unittest.skipUnless(sys.platform == "linux", "Linux installer/renameat2")
class ManagedUpdateTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.base = Path(temp.name)
        home = self.base / "user home"
        env = patch.dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(home / ".local/share"))
        env.start()
        self.addCleanup(env.stop)
        self.exe = home / ".local/opt/kver/kver"
        self.key = self.base / "test-key"
        subprocess.run(["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048",
                        "-out", self.key], check=True, stderr=subprocess.DEVNULL)
        self.pub = subprocess.check_output(["openssl", "pkey", "-in", self.key, "-pubout"], text=True)
        self.original = self.package("1.0.0")
        subprocess.run([sys.executable, self.original / "install.py", "install"], check=True,
                       stdout=subprocess.DEVNULL)
        self.identity = update.digest(self.exe)
        self.new = self.package("1.1.0")
        self.archive = self.base / (self.new.name + ".tar.gz")
        with tarfile.open(self.archive, "w:gz") as tar:
            for path in self.new.rglob("*"):
                if path.is_file():
                    tar.add(path, arcname=path.relative_to(self.base), recursive=False)
        self.sums = Path(str(self.archive) + ".sha256")
        self.sums.write_text(f"{update.digest(self.archive)}  {self.archive.name}\n")
        subprocess.run(["openssl", "dgst", "-sha256", "-sign", self.key, "-out",
                        str(self.sums) + ".sig", self.sums], check=True)

    def package(self, version):
        path = self.base / f"Kver-v{version}-linux-x86_64"
        (path / "resources").mkdir(parents=True)
        (path / "resources/kver.png").write_bytes(b"icon")
        shutil.copyfile(LINUX / "install.py", path / "install.py")
        (path / "BUILD-INFO.json").write_text(json.dumps(dict(version=version, target="x86_64-unknown-linux-gnu")))
        (path / "kver").write_text(f'#!/bin/sh\nif [ "$1" = --version ]; then echo "kver {version}"; '
                                   f'else echo "{version}" > "$HOME/relaunched"; fi\n')
        (path / "kver").chmod(0o755)
        files = sorted(p for p in path.rglob("*") if p.is_file())
        (path / "SHA256SUMS").write_text("".join(f"{update.digest(p)}  {p.relative_to(path)}\n" for p in files))
        return path

    def replace(self):
        update.replace(self.exe, self.base, "1.1.0", self.pub, self.identity)

    def test_install_and_relaunch_only_after_quit(self):
        stage = self.base / "kver-update-staged"
        stage.mkdir(mode=0o700)
        with patch.object(update, "fetch", side_effect=lambda url, dest, limit:
                          shutil.copyfile(self.base / url.split("/")[-1], dest)):
            update.prepare(stage, "1.1.0", self.pub)
        with subprocess.Popen(["sleep", "20"]) as reader:
            lock = self.base / "instance.lock"
            lock.touch()
            helper = subprocess.Popen([sys.executable, "-I", LINUX / "update.py", "restart",
                                       self.exe, self.pub, "1.1.0", stage, str(reader.pid), self.identity, lock])
            try:
                time.sleep(0.1)
                self.assertIsNone(helper.poll())
                self.assertEqual(update.digest(self.exe), self.identity)
                reader.terminate()
                reader.wait(3)
                self.assertEqual(helper.wait(10), 0)
            finally:
                helper.kill()
                helper.wait()
        self.assertEqual((Path.home() / "relaunched").read_text().strip(), "1.1.0")
        update.owned(self.exe, update.digest(self.new / "kver"))
        self.assertEqual(update.digest(self.original / "kver"), self.identity)
        self.assertFalse(stage.exists())

    def test_refuses_portable_unowned_or_modified_copy(self):
        with self.assertRaises(ValueError):
            update.owned(self.original / "kver")
        extra = self.exe.parent / "private"
        extra.mkdir()
        with self.assertRaisesRegex(ValueError, "unowned directories"):
            self.replace()
        extra.rmdir()
        self.exe.write_text("manually compiled binary")
        with self.assertRaisesRegex(ValueError, "Installation changed"):
            self.replace()
        self.assertEqual(self.exe.read_text(), "manually compiled binary")
        (self.exe.parent / ".install.json").unlink()
        with self.assertRaises((ValueError, FileNotFoundError)):
            update.owned(self.exe)

    def test_tampered_archive_and_sidecar_leave_original_untouched(self):
        with self.archive.open("ab") as file:
            file.write(b"changed")
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.replace()
        self.sums.write_text(f"{update.digest(self.archive)}  {self.archive.name}\n")
        with self.assertRaisesRegex(ValueError, "signature"):
            self.replace()
        self.assertEqual(update.digest(self.exe), self.identity)

    def test_rechecks_identity_and_never_overrides_an_active_installer(self):
        import fcntl
        with self.assertRaisesRegex(ValueError, "no longer installed"):
            update.replace(self.exe, self.base, "1.1.0", self.pub, "stale")
        fd = os.open(self.exe.parent.parent, os.O_DIRECTORY)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX)
            with self.assertRaises(BlockingIOError):
                self.replace()
        finally:
            os.close(fd)
        self.assertEqual(update.digest(self.exe), self.identity)

    def test_helper_distinguishes_running_installed_copy_from_identical_portable_binary(self):
        # A real ELF parent is needed for /proc/PID/exe. Package the Python
        # interpreter so the fixture can spawn the same helper as the Rust app.
        portable = self.original / "kver"
        shutil.copyfile(sys.executable, portable)
        sums = self.original / "SHA256SUMS"
        sums.write_text(sums.read_text().replace(self.identity, update.digest(portable)))
        subprocess.run([sys.executable, self.original / "install.py", "install"], check=True,
                       stdout=subprocess.DEVNULL)
        probe = "import subprocess,sys;sys.exit(subprocess.call(sys.argv[1:], stdin=open('/proc/self/exe', 'rb')))"
        def launch(binary):
            return subprocess.run([binary, "-c", probe, sys.executable, "-I", LINUX / "update.py",
                                   "eligible", self.exe, self.pub], capture_output=True, text=True)
        result = launch(self.exe)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), update.digest(self.exe))
        self.assertNotEqual(launch(portable).returncode, 0)
