"""Check out an existing release with the current archive's notice policy."""

import argparse
import os
from pathlib import Path
import subprocess
import sys


def run(*command):
    subprocess.run(command, check=True)


def prepare(platform, tag):
    ref = f"refs/tags/{tag}"
    policy = subprocess.check_output(["git", "show", f"HEAD:packaging/{platform}/about.toml"])
    run("git", "check-ref-format", ref)
    run("gh", "release", "view", "--repo", os.environ["GITHUB_REPOSITORY"], "--", tag)
    run("git", "fetch", "--no-tags", "origin", ref)
    run("git", "checkout", "--detach", "FETCH_HEAD")

    # Existing tags still carry their own packaging scripts under scripts/.
    if Path("packaging/common/release-metadata.py").is_file():
        tools = "packaging"
        metadata = "packaging/common/release-metadata.py"
        tests = "packaging/tests"
        entry = {"linux": "packaging/linux/package.sh", "windows": "packaging/windows/package.ps1"}
    else:
        tools = "scripts"
        metadata = "scripts/release-metadata.py"
        tests = "scripts"
        entry = {"linux": "scripts/package-linux.sh", "windows": "scripts/package-windows.ps1"}
    run(sys.executable, metadata, "--tag", tag)
    Path(f"{tools}/{platform}/about.toml").write_bytes(policy)
    with Path(os.environ["GITHUB_ENV"]).open("a", encoding="utf-8") as output:
        print(f"PACKAGE_TOOLS={tools}", file=output)
        print(f"PACKAGE_METADATA={metadata}", file=output)
        print(f"PACKAGE_TESTS={tests}", file=output)
        print(f"PACKAGE_ENTRY={entry[platform]}", file=output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=("linux", "windows"))
    parser.add_argument("tag")
    args = parser.parse_args()
    prepare(args.platform, args.tag)
