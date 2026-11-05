#!/usr/bin/env python3
"""Validate release metadata and emit the package version or changelog chapter."""

import argparse
import datetime
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def package_version(manifest: str) -> str:
    in_package = False
    versions = []
    for line in manifest.splitlines():
        if re.match(r"^\[.*\]\s*$", line):
            in_package = line.strip() == "[package]"
        elif in_package:
            match = re.match(r'^\s*version\s*=\s*"([^"]+)"(?:\s*#.*)?\s*$', line)
            if match:
                versions.append(match.group(1))
    if len(versions) != 1 or not TAG.fullmatch("v" + versions[0]):
        raise ValueError("Cargo [package] must contain exactly one x.y.z version")
    return versions[0]


def validate(tag: str, version: str, changelog: str) -> None:
    if not TAG.fullmatch(tag):
        raise ValueError("release tag must be vX.Y.Z (no prefix, suffix, or leading zeroes)")
    if tag != "v" + version:
        raise ValueError(f"tag {tag} does not match Cargo package version {version}")
    dates = re.findall(
        rf"^## \[{re.escape(version)}\] - (\d{{4}}-\d{{2}}-\d{{2}})$",
        changelog,
        re.MULTILINE,
    )
    if len(dates) != 1:
        raise ValueError(f"expected exactly one dated ## [{version}] - YYYY-MM-DD changelog section")
    try:
        datetime.date.fromisoformat(dates[0])
    except ValueError as exc:
        raise ValueError(f"invalid changelog date: {dates[0]}") from exc


def changelog_chapter(version: str, changelog: str) -> str:
    """Return the dated release chapter, including its heading, without later releases."""
    heading = re.search(
        rf"^## \[{re.escape(version)}\] - \d{{4}}-\d{{2}}-\d{{2}}$",
        changelog,
        re.MULTILINE,
    )
    if heading is None:
        raise ValueError(f"missing dated changelog chapter for {version}")
    next_heading = re.search(r"^## ", changelog[heading.end() :], re.MULTILINE)
    end = heading.end() + next_heading.start() if next_heading else len(changelog)
    return changelog[heading.start() : end].strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="validate release tag and dated changelog")
    parser.add_argument("--version", action="store_true", help="print package version only")
    parser.add_argument("--notes", action="store_true", help="print the validated changelog chapter")
    args = parser.parse_args()
    if args.version == bool(args.tag):
        parser.error("choose exactly one of --tag or --version")
    if args.notes and not args.tag:
        parser.error("--notes requires --tag")
    try:
        version = package_version((ROOT / "Cargo.toml").read_text())
        if args.tag:
            changelog = (ROOT / "CHANGELOG.md").read_text()
            validate(args.tag, version, changelog)
            print(changelog_chapter(version, changelog) if args.notes else version)
        else:
            print(version)
    except (ValueError, OSError) as exc:
        print(f"Release metadata error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
