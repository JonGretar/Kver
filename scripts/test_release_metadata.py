import importlib.util
import contextlib
import io
import pathlib
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = pathlib.Path(__file__).with_name("release-metadata.py")
spec = importlib.util.spec_from_file_location("release_metadata", SCRIPT)
release_metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_metadata)


class ReleaseMetadataTests(unittest.TestCase):
    def test_metadata_reads_utf8_under_legacy_windows_locale(self):
        read_text = pathlib.Path.read_text

        def legacy_read_text(path, encoding=None, errors=None):
            return read_text(path, encoding=encoding or "cp1252", errors=errors)

        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / "Cargo.toml").write_text(
                '[package]\nname = "kver"\nversion = "1.2.3"\n', encoding="utf-8"
            )
            (root / "CHANGELOG.md").write_text(
                "## [1.2.3] - 2026-02-28\n\nChoose “New window”.\n", encoding="utf-8"
            )
            output = io.StringIO()
            with (
                patch.object(release_metadata, "ROOT", root),
                patch("sys.argv", [str(SCRIPT), "--tag", "v1.2.3"]),
                patch.object(pathlib.Path, "read_text", legacy_read_text),
                contextlib.redirect_stdout(output),
            ):
                self.assertEqual(release_metadata.main(), 0)
            self.assertEqual(output.getvalue(), "1.2.3\n")

    def test_valid_release(self):
        release_metadata.validate("v1.2.3", "1.2.3", "## [Unreleased]\n\n## [1.2.3] - 2026-02-28\n")

    def test_rejects_invalid_tags(self):
        for tag in ("1.2.3", "v01.2.3", "v1.2", "v1.2.3-rc1", "v1.2.4", "v1.2.3\n"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release_metadata.validate(tag, "1.2.3", "## [1.2.3] - 2026-02-28\n")

    def test_requires_exactly_one_real_dated_section(self):
        for changelog in (
            "## [Unreleased]\n",
            "## [1.2.3]\n",
            "## [1.2.3] - 2026-02-30\n",
            "## [1.2.3] - 2026-02-28\n## [1.2.3] - 2026-02-28\n",
            "## [1.2.4] - 2026-02-28\n",
        ):
            with self.subTest(changelog=changelog), self.assertRaises(ValueError):
                release_metadata.validate("v1.2.3", "1.2.3", changelog)

    def test_package_version_comes_from_package_section(self):
        manifest = '[package]\nname = "kver"\nversion = "2.3.4"\n[dependencies]\nexample = "9.8.7"\n'
        self.assertEqual(release_metadata.package_version(manifest), "2.3.4")
        with self.assertRaises(ValueError):
            release_metadata.package_version(manifest.replace("2.3.4", "2.3.4-rc1"))
        with self.assertRaises(ValueError):
            release_metadata.package_version('[dependencies]\nexample = "9.8.7"\n')

    def test_release_notes_include_only_the_validated_changelog_chapter(self):
        changelog = (
            "## [Unreleased]\n\n"
            "## [1.2.3] - 2026-02-28\n\n"
            "### Added\n\n- Find matches with snippets.\n\n"
            "## [1.2.2] - 2026-02-01\n\n"
            "- Older changes.\n"
        )
        release_metadata.validate("v1.2.3", "1.2.3", changelog)
        self.assertEqual(
            release_metadata.changelog_chapter("1.2.3", changelog),
            "## [1.2.3] - 2026-02-28\n\n### Added\n\n- Find matches with snippets.",
        )
        with self.assertRaises(ValueError):
            release_metadata.changelog_chapter("1.2.4", changelog)


if __name__ == "__main__":
    unittest.main()
