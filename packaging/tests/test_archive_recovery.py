"""Verify recovery keeps the tagged scripts and replaces only their licence policy."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).parents[1] / "common/prepare-archive-recovery.py"
spec = importlib.util.spec_from_file_location("archive_recovery", SCRIPT)
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


class ArchiveRecoveryTests(unittest.TestCase):
    def test_tagged_layouts(self):
        for platform in ("linux", "windows"):
            for tools in ("scripts", "packaging"):
                with self.subTest(platform=platform, tools=tools):
                    self.verify_layout(platform, tools)

    def verify_layout(self, platform, tools):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            env_file = root / "github-env"
            expected_version = root / "verified-version"
            previous = Path.cwd()
            os.chdir(root)
            try:
                self.git("init", "-q")
                self.git("config", "user.name", "Packaging Test")
                self.git("config", "user.email", "packaging@example.invalid")
                self.git("config", "commit.gpgsign", "false")
                self.git("config", "core.hooksPath", str(root / "no-hooks"))
                policy = Path(tools) / platform / "about.toml"
                policy.parent.mkdir(parents=True)
                policy.write_text("old policy", encoding="utf-8")
                metadata_dir = Path(tools) if tools == "scripts" else Path(tools) / "common"
                metadata_dir.mkdir(exist_ok=True)
                metadata = metadata_dir / "release-metadata.py"
                metadata.write_text(
                    "import sys\nfrom pathlib import Path\n"
                    "assert sys.argv[1:] == ['--tag', 'v0.8.0']\n"
                    f"Path({str(expected_version)!r}).touch()\n",
                    encoding="utf-8",
                )
                self.git("add", ".")
                self.git("commit", "-qm", "Tagged source")
                self.git("tag", "v0.8.0")
                self.git("remote", "add", "origin", ".")
                current = Path("packaging") / platform / "about.toml"
                current.parent.mkdir(parents=True, exist_ok=True)
                current.write_text("current policy", encoding="utf-8")
                self.git("add", ".")
                self.git("commit", "-qm", "Updated policy")

                def run(*command):
                    if command[0] == "gh":
                        self.assertEqual(command, ("gh", "release", "view", "--repo",
                                                   "owner/project", "--", "v0.8.0"))
                    else:
                        subprocess.run(command, check=True, capture_output=True)

                with patch.object(recovery, "run", run), patch.dict(os.environ, {
                    "GITHUB_ENV": str(env_file), "GITHUB_REPOSITORY": "owner/project",
                }):
                    recovery.prepare(platform, "v0.8.0")

                self.assertTrue(expected_version.exists())
                self.assertEqual(policy.read_text(encoding="utf-8"), "current policy")
                self.assertEqual(self.git("diff", "--name-only").strip(), policy.as_posix())
                self.assertEqual(self.git("rev-parse", "HEAD"), self.git("rev-parse", "v0.8.0"))
                extension = "sh" if platform == "linux" else "ps1"
                entry = (f"scripts/package-{platform}.{extension}" if tools == "scripts"
                         else f"packaging/{platform}/package.{extension}")
                tests = "scripts" if tools == "scripts" else "packaging/tests"
                self.assertEqual(env_file.read_text(encoding="utf-8"),
                                 f"PACKAGE_TOOLS={tools}\nPACKAGE_METADATA={metadata.as_posix()}\n"
                                 f"PACKAGE_TESTS={tests}\nPACKAGE_ENTRY={entry}\n")
            finally:
                os.chdir(previous)

    @staticmethod
    def git(*args):
        return subprocess.check_output(["git", *args], text=True)
