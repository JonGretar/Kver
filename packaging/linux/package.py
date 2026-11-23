"""Stage and verify the actual experimental Linux tarball; never publish it."""

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-unknown-linux-gnu"
# Reviewed against gpui-pre-linux's Wayland dlopen backend and wgpu/ash's
# Vulkan/EGL loaders. These are invisible to ldd; loading is not GPU acceptance.
DYNAMIC_LIBRARIES = ("libwayland-client.so.0", "libvulkan.so.1", "libEGL.so.1")
REQUIRED = {
    "kver", "LICENSE", "THIRD-PARTY-LICENSES.html", "README.md", "CLI.md",
    "ELF-DEPENDENCIES.json", "BUILD-INFO.json", "licenses/LICENSE-LUCIDE",
    "licenses/rust/COPYRIGHT.html", "licenses/rust/COPYRIGHT-library.html",
    "resources/kver.png", "install.py",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def output(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True, stderr=subprocess.STDOUT,
                                   timeout=60)


def inspect_elf(executable):
    with executable.open("rb") as source:
        header = source.read(20)
    require(header[:7] == b"\x7fELF\x02\x01\x01"
            and int.from_bytes(header[18:20], "little") == 62,
            "Executable must be little-endian ELF64 x86-64")
    dynamic = output("readelf", "-dW", str(executable))
    require("(RPATH)" not in dynamic and "(RUNPATH)" not in dynamic,
            "Unreviewed ELF runtime search path")
    needed = sorted(re.findall(r"\(NEEDED\).*?\[(.*?)\]", dynamic))
    require(needed, "Missing ELF dependencies")
    linkage = output("ldd", str(executable))
    require("not found" not in linkage, f"Missing linked runtime dependency:\n{linkage}")
    # ASLR changes addresses on each invocation, including clean extraction.
    linkage = re.sub(r"0x[0-9a-fA-F]+", "<address>", linkage)
    program = output("readelf", "-lW", str(executable))
    require("/lib64/ld-linux-x86-64.so.2" in program, "Unexpected ELF interpreter")
    versions = output("readelf", "--version-info", "-W", str(executable))
    symbols = sorted(set(re.findall(r"\b(?:GLIBC|GLIBCXX|CXXABI)_[0-9.]+", versions)))
    loaded = []
    for library in DYNAMIC_LIBRARIES:
        ctypes.CDLL(library)
        loaded.append(library)
    return {
        "architecture": "ELF64 x86-64", "needed": needed,
        "symbol_versions": symbols, "ldd": linkage,
        "dynamic_load_probes": loaded,
        "dynamic_review": {
            "wayland": "gpui-pre-linux uses wayland-backend client_system + dlopen.",
            "vulkan": "wgpu/ash loads libvulkan.so.1; a working Vulkan ICD/driver is also required.",
            "egl": "wgpu GLES fallback loads libEGL.so.1; an appropriate GL/EGL driver is also required.",
            "xkb_xcb": "Retained xkbcommon/X11 and XCB linkage is recorded by readelf/ldd.",
            "fonts": "GPUI WGPU uses cosmic-text/fontdb for system fonts and fontconfig configuration. "
                     "Build-time font-kit dependencies need not survive linking; consult actual ELF imports.",
            "services": "Session D-Bus and xdg-desktop-portal with a desktop backend are needed "
                        "for rfd/ashpd file dialogs; services are not ELF libraries.",
            "unresolved": "Driver/ICD transitive loads, compositor/portal compatibility, "
                          "fonts and native GPU usability are not established by loader probes.",
        },
        "native_gui": "UNVERIFIED: X11, Wayland, portals, GPU and non-development desktop",
    }


def verify_archive(archive, version, expected_hashes):
    require(REQUIRED <= expected_hashes.keys(), "Missing required inventory")
    require(any(n.startswith("licenses/rust/licenses/") for n in expected_hashes),
            "Missing Rust license texts")
    prefix = f"Kver-v{version}-experimental-linux-x86_64"
    with tempfile.TemporaryDirectory(prefix="kver-linux-extracted-") as temp:
        clean = Path(temp)
        with tarfile.open(archive, "r:gz") as tar:
            members = tar.getmembers()
            names = [m.name for m in members]
            require(len(names) == len(set(names)), "Duplicate tar paths")
            require(all(m.isfile() and not PurePosixPath(m.name).is_absolute()
                        and ".." not in PurePosixPath(m.name).parts
                        and "\\" not in m.name and ":" not in m.name
                        for m in members), "Unsafe tar member")
            require(set(names) == {f"{prefix}/{n}" for n in expected_hashes}
                    | {f"{prefix}/SHA256SUMS"}, "Missing or unexpected tar content")
            for member in members:
                expected_mode = 0o755 if member.name == f"{prefix}/kver" else 0o644
                require(member.mode == expected_mode, f"Incorrect permissions: {member.name}")
                dest = clean / member.name
                dest.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as source, dest.open("wb") as target:
                    shutil.copyfileobj(source, target)
                dest.chmod(member.mode)
        stage = clean / prefix
        sums = "".join(f"{checksum}  {name}\n" for name, checksum in sorted(expected_hashes.items()))
        require((stage / "SHA256SUMS").read_text() == sums, "Incorrect checksum inventory")
        for name, checksum in expected_hashes.items():
            require(digest(stage / name) == checksum, f"Extracted checksum mismatch: {name}")
        require(os.access(stage / "kver", os.X_OK), "Extracted executable is not executable")
        report = inspect_elf(stage / "kver")
        require(report == json.loads((stage / "ELF-DEPENDENCIES.json").read_text()),
                "Extracted ELF differs from packaged report")
        unrelated = clean / "unrelated-working-directory"
        unrelated.mkdir()
        require(output(str(stage / "kver"), "--version", cwd=unrelated).strip() == f"kver {version}",
                "Extracted CLI identity/version mismatch")
        # Run the production installer against this exact extracted archive.
        print(output(sys.executable, str(ROOT / "packaging/tests/test_linux_install.py"), str(stage),
                     "--version", version), end="")


def package(release, licenses):
    require(platform.system() == "Linux" and platform.machine() == "x86_64",
            "Native x86-64 Linux required")
    version = output(sys.executable, str(ROOT / "packaging/common/release-metadata.py"), "--version").strip()
    executable = release / "kver"
    report = inspect_elf(executable)
    require(licenses.is_file() and "<pre>" in licenses.read_text(),
            "Missing dependency license text")
    sysroot = Path(output("rustc", "+stable", "--print", "sysroot").strip())
    rust_docs = sysroot / "share/doc/rust"
    require(any((rust_docs / "licenses").glob("*")), "Install stable rust-docs for runtime licenses")
    baseline = dict(line.split("=", 1) for line in Path("/etc/os-release").read_text().splitlines()
                    if "=" in line)
    require(baseline.get("ID", "").strip('"') == "ubuntu"
            and baseline.get("VERSION_ID", "").strip('"') == "24.04", "Ubuntu 24.04 baseline required")
    build = {
        "version": version, "target": TARGET, "experimental": True,
        "os_release": baseline, "rustc": output("rustc", "+stable", "-vV"),
        "glibc": output("getconf", "GNU_LIBC_VERSION").strip(),
        "source_commit": output("git", "rev-parse", "HEAD", cwd=ROOT).strip(),
        "resource_policy": "Reader/Kit icons, app artwork and keybinding presets are embedded. "
                           "resources/kver.png is optional external integration artwork, not a runtime lookup.",
    }
    prefix = f"Kver-v{version}-experimental-linux-x86_64"
    archive = release / f"{prefix}.tar.gz"
    with tempfile.TemporaryDirectory(prefix="kver-linux-staging-") as temp:
        stage = Path(temp)
        for source, destination in (
            (executable, "kver"), (ROOT / "LICENSE", "LICENSE"),
            (licenses, "THIRD-PARTY-LICENSES.html"),
            (ROOT / "docs/linux.md", "README.md"), (ROOT / "docs/cli.md", "CLI.md"),
            (ROOT / "assets/icons/png/256.png", "resources/kver.png"),
            (ROOT / "packaging/linux/install.py", "install.py"),
            (rust_docs / "COPYRIGHT.html", "licenses/rust/COPYRIGHT.html"),
            (rust_docs / "COPYRIGHT-library.html", "licenses/rust/COPYRIGHT-library.html"),
        ):
            dest = stage / destination
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, dest)
        shutil.copytree(rust_docs / "licenses", stage / "licenses/rust/licenses")
        output(sys.executable, str(ROOT / "packaging/common/collect-notices.py"),
               str(ROOT), TARGET, str(stage / "licenses"))
        for name, data in (("ELF-DEPENDENCIES.json", report), ("BUILD-INFO.json", build)):
            (stage / name).write_text(json.dumps(data, indent=2) + "\n")
        files = sorted(p for p in stage.rglob("*") if p.is_file())
        require(all(p.stat().st_size for p in files), "Empty required package content")
        hashes = {p.relative_to(stage).as_posix(): digest(p) for p in files}
        (stage / "SHA256SUMS").write_text(
            "".join(f"{checksum}  {name}\n" for name, checksum in sorted(hashes.items())))
        with tarfile.open(archive, "w:gz") as tar:
            for path in files + [stage / "SHA256SUMS"]:
                name = path.relative_to(stage).as_posix()
                info = tar.gettarinfo(path, f"{prefix}/{name}")
                info.mode = 0o755 if name == "kver" else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                with path.open("rb") as source:
                    tar.addfile(info, source)
        checksum = digest(archive)
        sidecar = archive.with_suffix(".gz.sha256")
        sidecar.write_text(f"{checksum}  {archive.name}\n")
        require(sidecar.read_text() == f"{digest(archive)}  {archive.name}\n",
                "Archive checksum mismatch")
        verify_archive(archive, version, hashes)
    print(json.dumps({"build": build, "dependencies": report}, indent=2))
    print(f"Verified release archive, extraction and CLI only: {archive}\nSHA256: {checksum}")
    print("Native desktop/X11/Wayland/portal/GPU acceptance: UNVERIFIED")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-dir", type=Path, required=True)
    parser.add_argument("--licenses", type=Path, required=True)
    args = parser.parse_args()
    package(args.release_dir.resolve(), args.licenses.resolve())
