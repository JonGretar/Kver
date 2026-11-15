"""Package a native release; inspect the PE again after extracting the actual ZIP."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-pc-windows-msvc"
# OS components for the experimental Windows 10/11 baseline, not whatever a
# development runner happens to have installed. Unknown imports fail closed.
SYSTEM_DLLS = set("""
advapi32.dll avrt.dll bcrypt.dll bcryptprimitives.dll cabinet.dll cfgmgr32.dll
comctl32.dll comdlg32.dll crypt32.dll cryptbase.dll cryptnet.dll cryptui.dll
d2d1.dll d3d11.dll d3d12.dll d3dcompiler_47.dll dcomp.dll dbghelp.dll
dnsapi.dll dwmapi.dll dwrite.dll dxgi.dll dxguid.dll gdi32.dll hid.dll
imm32.dll iphlpapi.dll kernel32.dll kernelbase.dll msimg32.dll
msvcrt.dll ncrypt.dll netapi32.dll normaliz.dll ntdll.dll ole32.dll oleaut32.dll
opengl32.dll powrprof.dll propsys.dll psapi.dll rasapi32.dll rpcrt4.dll
secur32.dll setupapi.dll shcore.dll shell32.dll shlwapi.dll user32.dll
userenv.dll usp10.dll ucrtbase.dll uxtheme.dll version.dll win32u.dll
windowscodecs.dll winhttp.dll wininet.dll winmm.dll winspool.drv
wintrust.dll wldap32.dll ws2_32.dll wtsapi32.dll
""".split())
CRT_DLL = re.compile(r"(?:vcruntime140(?:_1)?|msvcp140(?:_[12])?|concrt140)\.dll")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def resources(pe, kind):
    found = {}
    for entry in pe.DIRECTORY_ENTRY_RESOURCE.entries:
        if entry.id == kind:
            for name in entry.directory.entries:
                require(name.id not in found, f"Duplicate resource {kind}/{name.id}")
                require(len(name.directory.entries) == 1, "Unexpected resource languages")
                data = name.directory.entries[0].data.struct
                found[name.id] = pe.get_data(data.OffsetToData, data.Size)
    return found


def inspect_pe(executable, version):
    import pefile

    pe = pefile.PE(str(executable))
    try:
        require(pe.FILE_HEADER.Machine == 0x8664, "PE must be AMD64")
        require(pe.OPTIONAL_HEADER.Magic == 0x20B, "PE must be PE32+")
        strings = {}
        for group in pe.FileInfo:
            for entry in group:
                if entry.Key == b"StringFileInfo":
                    for table in entry.StringTable:
                        strings.update({
                            key.decode(): value.decode() for key, value in table.entries.items()
                        })
        for key, expected in {
            "CompanyName": "JonGretar",
            "ProductName": "Kver",
            "FileDescription": "Kver PDF reader",
            "OriginalFilename": "kver.exe",
            "InternalName": "kver",
            "FileVersion": version,
            "ProductVersion": version,
        }.items():
            require(strings.get(key) == expected, f"Incorrect executable identity: {key}")
        numeric = tuple(map(int, version.split("."))) + (0,)
        fixed = pe.VS_FIXEDFILEINFO[0]
        for prefix in ("File", "Product"):
            ms, ls = getattr(fixed, prefix + "VersionMS"), getattr(fixed, prefix + "VersionLS")
            require((ms >> 16, ms & 0xFFFF, ls >> 16, ls & 0xFFFF) == numeric,
                    f"Incorrect numeric {prefix}Version")

        # Check the real embedded images, not just the existence of an icon slot.
        ico = (ROOT / "assets/icons/kver.ico").read_bytes()
        reserved, kind, count = struct.unpack_from("<HHH", ico)
        require((reserved, kind) == (0, 1) and count > 0, "Invalid source ICO")
        groups = resources(pe, 14)
        require(set(groups) == {1}, "Missing or unexpected executable icon group")
        group = groups[1]
        require(struct.unpack_from("<HHH", group) == (0, 1, count), "ICO group mismatch")
        icons = resources(pe, 3)
        for index in range(count):
            source = struct.unpack_from("<BBBBHHII", ico, 6 + index * 16)
            embedded = struct.unpack_from("<BBBBHHIH", group, 6 + index * 14)
            require(source[:7] == embedded[:7], "ICO image metadata mismatch")
            size, offset = source[-2:]
            require(icons.get(embedded[-1]) == ico[offset:offset + size], "ICO image mismatch")

        manifests = resources(pe, 24)
        require(set(manifests) == {1}, "Missing framework application manifest")
        assembly = ET.fromstring(manifests[1].decode("utf-8-sig"))
        nodes = {node.tag.rsplit("}", 1)[-1]: node for node in assembly.iter()}
        require(nodes["requestedExecutionLevel"].attrib == {
            "level": "asInvoker", "uiAccess": "false",
        }, "Framework privilege behavior changed")
        for tag, text in (
            ("dpiAware", "true/pm"), ("dpiAwareness", "PerMonitorV2"),
            ("heapType", "SegmentHeap"),
        ):
            require(nodes[tag].text == text, f"Framework manifest changed: {tag}")
        require(nodes["supportedOS"].get("Id") == "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}",
                "Framework Windows compatibility declaration changed")
        require(any(node.get("name") == "Microsoft.Windows.Common-Controls"
                    and node.get("version") == "6.0.0.0" for node in assembly.iter()),
                "Framework common-controls dependency missing")

        imports = sorted({
            entry.dll.decode("ascii").lower()
            for directory in ("DIRECTORY_ENTRY_IMPORT", "DIRECTORY_ENTRY_DELAY_IMPORT")
            for entry in getattr(pe, directory, [])
        })
        require(imports, "No PE imports found")
        crt = []
        for dll in imports:
            if CRT_DLL.fullmatch(dll):
                crt.append(dll)
            else:
                require(dll in SYSTEM_DLLS or re.fullmatch(r"(?:api|ext)-ms-[a-z0-9-]+\.dll", dll),
                        f"Unreviewed DLL import: {dll}; determine deployment/license obligations")
        return {
            "architecture": "AMD64/PE32+",
            "identity": strings,
            "imports_including_delay_load": imports,
            "vc_redist_imports": crt,
            "deployment": "Windows OS DLLs; install Microsoft x64 v14 Redistributable if listed CRT imports require it. No DLL sidecars.",
            "dynamic_load_review": {
                "nvapi64.dll": "Optional GPUI NVIDIA driver-version query; failure becomes Unknown Driver.",
                "amd_ags_x64.dll": "Optional GPUI AMD driver-version query; failure becomes Unknown Driver.",
            },
            "native_gui_clean_pc": "UNVERIFIED by packaging; CI CLI execution is not GUI acceptance",
        }
    finally:
        pe.close()


def verify_archive(archive, version, expected_hashes):
    with tempfile.TemporaryDirectory(prefix="kver-windows-extracted-") as temp:
        clean = Path(temp)
        with zipfile.ZipFile(archive) as zipped:
            names = zipped.namelist()
            require(len(names) == len(set(names)), "Duplicate ZIP paths")
            require(all(not PurePosixPath(name).is_absolute() and ".." not in PurePosixPath(name).parts
                        and "\\" not in name and ":" not in name
                        for name in names + list(expected_hashes)),
                    "Unsafe ZIP paths")
            require(set(names) == set(expected_hashes) | {"SHA256SUMS"},
                    "Missing or unexpected ZIP content")
            zipped.extractall(clean)
        sums = "".join(f"{checksum}  {name}\n" for name, checksum in sorted(expected_hashes.items()))
        require((clean / "SHA256SUMS").read_text() == sums, "Incorrect checksum inventory")
        for name, checksum in expected_hashes.items():
            require(digest(clean / name) == checksum, f"Extracted checksum mismatch: {name}")
        report = inspect_pe(clean / "kver.exe", version)
        require(report == json.loads((clean / "PE-DEPENDENCIES.json").read_text()),
                "Extracted PE inspection differs from packaged report")
        # This calls the landed CLI contract. It neither opens a window nor
        # exercises named-pipe forwarding. Run away from both checkout and exe.
        cwd = clean / "unrelated-working-directory"
        cwd.mkdir()
        result = subprocess.run([str(clean / "kver.exe"), "--version"], cwd=cwd,
                                capture_output=True, text=True, timeout=30, check=True)
        require(result.stdout.strip() == f"kver {version}", "Extracted CLI version mismatch")


def package(release, licenses):
    require(os.name == "nt", "Packaging/CLI verification must run on native Windows")
    import tomllib

    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    require(re.fullmatch(r"\d+\.\d+\.\d+", version), "Resource version requires X.Y.Z")
    executable = release / "kver.exe"
    report = inspect_pe(executable, version)
    require(licenses.is_file() and "<pre>" in licenses.read_text(encoding="utf-8"),
            "Missing third-party license text")
    metadata = json.loads(subprocess.check_output([
        "cargo", "+stable", "metadata", "--locked", "--format-version", "1",
        "--filter-platform", TARGET,
    ], cwd=ROOT))
    assets = [p for p in metadata["packages"] if p["name"] == "gpui-kit-assets"]
    require(len(assets) == 1, "Cannot locate GPUI Kit artwork license")
    lucide = Path(assets[0]["manifest_path"]).parent / "LICENSE-LUCIDE"
    sysroot = Path(subprocess.check_output(["rustc", "+stable", "--print", "sysroot"], text=True).strip())
    rust_docs = sysroot / "share/doc/rust"
    require((rust_docs / "COPYRIGHT.html").is_file()
            and (rust_docs / "COPYRIGHT-library.html").is_file()
            and any((rust_docs / "licenses").glob("*")),
            "Missing Rust runtime license material; install rust-docs for stable")
    name = f"Kver-v{version}-experimental-windows-x86_64.zip"
    archive = release / name
    with tempfile.TemporaryDirectory(prefix="kver-windows-staging-") as temp:
        stage = Path(temp)
        for source, destination in (
            (executable, "kver.exe"), (ROOT / "LICENSE", "LICENSE"),
            (licenses, "THIRD-PARTY-LICENSES.html"), (lucide, "licenses/LICENSE-LUCIDE"),
            (ROOT / "docs/windows.md", "README.md"),
            (ROOT / "docs/cli.md", "CLI.md"),
            (rust_docs / "COPYRIGHT.html", "licenses/rust/COPYRIGHT.html"),
            (rust_docs / "COPYRIGHT-library.html", "licenses/rust/COPYRIGHT-library.html"),
        ):
            dest = stage / destination
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, dest)
        # Preserve accompanying attribution notices in addition to the license
        # texts collected by cargo-about (notably Apache NOTICE obligations).
        for dependency in metadata["packages"]:
            source_root = Path(dependency["manifest_path"]).parent
            if source_root == ROOT:
                continue
            for notice in source_root.rglob("*"):
                if notice.is_file() and notice.name.upper().startswith("NOTICE"):
                    dest = (stage / "licenses/notices"
                            / f"{dependency['name']}-{dependency['version']}"
                            / notice.relative_to(source_root))
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(notice, dest)
        shutil.copytree(rust_docs / "licenses", stage / "licenses/rust/licenses")
        (stage / "PE-DEPENDENCIES.json").write_text(json.dumps(report, indent=2) + "\n")
        hashes = {p.relative_to(stage).as_posix(): digest(p)
                  for p in stage.rglob("*") if p.is_file()}
        require(all((stage / name).stat().st_size for name in hashes), "Empty packaged requirement")
        (stage / "SHA256SUMS").write_text(
            "".join(f"{checksum}  {name}\n" for name, checksum in sorted(hashes.items())),
            encoding="utf-8",
        )
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
            for path in sorted(stage.rglob("*")):
                if path.is_file():
                    zipped.write(path, path.relative_to(stage).as_posix())
        verify_archive(archive, version, hashes)
    checksum = digest(archive)
    archive.with_suffix(".zip.sha256").write_text(f"{checksum}  {archive.name}\n", encoding="ascii")
    require(archive.with_suffix(".zip.sha256").read_text().split()[0] == digest(archive),
            "Archive checksum mismatch")
    print(json.dumps(report, indent=2))
    print(f"Verified native MSVC build/package and extracted CLI only: {archive}\nSHA256: {checksum}")
    print("Native clean-PC launch, GUI reading, security prompts and IPC: UNVERIFIED")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-dir", type=Path, required=True)
    parser.add_argument("--licenses", type=Path, required=True)
    args = parser.parse_args()
    package(args.release_dir.resolve(), args.licenses.resolve())
