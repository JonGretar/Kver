"""Exercise the packaged installer on an actual extracted Linux archive.

Usage: python3 scripts/test_linux_install.py EXTRACTED_PACKAGE [--version X.Y.Z]
All user directories and mutations are contained in a workspace-local fixture.
"""

import argparse
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile


def verify(source, version=None, custom_data=True):
    workspace = Path(__file__).resolve().parents[1] / "target"
    workspace.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="linux install ", dir=workspace) as temp:
        base = Path(temp)
        package = base / "extracted package"
        shutil.copytree(source, package)
        home = base / 'home space % dollar$ tick` quote" slash\\'
        home.mkdir()
        data = home / "custom data" if custom_data else home / ".local/share"
        env = dict(os.environ, HOME=str(home), XDG_DATA_HOME=str(data),
                   XDG_CONFIG_HOME=str(home / "config"), XDG_STATE_HOME=str(home / "state"))
        if not custom_data:
            del env["XDG_DATA_HOME"]
        root = home / ".local/opt/kver"
        desktop = data / "applications/kver.desktop"
        icon = data / "icons/hicolor/256x256/apps/kver.png"
        sentinels = [home / "Documents/book.pdf", home / "config/kver/config.toml",
                     data / "kver/reading.sqlite3",
                     home / "state/kver/history.sqlite",
                     data / "applications/mimeapps.list",
                     data / "applications/other.desktop"]
        for path in sentinels:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"preserve me")

        def call(action, error=None):
            result = subprocess.run(
                [sys.executable, str(package / "install.py"), action], env=env,
                cwd=base, text=True, capture_output=True, timeout=30)
            if error:
                assert result.returncode != 0, result.stdout
                assert error in result.stderr, result.stderr
            else:
                assert result.returncode == 0, result.stderr
            for path in sentinels:
                assert path.read_bytes() == b"preserve me", path

        call("uninstall")
        call("install")
        assert (root / "kver").read_bytes() == (package / "kver").read_bytes()
        assert os.access(root / "kver", os.X_OK)
        assert icon.read_bytes() == (package / "resources/kver.png").read_bytes()
        text = desktop.read_text()
        assert "Exec=" in text and " -- %f\n" in text
        assert "%F" not in text and "%U" not in text and "DBusActivatable" not in text
        assert "Icon=kver\n" in text and "StartupWMClass=kver\n" in text
        if shutil.which("desktop-file-validate"):
            subprocess.run(["desktop-file-validate", str(desktop)], check=True)

        # GLib consumes the production desktop entry. A temporary argv recorder
        # checks field expansion without starting Kver's GUI, IPC or a shell.
        if platform.system() == "Linux":
            if version:
                actual = subprocess.check_output([str(root / "kver"), "--version"],
                                                 env=env, cwd=base, text=True).strip()
                assert actual == "kver " + version, actual
            executable = root / "kver"
            original = executable.read_bytes()
            result_path = base / "launcher argv.json"
            probe_env = dict(env, KVER_LAUNCH_TEST_OUTPUT=str(result_path),
                             DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(base / "no-session-bus"))
            for variable in ("DISPLAY", "WAYLAND_DISPLAY"):
                probe_env.pop(variable, None)
            try:
                executable.write_text(
                    "#!/usr/bin/python3\nimport json, os, sys\n"
                    "result = os.environ['KVER_LAUNCH_TEST_OUTPUT']\n"
                    "with open(result + '.tmp', 'w') as out:\n"
                    "    json.dump(sys.argv, out)\n"
                    "os.replace(result + '.tmp', result)\n")
                subprocess.run(["/usr/bin/python3", "-c", """
import json
import sys
import time
from pathlib import Path
from gi.repository import Gio, GLib
entry = Gio.DesktopAppInfo.new_from_filename(sys.argv[1])
assert entry is not None
ok, argv = GLib.shell_parse_argv(entry.get_commandline())
assert ok and argv == ["/usr/bin/env", "--", sys.argv[2].replace("%", "%%"), "--", "%f"], argv
assert entry.launch([Gio.File.new_for_path(sys.argv[3])], None)
result = Path(sys.argv[4])
deadline = time.monotonic() + 5
while not result.exists() and time.monotonic() < deadline:
    time.sleep(0.01)
assert json.loads(result.read_text()) == [sys.argv[2], "--", sys.argv[3]]
""", str(desktop), str(executable), str(sentinels[0]), str(result_path)],
                               env=probe_env, check=True, timeout=15)
            finally:
                executable.write_bytes(original)

        call("install")
        # Replacement changes resources, removes obsolete owned content, and
        # preserves unrelated content even inside the installation directory.
        unowned = root / "obsolete.txt"
        unowned.write_bytes(b"unrelated")
        (package / "obsolete.txt").write_bytes(b"new package file")
        sums = (package / "SHA256SUMS").read_bytes()
        with (package / "SHA256SUMS").open("a") as inventory_file:
            inventory_file.write(hashlib.sha256(b"new package file").hexdigest() + "  obsolete.txt\n")
        call("install", "Refusing to overwrite unowned file")
        assert unowned.read_bytes() == b"unrelated"
        unowned.unlink()
        (package / "obsolete.txt").unlink()
        (package / "SHA256SUMS").write_bytes(sums)
        unrelated = root / "my-notes.txt"
        unrelated.write_bytes(b"preserve me")
        sentinels.append(unrelated)
        extra = package / "obsolete.txt"
        extra.write_bytes(b"old package")

        def inventory():
            files = sorted(p for p in package.rglob("*") if p.is_file() and p.name != "SHA256SUMS")
            (package / "SHA256SUMS").write_text("".join(
                f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(package).as_posix()}\n"
                for p in files))

        inventory()
        call("install")
        assert (root / "obsolete.txt").exists()
        extra.unlink()
        (package / "resources/kver.png").write_bytes(b"replacement icon")
        inventory()
        if os.geteuid() != 0:
            # Fail after the pending manifest and earlier payload writes, then
            # repair the permissions and retry the real partial replacement.
            (root / "resources").chmod(0o500)
            try:
                call("install", "Permission denied")
            finally:
                (root / "resources").chmod(0o755)
        call("install")
        assert not (root / "obsolete.txt").exists()
        assert icon.read_bytes() == b"replacement icon"
        # Missing owned files are safe partial states, for repair and removal.
        desktop.unlink()
        (root / "kver").unlink()
        call("install")
        assert desktop.exists() and (root / "kver").exists()
        icon.unlink()
        call("uninstall")
        assert not desktop.exists() and not (root / ".install.json").exists()
        assert unrelated.exists()
        call("uninstall", "Missing ownership manifest")
        sentinels.remove(unrelated)
        unrelated.unlink()
        call("uninstall")

        # Neither an unowned collision nor a changed owned file is disposable.
        desktop.parent.mkdir(parents=True, exist_ok=True)
        desktop.write_text("another installation")
        call("install", "Unowned launcher/icon")
        assert desktop.read_text() == "another installation"
        desktop.unlink()
        call("install")
        icon.write_bytes(b"user modified")
        call("install", "Installed file changed")
        call("uninstall", "Installed file changed")
        assert icon.read_bytes() == b"user modified"
        icon.unlink()
        # Directory and symlink substitutions cannot redirect removal/writes.
        (root / "resources/kver.png").unlink()
        (root / "resources/kver.png").symlink_to(sentinels[0])
        call("uninstall", "Refusing symlink")
        (root / "resources/kver.png").unlink()
        call("uninstall")
        root.mkdir(parents=True, exist_ok=True)
        root.rmdir()
        root.symlink_to(home / "Documents", target_is_directory=True)
        call("install", "Refusing symlink")
        root.unlink()
        # Invalid inventories are rejected before any destination payload write.
        (package / "SHA256SUMS").write_text("0" * 64 + "  ../escape\n")
        call("install", "Unsafe package path")
        assert not desktop.exists()

    print("Verified extracted-package install/replacement/partial-state/remove and sentinel preservation.")
    if platform.system() != "Linux":
        print("Linux Desktop Entry parser and installed ELF execution: NOT RUN on this host.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("--version")
    args = parser.parse_args()
    for custom_data in (True, False):
        verify(args.package.resolve(), args.version, custom_data)
