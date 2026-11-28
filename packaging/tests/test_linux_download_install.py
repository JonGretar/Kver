"""Exercise the POSIX bootstrap with fixture downloads and no network."""

import hashlib
import io
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tarfile
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "linux/install.sh"
RELEASES = "https://github.com/JonGretar/Kver/releases"
BUNDLE = "Kver-v1.2.3-linux-x86_64"
ARCHIVE = BUNDLE + ".tar.gz"
LEGACY = "Kver-v1.2.3-experimental-linux-x86_64"


class LinuxDownloadInstallTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.stage = self.root / "stage space"
        self.stage.mkdir()
        self.env = dict(os.environ, TMPDIR=str(self.stage),
                        PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        FIXTURE=str(self.root), MACHINE="Linux x86_64",
                        LATEST=RELEASES + "/tag/v1.2.3")
        self.executable("uname", '#!/bin/sh\nprintf "%s\\n" "$MACHINE"\n')
        self.executable("python3", "#!/bin/sh\nexec " + shlex.quote(sys.executable) + ' "$@"\n')
        handler = self.root / "curl.py"
        handler.write_text(f"""
import os
from pathlib import Path
import shutil
import sys
root = Path(os.environ["FIXTURE"])
url = sys.argv[-1]
with (root / "requests").open("a") as requests:
    print(url, file=requests)
if url == {RELEASES + '/latest'!r}:
    print(os.environ["LATEST"], end="")
else:
    name = url.removeprefix({RELEASES + '/download/v1.2.3/'!r})
    allowed = ({ARCHIVE!r}, {ARCHIVE + '.sha256'!r},
               {LEGACY + '.tar.gz'!r}, {LEGACY + '.tar.gz.sha256'!r})
    if name not in allowed or not (root / name).exists():
        if "-w" in sys.argv:
            print(os.environ.get("MISSING_STATUS", "404"), end="")
        sys.exit(22)
    shutil.copyfile(root / name, sys.argv[sys.argv.index("-o") + 1])
    if "-w" in sys.argv:
        print("200", end="")
""")
        self.executable("curl", "#!/bin/sh\nexec " + shlex.quote(sys.executable) +
                        " " + shlex.quote(str(handler)) + ' "$@"\n')
        self.archive, self.checksum = self.make_archive(BUNDLE)

    def make_archive(self, bundle):
        archive = self.root / (bundle + ".tar.gz")
        checksum = archive.with_suffix(".gz.sha256")
        with tarfile.open(archive, "w:gz") as tar:
            body = (b'import os, sys\nfrom pathlib import Path\n'
                    b'assert sys.argv[1:] == ["install"]\n'
                    b'Path(os.environ["FIXTURE"], "installed").touch()\n')
            member = tarfile.TarInfo(bundle + "/install.py")
            member.size = len(body)
            tar.addfile(member, io.BytesIO(body))
        checksum.write_text(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
        return archive, checksum

    def executable(self, name, text):
        path = self.bin / name
        path.write_text(text)
        path.chmod(0o755)

    def run_installer(self):
        result = subprocess.run(["sh", str(SCRIPT)], env=self.env, cwd=self.root,
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(list(self.stage.iterdir()), [])
        return result

    def assert_refused(self, message):
        result = self.run_installer()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(message, result.stderr)
        self.assertFalse((self.root / "installed").exists())

    def test_verifies_then_calls_packaged_installer(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "installed").exists())

    def test_bad_checksum_never_executes_installer(self):
        self.checksum.write_text(f"{'0' * 64}  {ARCHIVE}\n")
        self.assert_refused("Checksum verification failed")

    def test_old_filename_is_supported_on_the_same_release(self):
        self.checksum.unlink()
        self.make_archive(LEGACY)
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "installed").exists())

    def test_server_error_is_not_treated_as_an_old_filename(self):
        self.checksum.unlink()
        self.make_archive(LEGACY)
        self.env["MISSING_STATUS"] = "503"
        self.assert_refused("Could not download")
        self.assertEqual((self.root / "requests").read_text().splitlines()[-1],
                         RELEASES + "/download/v1.2.3/" + ARCHIVE + ".sha256")

    def test_missing_latest_archive_does_not_fall_back(self):
        self.archive.unlink()
        self.assert_refused("may not be published yet")
        self.assertEqual((self.root / "requests").read_text().splitlines(), [
            RELEASES + "/latest",
            RELEASES + "/download/v1.2.3/" + ARCHIVE + ".sha256",
            RELEASES + "/download/v1.2.3/" + ARCHIVE,
        ])

    def test_unsupported_platform_does_not_download(self):
        self.env["MACHINE"] = "Linux aarch64"
        self.assert_refused("Linux x86-64 only")
        self.assertFalse((self.root / "requests").exists())

    def test_invalid_release_redirect_does_not_download_assets(self):
        self.env["LATEST"] = RELEASES + "/tag/v1.2.3-rc1"
        self.assert_refused("Unexpected release tag")
        self.assertEqual((self.root / "requests").read_text().splitlines(), [RELEASES + "/latest"])
