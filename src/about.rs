//! A single, document-independent About window, owned by the application menu.

use gpui_kit as gpui;
use gpui_kit::base::{Link, StyledExt as _};
use gpui_kit::component::{ActiveTheme as _, TitleBar, button::Button};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClipboardItem, Context, FocusHandle,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, Styled as _,
    Window, WindowBounds, WindowOptions, div, img, px, size,
};

use crate::reader_windows::CloseWindow;

const PROJECT_URL: &str = "https://github.com/JonGretar/Kver";
const VERSION: &str = env!("CARGO_PKG_VERSION");

gpui::actions!(kver, [CloseAbout]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseAbout, Some("AboutKver"))]);
}

pub(crate) fn open(cx: &mut App) -> gpui_kit::Result<AnyWindowHandle> {
    // Initial native window bounds are in platform pixels; content uses rem helpers.
    let bounds = Bounds::centered(None, size(px(440.0), px(400.0)), cx);
    let (handle, _) = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            is_minimizable: false,
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle().tab_stop(true);
                focus.focus(window, cx);
                AboutView { focus }
            })
        },
    )?;
    cx.update_window(handle, |_, window, _| window.activate_window())?;
    Ok(handle)
}

struct AboutView {
    focus: FocusHandle,
}

impl Render for AboutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .tab_group()
            .key_context("AboutKver")
            .on_action(|_: &CloseAbout, window, _| window.remove_window())
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .child(TitleBar::new())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(img("images/kver.png").size_16())
                    .child(div().text_lg().font_semibold().child("Kver"))
                    .child(
                        Link::new("about-project")
                            .href(PROJECT_URL)
                            .open_with(|url, _, _, cx| cx.open_url(url))
                            .text_sm()
                            .text_color(cx.theme().link)
                            .underline()
                            .cursor_pointer()
                            .hover(|this| this.text_color(cx.theme().link.opacity(0.8)))
                            .focus_visible(|this| this.border_1().border_color(cx.theme().ring))
                            .child("github.com/JonGretar/Kver"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("Version {VERSION}")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_4()
                    .child(
                        Button::new("about-ok")
                            .label("OK")
                            .flex_1()
                            .track_focus(&self.focus)
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(Button::new("about-copy").label("Copy").flex_1().on_click(
                        |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                "Kver\nVersion {VERSION}\n{PROJECT_URL}"
                            )));
                        },
                    )),
            )
    }
}
