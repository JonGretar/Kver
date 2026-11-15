"""Install/remove this extracted Kver archive for the current user (Python 3.9+)."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checksum(data):
    return hashlib.sha256(data).hexdigest()


def relative(name):
    path = PurePosixPath(name)
    require(name and not path.is_absolute() and ".." not in path.parts
            and str(path) == name and "\\" not in name,
            f"Unsafe package path: {name!r}")
    return path


def safe(path):
    """Do not follow destination symlinks, including symlinked parent directories."""
    for part in (path, *path.parents):
        require(not part.is_symlink(), f"Refusing symlink: {part}")
    require(not path.exists() or path.is_file(), f"Expected a regular file: {path}")


def atomic_write(path, data, mode=0o644):
    safe(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".kver-", delete=False) as temp:
        temporary = Path(temp.name)
        try:
            temp.write(data)
            temp.flush()
            os.fchmod(temp.fileno(), mode)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


def desktop_string(value):
    return value.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")


def exec_path(path):
    # Desktop Entry Exec is not a shell command. Escape its quoting first,
    # then the entry's string layer; percent signs are field-code escapes.
    value = str(path).replace("%", "%%")
    for char in ("\\", '"', "`", "$"):
        value = value.replace(char, "\\" + char)
    return desktop_string('"' + value + '"')


def layout():
    home = Path(os.environ.get("HOME", ""))
    require(home.is_absolute(), "HOME must be an absolute path")
    data = Path(os.environ.get("XDG_DATA_HOME") or home / ".local/share")
    require(data.is_absolute(), "XDG_DATA_HOME must be an absolute path")
    root = home / ".local/opt/kver"
    return root, {
        "launcher": data / "applications/kver.desktop",
        "icon": data / "icons/hicolor/256x256/apps/kver.png",
    }


def payload(source):
    sums = source / "SHA256SUMS"
    require(sums.is_file(), f"Missing package inventory: {sums}")
    files = {}
    for line in sums.read_text().splitlines():
        digest, name = line.split("  ", 1)
        relative(name)
        require(name not in files and name not in (".install.json", "SHA256SUMS"),
                f"Duplicate/reserved package path: {name}")
        require(re.fullmatch("[0-9a-f]{64}", digest), f"Invalid checksum: {name}")
        path = source / name
        safe(path)
        content = path.read_bytes()
        require(checksum(content) == digest, f"Package checksum mismatch: {name}")
        files[name] = content
    require({"kver", "resources/kver.png", "install.py", "BUILD-INFO.json"} <= files.keys(),
            "Incomplete extracted package")
    require(os.access(source / "kver", os.X_OK), "Package executable is not executable")
    files["SHA256SUMS"] = sums.read_bytes()
    return files


def run(action):
    root, external = layout()
    manifest = root / ".install.json"
    safe(manifest)
    previous = {}
    if manifest.exists():
        state = json.loads(manifest.read_text())
        require(isinstance(state, dict), f"Invalid ownership manifest: {manifest}")
        require(state["format"] == 1 and state["data_home"] == str(external["launcher"].parent.parent),
                "Installation location changed; use the original XDG_DATA_HOME to remove it")
        previous = state["files"]
        require(isinstance(previous, dict), f"Invalid ownership file inventory: {manifest}")
    elif root.exists() or any(p.exists() or p.is_symlink() for p in external.values()):
        require(not root.exists() or not any(root.iterdir()),
                f"Missing ownership manifest: {manifest}; refusing to adopt existing contents")
        require(not any(p.exists() or p.is_symlink() for p in external.values()),
                "Unowned launcher/icon exists; move it aside before installing")

    def destination(key):
        if key in external:
            return external[key]
        require(key.startswith("package/"), f"Invalid ownership key: {key}")
        name = key[len("package/"):]
        relative(name)
        require(name != ".install.json", "Reserved ownership path")
        return root / name

    for key, hashes in previous.items():
        path = destination(key)
        safe(path)
        require(isinstance(hashes, list) and hashes
                and all(isinstance(h, str) and re.fullmatch("[0-9a-f]{64}", h) for h in hashes),
                f"Invalid ownership hashes: {key}")
        require(not path.exists() or checksum(path.read_bytes()) in hashes,
                f"Installed file changed; move it aside and retry: {path}")

    contents = {}
    if action == "install":
        contents = {"package/" + name: data for name, data in payload(Path(__file__).resolve().parent).items()}
        contents["icon"] = contents["package/resources/kver.png"]
        # GLib probes argv[0] before expanding %% in Exec. env keeps that
        # probe independent of the user's path, then execs Kver without a shell.
        contents["launcher"] = (
            "[Desktop Entry]\nType=Application\nName=Kver\n"
            "Comment=Read PDF documents\n"
            f"Exec=/usr/bin/env -- {exec_path(root / 'kver')} -- %f\n"
            "Icon=kver\nTerminal=false\nCategories=Office;Viewer;\n"
            "MimeType=application/pdf;\nStartupWMClass=kver\n"
        ).encode()

    for key in contents:
        path = destination(key)
        safe(path)
        require(key in previous or not path.exists(), f"Refusing to overwrite unowned file: {path}")

    if not previous and not contents:
        print("Kver is not installed; nothing to remove.")
        return

    # Record both versions before touching payloads, so interrupted replacement
    # can be retried or removed even when only some files reached the new version.
    pending = {key: list(hashes) for key, hashes in previous.items()}
    for key, data in contents.items():
        pending[key] = sorted(set(pending.get(key, []) + [checksum(data)]))

    def record(files):
        atomic_write(manifest, (json.dumps({
            "format": 1, "data_home": str(external["launcher"].parent.parent),
            "files": files,
        }, indent=2) + "\n").encode())

    record(pending)
    for key, data in contents.items():
        atomic_write(destination(key), data, 0o755 if key == "package/kver" else 0o644)
    for key in previous.keys() - contents.keys():
        destination(key).unlink(missing_ok=True)
    if contents:
        record({key: [checksum(data)] for key, data in contents.items()})
        print(f"Installed Kver at {root}; launcher: {external['launcher']}")
    else:
        manifest.unlink()
        print("Removed Kver installation; documents, settings and history were not touched.")

    # Only prune empty payload directories, never recursively remove a tree.
    directories = {destination(key).parent for key in pending if key.startswith("package/")}
    directories |= {parent for path in directories.copy() for parent in path.parents
                    if parent == root or root in parent.parents}
    for path in sorted(directories | {root}, key=lambda p: len(p.parts), reverse=True):
        try:
            path.rmdir()
        except OSError:
            pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("install", "uninstall"))
    args = parser.parse_args()
    try:
        run(args.action)
    except (OSError, ValueError, KeyError, TypeError) as error:
        sys.exit(f"Kver {args.action} failed: {error}")
