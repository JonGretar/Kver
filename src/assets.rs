//! Keep Kit's component icons and embed only Kver's additional reader icons.

use std::borrow::Cow;

use gpui_kit::{
    AssetSource, Result, SharedString,
    assets::{Assets, icon_assets},
};

icon_assets!(ReaderIcons, [MoveHorizontal, Expand]);

/// The asset source registered by both the app and reader integration tests.
pub struct ReaderAssets;

impl AssetSource for ReaderAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ReaderIcons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
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
            "icons/arrow-down.svg",
        ] {
            assert!(ReaderAssets.load(path).unwrap().is_some(), "{path}");
            assert!(ReaderAssets.list("icons/").unwrap().contains(&path.into()));
        }
    }
}
