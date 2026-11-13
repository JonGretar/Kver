# Kver app icons

Copy `assets/icons` into your Rust/GPUI repository.

Contents:
- `kver-master.png`: 1254px RGBA raster artwork with transparent outer corners.
- `kver.icns`: macOS icon with transparent margins (tile occupies about 80% of canvas).
- `kver.iconset`: macOS source PNGs, including Retina sizes. On macOS regenerate with `iconutil -c icns assets/icons/kver.iconset -o assets/icons/kver.icns`.
- `kver.ico`: Windows multi-resolution icon, 16/24/32/48/64/128/256px.
- `png`: Linux/general PNGs, 16 through 1024px; tile occupies about 94% of canvas.
- `preview.png` in the package root: small-size comparison on a light background.

## macOS

Run `sh scripts/build-macos-app.sh` on a Mac to build the release binary and
package it with `kver.icns`. See the [README](../../README.md#run-from-source)
for local launch and [release guide](../../docs/releasing.md) for archive checks. A plain `cargo run`
executable is not an app bundle.

## Windows

Add to Cargo.toml:

```toml
[build-dependencies]
winresource = "0.1"
```

Merge into build.rs (preserve any existing build steps):

```rust
fn main() {
    println!("cargo:rerun-if-changed=assets/icons/kver.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icons/kver.ico")
            .compile()
            .expect("could not compile Kver Windows icon resource");
    }
}
```

Requires the Windows resource compiler supplied by the Windows SDK or the appropriate MinGW toolchain. Embeds the executable icon; window/taskbar integration can additionally depend on your GPUI version and application identity configuration.

## Linux

Install each PNG under `~/.local/share/icons/hicolor/SIZExSIZE/apps/kver.png`, for example `256x256/apps/kver.png`.
Install a launcher under `~/.local/share/applications/APP_ID.desktop`:

```ini
[Desktop Entry]
Type=Application
Name=Kver
Comment=PDF reader
Exec=kver %F
Icon=kver
Terminal=false
Categories=Office;Viewer;
MimeType=application/pdf;
```

Replace APP_ID with the identifier used by your app and set Exec to your actual executable if it is not on PATH. Match the Wayland application ID to the desktop file basename; X11 window grouping may also require matching StartupWMClass. Verify against your pinned GPUI version.

## Validation and scope

ICO and ICNS containers were decoded and their available resolutions checked. PNG dimensions and transparent corners were checked. Small sizes use a light sharpening pass; they retain the original design rather than a separately redrawn glyph. Actual native builds and desktop integration have not been tested in your project.

Artwork is raster, not SVG. No document-association icon is included. The transparent artwork was prepared from the approved generated design using image generation; minor pixel differences from the original are possible.
