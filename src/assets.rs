//! Keep Kit's component icons and embed Kver's reader icons and app artwork.

use std::borrow::Cow;

use gpui_kit::{
    AssetSource, Result, SharedString,
    assets::{Assets, icon_assets},
};

icon_assets!(
    ReaderIcons,
    [MoveHorizontal, Expand, ZoomIn, ZoomOut, Sun, Moon, SunMoon]
);

/// The asset source registered by both the app and reader integration tests.
pub struct ReaderAssets;

impl AssetSource for ReaderAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "images/kver.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/png/128.png"
            ))));
        }
        if let Some(bytes) = ReaderIcons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        if "images/kver.png".starts_with(path) {
            paths.push("images/kver.png".into());
        }
        paths.extend(ReaderIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_reader_and_component_icons() {
        for path in [
            "icons/move-horizontal.svg",
            "icons/expand.svg",
            "icons/zoom-in.svg",
            "icons/zoom-out.svg",
            "icons/sun.svg",
            "icons/moon.svg",
            "icons/sun-moon.svg",
            "icons/arrow-down.svg",
        ] {
            assert!(ReaderAssets.load(path).unwrap().is_some(), "{path}");
            assert!(ReaderAssets.list("icons/").unwrap().contains(&path.into()));
        }
    }

    #[test]
    fn serves_embedded_about_icon() {
        let bytes = ReaderAssets.load("images/kver.png").unwrap().unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(
            ReaderAssets
                .list("images/")
                .unwrap()
                .contains(&"images/kver.png".into())
        );
    }
}
