#!/usr/bin/env python3
"""Validate release metadata before publishing; print the package version for packaging."""

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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="validate release tag and dated changelog")
    parser.add_argument("--version", action="store_true", help="print package version only")
    args = parser.parse_args()
    if args.version == bool(args.tag):
        parser.error("choose exactly one of --tag or --version")
    try:
        version = package_version((ROOT / "Cargo.toml").read_text())
        if args.tag:
            validate(args.tag, version, (ROOT / "CHANGELOG.md").read_text())
        print(version)
    except (ValueError, OSError) as exc:
        print(f"Release metadata error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
