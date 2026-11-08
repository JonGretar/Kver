//! Platform-aware presentation for effective key sequences in help and palettes.

use gpui_kit::{App, Keystroke, component::kbd::Kbd};

pub(crate) fn format(keys: &str, cx: &App) -> String {
    keys.split_whitespace()
        .map(|part| {
            if part.len() == 1 && part.bytes().all(|byte| byte.is_ascii_lowercase()) {
                return part.to_owned();
            }
            Keystroke::parse_with_os(part, cx.platform_info().os)
                .map(|stroke| Kbd::format(&stroke))
                .unwrap_or_else(|_| part.to_owned())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
