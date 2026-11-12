//! Catppuccin colors for application chrome; PDF raster colors are untouched.
use std::rc::Rc;

use gpui_kit::{
    App,
    component::theme::{Theme, ThemeConfig, ThemeConfigColors, ThemeMode},
};

pub(super) fn install(cx: &mut App) {
    let mode = Theme::global(cx).mode;
    Theme::update(cx, |theme| {
        theme.light_theme = Rc::new(config(ThemeMode::Light));
        theme.dark_theme = Rc::new(config(ThemeMode::Dark));
    });
    // Register both configurations, not just the current colors, so settings
    // changes and system appearance updates keep using the product palette.
    Theme::change(mode, None, cx);
}

/// Selection on PDF pages has a paper-colored surface in either chrome theme.
/// Keep this document-surface role distinct from primary chrome, which may be
/// white in a dark/custom theme and disappear against the PDF.
pub(crate) fn document_identification() -> gpui_kit::Hsla {
    gpui_kit::rgb(0x1e66f5).into()
}

// Official palette roles: https://catppuccin.com/palette/
struct Palette {
    base: &'static str,
    mantle: &'static str,
    crust: &'static str,
    surface0: &'static str,
    surface1: &'static str,
    surface2: &'static str,
    overlay0: &'static str,
    text: &'static str,
    subtext0: &'static str,
    mauve: &'static str,
    blue: &'static str,
    green: &'static str,
    yellow: &'static str,
    red: &'static str,
    teal: &'static str,
    selection: &'static str,
    overlay: &'static str,
}

fn config(mode: ThemeMode) -> ThemeConfig {
    let (name, p) = if mode.is_dark() {
        (
            "Catppuccin Mocha",
            Palette {
                base: "#1e1e2e",
                mantle: "#181825",
                crust: "#11111b",
                surface0: "#313244",
                surface1: "#45475a",
                surface2: "#585b70",
                overlay0: "#6c7086",
                text: "#cdd6f4",
                subtext0: "#a6adc8",
                mauve: "#cba6f7",
                blue: "#89b4fa",
                green: "#a6e3a1",
                yellow: "#f9e2af",
                red: "#f38ba8",
                teal: "#94e2d5",
                selection: "#cba6f733",
                overlay: "#11111b99",
            },
        )
    } else {
        (
            "Catppuccin Latte",
            Palette {
                base: "#eff1f5",
                mantle: "#e6e9ef",
                crust: "#dce0e8",
                surface0: "#ccd0da",
                surface1: "#bcc0cc",
                surface2: "#acb0be",
                overlay0: "#9ca0b0",
                text: "#4c4f69",
                subtext0: "#6c6f85",
                mauve: "#8839ef",
                blue: "#1e66f5",
                green: "#40a02b",
                yellow: "#df8e1d",
                red: "#d20f39",
                teal: "#179299",
                selection: "#8839ef26",
                overlay: "#4c4f694d",
            },
        )
    };
    let color = |value: &'static str| Some(value.into());
    let mut colors = ThemeConfigColors::default();
    colors.background = color(p.base);
    colors.foreground = color(p.text);
    colors.border = color(p.surface0);
    colors.input = color(p.surface2);
    colors.muted = color(p.mantle);
    colors.muted_foreground = color(p.subtext0);
    colors.accent = color(p.surface0);
    colors.accent_foreground = color(p.text);
    colors.primary = color(p.mauve);
    colors.primary_foreground = color(if mode.is_dark() { p.crust } else { p.base });
    colors.secondary = color(p.surface0);
    colors.secondary_foreground = color(p.text);
    colors.secondary_hover = color(p.surface1);
    colors.secondary_active = color(p.surface2);
    colors.button = color(p.base);
    colors.button_foreground = color(p.text);
    colors.button_hover = color(p.surface0);
    colors.button_active = color(p.surface1);
    colors.caret = color(p.mauve);
    colors.ring = color(p.mauve);
    colors.selection = color(p.selection);
    colors.link = color(p.blue);
    colors.link_hover = color(p.mauve);
    colors.link_active = color(p.mauve);
    colors.popover = color(p.base);
    colors.popover_foreground = color(p.text);
    colors.group_box = color(p.mantle);
    colors.group_box_foreground = color(p.text);
    colors.accordion = color(p.mantle);
    colors.sidebar = color(p.mantle);
    colors.sidebar_foreground = color(p.text);
    colors.sidebar_border = color(p.surface0);
    colors.sidebar_accent = color(p.surface0);
    colors.sidebar_accent_foreground = color(p.text);
    colors.sidebar_primary = color(p.mauve);
    colors.sidebar_primary_foreground = colors.primary_foreground.clone();
    colors.title_bar = color(p.mantle);
    colors.title_bar_border = color(p.surface0);
    colors.status_bar = color(p.mantle);
    colors.status_bar_border = color(p.surface0);
    colors.list = color(p.base);
    colors.list_hover = color(p.surface0);
    colors.list_active = color(p.selection);
    colors.list_active_border = color(p.mauve);
    colors.list_head = color(p.mantle);
    colors.list_even = color(p.mantle);
    colors.tab = color(p.mantle);
    colors.tab_foreground = color(p.subtext0);
    colors.tab_active = color(p.base);
    colors.tab_active_foreground = color(p.mauve);
    colors.tab_bar = color(p.mantle);
    colors.tab_bar_segmented = color(p.crust);
    colors.table = color(p.base);
    colors.table_hover = color(p.surface0);
    colors.table_active = color(p.selection);
    colors.table_active_border = color(p.mauve);
    colors.table_even = color(p.mantle);
    colors.table_head = color(p.mantle);
    colors.table_head_foreground = color(p.text);
    colors.table_foot = color(p.mantle);
    colors.table_foot_foreground = color(p.text);
    colors.table_row_border = color(p.surface0);
    colors.description_list_label = color(p.mantle);
    colors.description_list_label_foreground = color(p.subtext0);
    colors.scrollbar = color(p.mantle);
    colors.scrollbar_thumb = color(p.surface2);
    colors.scrollbar_thumb_hover = color(p.overlay0);
    colors.switch = color(p.surface2);
    colors.switch_thumb = color(p.base);
    colors.skeleton = color(p.surface0);
    colors.progress_bar = color(p.mauve);
    colors.slider_bar = color(p.mauve);
    colors.slider_thumb = color(p.base);
    colors.drag_border = color(p.mauve);
    colors.drop_target = color(p.selection);
    colors.overlay = color(p.overlay);
    colors.window_border = color(p.surface0);
    colors.danger = color(p.red);
    colors.warning = color(p.yellow);
    colors.success = color(p.green);
    colors.info = color(p.blue);
    colors.chart_1 = color(p.mauve);
    colors.chart_2 = color(p.blue);
    colors.chart_3 = color(p.teal);
    colors.chart_4 = color(p.yellow);
    colors.chart_5 = color(p.red);
    // Kit derives button/status hover and pressed states from these roles.
    ThemeConfig {
        name: name.into(),
        mode,
        colors,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{TestAppContext, rgb};

    #[gpui_kit::test]
    fn palettes_survive_mode_changes_and_keep_tokens_in_sync(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx);
            for (mode, background, primary) in [
                (ThemeMode::Dark, 0x1e1e2e, 0xcba6f7),
                (ThemeMode::Light, 0xeff1f5, 0x8839ef),
                (ThemeMode::Dark, 0x1e1e2e, 0xcba6f7),
            ] {
                Theme::change(mode, None, cx);
                let theme = Theme::global(cx);
                assert_eq!(theme.background, rgb(background).into());
                assert_eq!(theme.primary, rgb(primary).into());
                assert_eq!(theme.tokens.sidebar.color, theme.sidebar);
                assert_eq!(theme.tokens.primary.color, theme.primary);
                assert_ne!(theme.button_hover, theme.button);
                assert_ne!(theme.list_active, theme.list_hover);
                assert_ne!(theme.primary_foreground, theme.primary);
            }
            Theme::sync_system_appearance(None, cx);
            let theme = Theme::global(cx);
            assert!(theme.theme_name().starts_with("Catppuccin "));
            assert_eq!(theme.tokens.background.color, theme.background);
        });
    }
}
