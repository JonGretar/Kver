"""Prepare and run a local-only two-candidate Kver desktop update rehearsal."""

import argparse
from functools import partial
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
OLD, NEW = "0.0.1", "0.0.2"
HOST, PORT = "127.0.0.1", 38419
ORIGIN = f"http://{HOST}:{PORT}"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def substitute(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        raise ValueError(f"Fixture no longer matches {path.name}; review the candidate build")
    path.write_text(text.replace(old, new))


def configure_copy(src):
    # Don't add a feed override or an insecure transport to the shipped updater.
    # Only these private candidate sources will ever accept the loopback feed.
    rust = src / "src/app/linux_updates.rs"
    substitute(rust, "https://api.github.com/repos/JonGretar/Kver/releases/latest", ORIGIN + "/latest.json")
    substitute(rust, 'format!("{}/releases/tag/v{}", commands::PROJECT_URL, self.version)',
               f'format!("{ORIGIN}/notes.html#v{{}}", self.version)')
    helper = src / "packaging/linux/update.py"
    substitute(helper, "https://github.com/JonGretar/Kver/releases/download", ORIGIN)
    substitute(helper, 'src.geturl().startswith("https://")', 'src.geturl() == url')


def package(src, binary, version, directory):
    """Minimal local archive; not a distributable release or a license/packaging test."""
    directory.mkdir()
    shutil.copy2(binary, directory / "kver")
    shutil.copy2(src / "packaging/linux/install.py", directory / "install.py")
    (directory / "resources").mkdir()
    shutil.copy2(src / "assets/icons/png/256.png", directory / "resources/kver.png")
    (directory / "BUILD-INFO.json").write_text(json.dumps({
        "target": "x86_64-unknown-linux-gnu", "version": version, "rehearsal": True,
    }))
    sums = "".join(f"{digest(p)}  {p.relative_to(directory)}\n"
                   for p in sorted(directory.rglob("*")) if p.is_file())
    (directory / "SHA256SUMS").write_text(sums)


def sign_feed(directory, candidate, private):
    assets = directory / "feed" / f"v{NEW}"
    assets.mkdir(parents=True)
    archive = assets / f"{candidate.name}.tar.gz"
    with tarfile.open(archive, "w:gz", compresslevel=1) as tar:
        for path in sorted(candidate.rglob("*")):
            if path.is_file():
                tar.add(path, arcname=path.relative_to(candidate.parent), recursive=False)
    sums = assets / (archive.name + ".sha256")
    sums.write_text(f"{digest(archive)}  {archive.name}\n")
    subprocess.run(["openssl", "dgst", "-sha256", "-sign", "/dev/stdin", "-out",
                    str(sums) + ".sig", sums], input=private, check=True)
    (assets.parent / "latest.json").write_text(json.dumps({
        "tag_name": f"v{NEW}", "draft": False, "prerelease": False,
        "assets": [{"name": p.name} for p in sorted(assets.iterdir())],
    }))
    (assets.parent / "notes.html").write_text("<title>Kver rehearsal</title>Local test release. Not for distribution.")


def build(directory):
    if (platform.system(), platform.machine()) != ("Linux", "x86_64"):
        raise ValueError("Build on x86-64 GNU/Linux (prefer the Ubuntu 24.04 desktop baseline)")
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    # Generate in memory, never export a key into the worktree, artifacts or logs.
    private = subprocess.check_output(["openssl", "genpkey", "-algorithm", "RSA",
                                       "-pkeyopt", "rsa_keygen_bits:3072"], stderr=subprocess.DEVNULL)
    public = subprocess.check_output(["openssl", "pkey", "-pubout"], input=private).decode()
    with tempfile.TemporaryDirectory(prefix="kver-candidates-") as tmp:
        src = Path(tmp) / "src"
        src.mkdir()
        for name in ("Cargo.toml", "Cargo.lock", "build.rs"):
            shutil.copy2(ROOT / name, src / name)
        for name in ("src", "assets", "examples", "tests", "packaging/linux"):
            shutil.copytree(ROOT / name, src / name)
        configure_copy(src)
        manifest, lock = src / "Cargo.toml", src / "Cargo.lock"
        current = re.search(r'^version = "([^"]+)"', manifest.read_text(), re.M)[1]
        target = Path(os.environ.get("CARGO_TARGET_DIR", Path(tmp) / "target")).resolve()
        for version, key, output in [(OLD, "", "source"), (OLD, public, "older"),
                                     (NEW, public, f"Kver-v{NEW}-linux-x86_64")]:
            if current != version:
                substitute(manifest, f'name = "kver"\nversion = "{current}"',
                           f'name = "kver"\nversion = "{version}"')
                substitute(lock, f'name = "kver"\nversion = "{current}"',
                           f'name = "kver"\nversion = "{version}"')
                current = version
            subprocess.run(["cargo", "+stable", "build", "--locked", "--bin", "kver"], cwd=src,
                           env=dict(os.environ, KVER_OFFICIAL_LINUX_UPDATE_KEY=key,
                                    CARGO_TARGET_DIR=str(target), CARGO_PROFILE_DEV_DEBUG="0"), check=True)
            binary = target / "debug/kver"
            if subprocess.check_output([binary, "--version"], text=True).strip() != f"kver {version}":
                raise ValueError("Candidate version mismatch")
            if output == "source":
                shutil.copy2(binary, directory / "source-kver")
            else:
                package(src, binary, version, directory / output)
        sign_feed(directory, directory / output, private)
    shutil.copy2(ROOT / "examples/selection-sample.pdf", directory / "sample.pdf")
    print(f"Prepared Linux {OLD} → {NEW} rehearsal in {directory}")


def launch(directory, source):
    # Use real desktop session/portal sockets but not the user's app data,
    # instance lock, installed binary, launcher, annotations or history.
    home = directory / ("source-home" if source else "managed-home")
    home.mkdir(mode=0o700, exist_ok=True)
    env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"),
               XDG_DATA_HOME=str(home / ".local/share"), XDG_CACHE_HOME=str(home / ".cache"),
               XDG_STATE_HOME=str(home / ".local/state"))
    if "XAUTHORITY" not in env and (Path.home() / ".Xauthority").is_file():
        env["XAUTHORITY"] = str(Path.home() / ".Xauthority")
    exe = directory / "source-kver" if source else home / ".local/opt/kver/kver"
    if not source and not exe.exists():
        subprocess.run([sys.executable, directory / "older/install.py", "install"], env=env, check=True)
    pdf = home / "Rehearsal.pdf"
    if not pdf.exists():
        shutil.copy2(directory / "sample.pdf", pdf)
    os.execve(exe, [str(exe), str(pdf)], env)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["build", "serve", "managed", "source"])
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    directory = args.directory.resolve()
    if args.action == "build":
        build(directory)
    elif args.action == "serve":
        handler = partial(http.server.SimpleHTTPRequestHandler, directory=str(directory / "feed"))
        with http.server.ThreadingHTTPServer((HOST, PORT), handler) as server:
            print(f"Serving only the rehearsal feed at {ORIGIN}", flush=True)
            server.serve_forever()
    else:
        launch(directory, args.action == "source")
