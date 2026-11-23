# Arch Linux package

Unofficial `PKGBUILD`. Builds the tagged release from the GitHub source
tarball with the system Rust (`cargo`), not the pinned `rust-toolchain.toml`.

```sh
cd packaging/arch
makepkg -sf
sudo pacman -U kver-*-x86_64.pkg.tar.zst
```

Installs `/usr/bin/kver`, `kver.desktop`, hicolor icons, docs and the licence.
Does not set the default PDF reader.

- `options=('!lto')` is required. makepkg's default `-flto=auto` breaks linking
  of `ring` under `ld.lld`.
- Runtime `depends` follow `docs/linux.md`. Wayland, Vulkan and EGL load
  dynamically, so `ldd` does not show them.
- Only checked under Xvfb with software rendering. Native Wayland/X11 and GPU
  use are unverified (see [#183](https://github.com/JonGretar/Kver/issues/183)).

## Bump to a new release

1. Set `pkgver`, reset `pkgrel=1`.
2. `updpkgsums`
3. `makepkg --printsrcinfo > .SRCINFO`
4. Build and run `kver --version`.
