"""Copy dependency attribution files that are not in cargo-about's report."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess


def collect_notices(source, target, destination, offline=False):
    command = ["cargo", "metadata", "--locked", "--format-version", "1",
               "--filter-platform", target]
    if offline:
        command.append("--offline")
    packages = json.loads(subprocess.check_output(
        command, cwd=source, env={**os.environ, "RUSTUP_TOOLCHAIN": "stable"},
    ))["packages"]
    assets = [p for p in packages if p["name"] == "gpui-kit-assets"]
    if len(assets) != 1:
        raise ValueError("Expected exactly one gpui-kit-assets crate")
    destination.mkdir(parents=True, exist_ok=True)
    lucide = Path(assets[0]["manifest_path"]).parent / "LICENSE-LUCIDE"
    if not lucide.is_file() or not lucide.stat().st_size:
        raise ValueError("Missing Lucide artwork license")
    shutil.copyfile(lucide, destination / lucide.name)

    for dependency in packages:
        root = Path(dependency["manifest_path"]).parent
        if root == source:
            continue
        for notice in root.rglob("*"):
            if notice.is_file() and notice.name.upper().startswith("NOTICE"):
                path = (destination / "notices" / f"{dependency['name']}-{dependency['version']}"
                        / notice.relative_to(root))
                path.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(notice, path)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("target")
    parser.add_argument("destination", type=Path)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    collect_notices(args.source.resolve(), args.target, args.destination, args.offline)
