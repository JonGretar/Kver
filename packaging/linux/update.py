"""Private helper embedded in update-enabled binaries. Python 3.9+, OpenSSL.

Never run code from a download until its version-bound checksum is authenticated.
Only exchange complete, inventory-owned package directories on the same filesystem.
"""
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def private_install_path(path):
    """An ownership record is not evidence if another user can replace it."""
    for part in (path, *path.parents):
        info = part.lstat()
        shared_temp = part.is_dir() and info.st_uid == 0 and info.st_mode & 0o1000
        require(not part.is_symlink() and info.st_uid in (0, os.getuid())
                and (not info.st_mode & 0o022 or shared_temp), "Untrusted installation path")


def owned(executable, running_hash=None):
    root = Path.home() / ".local/opt/kver"
    require(executable == root / "kver" and root.resolve() == root and
            root.stat().st_uid == os.getuid() and os.access(root.parent, os.W_OK),
            "Not a Kver-managed user installation")
    record = root / ".install.json"
    private_install_path(record)
    require(record.resolve() == record and record.stat().st_uid == os.getuid(), "Unowned installation")
    state = json.loads(record.read_text())
    files = state["files"]
    data = Path(os.getenv("XDG_DATA_HOME") or Path.home() / ".local/share")
    require(state["format"] == 1 and state["data_home"] == str(data), "Unrecognized installation")
    external = {"launcher": data / "applications/kver.desktop",
                "icon": data / "icons/hicolor/256x256/apps/kver.png"}
    require({"package/kver", "package/BUILD-INFO.json", *external} <= files.keys(),
            "Incomplete installer record")
    actual = set()
    directories = {p.parent for p in (root / k[len("package/"):] for k in files if k.startswith("package/"))}
    directories |= {p for d in tuple(directories) for p in d.parents}
    for p in root.rglob("*"):
        require(not p.is_symlink(), "Installation contains symlinks")
        if p.is_dir():
            require(p in directories, "Installation contains unowned directories")
            require(p.stat().st_uid == os.getuid() and not p.stat().st_mode & 0o022,
                    "Untrusted package directory")
        elif p != record:
            actual.add("package/" + p.relative_to(root).as_posix())
    require(actual | external.keys() == files.keys(), "Installation contains missing or unowned files")
    for key, hashes in files.items():
        p = external[key] if key in external else root / key[len("package/"):]
        require(p.resolve() == p and p.is_file() and p.stat().st_uid == os.getuid()
                and not p.stat().st_mode & 0o022
                and [digest(p)] == hashes, "Installation changed: " + key)
    if running_hash:
        require(files["package/kver"] == [running_hash], "The running copy is no longer installed")
    return root, state


def fetch(url, path, limit):
    with urllib.request.urlopen(url, timeout=30) as src, path.open("xb") as dst:
        require(src.geturl().startswith("https://"), "Insecure download redirect")
        for block in iter(lambda: src.read(1 << 20), b""):
            limit -= len(block)
            require(limit >= 0, "Download exceeds size limit")
            dst.write(block)


def verify(stage, version, public_key):
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version), "Invalid release version")
    name = f"Kver-v{version}-linux-x86_64.tar.gz"
    archive = stage / name
    # The key comes from the running executable, never the feed or staging area.
    with tempfile.NamedTemporaryFile(dir=stage) as key:
        key.write(public_key.encode())
        key.flush()
        verified = subprocess.run(["/usr/bin/openssl", "dgst", "-sha256", "-verify", key.name, "-signature",
                                   str(stage / (name + ".sha256.sig")), str(stage / (name + ".sha256"))],
                                  capture_output=True)
        require(verified.returncode == 0, "Could not verify the Linux update signature")
    require((stage / (name + ".sha256")).read_bytes() ==
            f"{digest(archive)}  {name}\n".encode(), "Update checksum or version mismatch")
    return archive


def prepare(stage, version, public_key):
    name = f"Kver-v{version}-linux-x86_64.tar.gz"
    base = f"https://github.com/JonGretar/Kver/releases/download/v{version}/{name}"
    for suffix, limit in [(".sha256.sig", 8192), (".sha256", 1024), ("", 512 << 20)]:
        fetch(base + suffix, stage / (name + suffix), limit)
    verify(stage, version, public_key)


def replace(executable, stage, version, public_key, running_hash):
    archive = verify(stage, version, public_key)
    fd = os.open(executable.parent.parent, os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        install_archive(executable, archive, version, running_hash)
    finally:
        os.close(fd)


def install_archive(executable, archive, version, running_hash):
    root, state = owned(executable, running_hash)
    with tempfile.TemporaryDirectory(dir=root.parent, prefix=".kver-update-") as temp:
        new = Path(temp) / "kver"
        new.mkdir(mode=root.stat().st_mode & 0o777)
        inventory = {}
        remaining = 2 << 30
        prefix = archive.name[:-len(".tar.gz")] + "/"
        with tarfile.open(archive, "r|gz") as tar:
            for member in tar:
                name = member.name[len(prefix):]
                require(member.name.startswith(prefix) and name and name != ".install.json"
                        and all(p not in ("", ".", "..") for p in name.split("/"))
                        and member.isfile(), "Unsafe update archive")
                remaining -= member.size
                require(remaining >= 0 and len(inventory) < 30000, "Update archive too large")
                dest = new / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                with dest.open("xb") as f, tar.extractfile(member) as source:
                    shutil.copyfileobj(source, f)
                dest.chmod(0o755 if name == "kver" else 0o644)
                inventory["package/" + name] = [digest(dest)]
        info = json.loads((new / "BUILD-INFO.json").read_text())
        require(info["version"] == version and info["target"] == "x86_64-unknown-linux-gnu"
                and {"package/kver", "package/install.py", "package/SHA256SUMS"} <= inventory.keys(),
                "Incompatible Linux archive")
        # Desktop integration is owned by install.py. If its layout/artwork
        # changes, require the full installer rather than half-applying it.
        require(inventory["package/resources/kver.png"] == state["files"]["icon"]
                and inventory["package/install.py"] == state["files"]["package/install.py"],
                "This release requires running the full installer")
        check = subprocess.check_output([new / "kver", "--version"], timeout=20, text=True)
        require(check.strip() == "kver " + version, "Updated executable cannot run on this system")
        inventory.update({k: state["files"][k] for k in ("icon", "launcher")})
        (new / ".install.json").write_text(json.dumps(dict(state, files=inventory)))
        # RENAME_EXCHANGE: old or new, never a partially overwritten installation.
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.renameat2(-100, os.fsencode(new), -100, os.fsencode(root), 2):
            raise OSError(ctypes.get_errno(), "Cannot activate the verified update")


def main(action, exe, key, *args):
    executable = Path(exe)
    if action == "eligible":
        require(os.path.samestat(executable.stat(), os.fstat(sys.stdin.fileno())),
                "Not the installed executable")
        _, state = owned(executable)
        require(os.access("/usr/bin/openssl", os.X_OK) and hasattr(os, "pidfd_open"), "Update tools unavailable")
        print(state["files"]["package/kver"][0])
        return
    version, directory = args[:2]
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version), "Invalid release version")
    stage = Path(directory)
    if action == "stage":
        owned(executable, args[2])
        prepare(stage, version, key)
        return
    require(action == "restart", "Unknown action")
    require(stage.resolve() == stage and stage.name.startswith("kver-update-") and stage.is_dir()
            and stage.stat().st_uid == os.getuid() and stage.stat().st_mode & 0o077 == 0,
            "Expected a private updater staging directory")
    pid, running_hash, instance_lock = args[2:]
    # Only launched after the normal save/discard/cancel flow approved quit.
    # pidfd identifies this particular process even if its PID gets recycled.
    try:
        try:
            fd = os.pidfd_open(int(pid))
        except ProcessLookupError:
            fd = None
        if fd is not None:
            with os.fdopen(fd, "rb", buffering=0) as parent:
                select.select([parent], [], [])
        # A new reader may have won instance election since the old one quit.
        # Never switch the package under it. Keep the lock until exec (CLOEXEC).
        election = os.open(instance_lock, os.O_RDWR | os.O_NOFOLLOW)
        fcntl.flock(election, fcntl.LOCK_EX | fcntl.LOCK_NB)
        replace(executable, stage, version, key, running_hash)
        os.environ.pop("KVER_UPDATE_FAILURE", None)
    except Exception as error:
        require(executable.resolve() == executable and digest(executable) == running_hash,
                "Installed executable changed; refusing to relaunch")
        os.environ["KVER_UPDATE_FAILURE"] = str(error)
    finally:
        shutil.rmtree(stage, ignore_errors=True)
    os.execl(executable, executable)


if __name__ == "__main__":
    try:
        main(*sys.argv[1:])
    except Exception as error:
        sys.exit(str(error))
