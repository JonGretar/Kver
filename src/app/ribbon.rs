//! App-wide notice presented in the active reader, never a focus-taking overlay.
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{App, Global, Window, div, prelude::*};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Choice {
    Activate,
    Dismiss,
}

pub(super) type Response = Rc<dyn Fn(Choice, &mut App)>;

pub(crate) struct Notice {
    pub message: String,
    pub action: Option<String>,
    pub link: Option<(String, String)>,
    pub respond: Response,
}

#[derive(Default)]
struct Ribbon(Option<Rc<Notice>>);
impl Global for Ribbon {}

pub(crate) fn present(notice: Option<Notice>, cx: &mut App) {
    cx.set_global(Ribbon(notice.map(Rc::new)));
    cx.refresh_windows();
}

fn respond(notice: &Rc<Notice>, choice: Choice, cx: &mut App) {
    if cx
        .try_global::<Ribbon>()
        .and_then(|r| r.0.as_ref())
        .is_some_and(|current| Rc::ptr_eq(current, notice))
    {
        present(None, cx);
        (notice.respond)(choice, cx);
    }
}

pub(super) fn render(window: &Window, cx: &App) -> Option<impl IntoElement + use<>> {
    if !window.is_window_active() {
        return None;
    }
    let notice = cx.try_global::<Ribbon>()?.0.clone()?;
    let dismiss = notice.clone();
    let link = notice.link.clone();
    Some(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(notice.message.clone()),
            )
            .when_some(notice.action.clone(), |row, label| {
                row.child(
                    Button::new("notice-action")
                        .small()
                        .outline()
                        .label(label)
                        .on_click(move |_, _, cx| respond(&notice, Choice::Activate, cx)),
                )
            })
            .when_some(link, |row, (label, url)| {
                row.child(
                    gpui_kit::base::Link::new("notice-link")
                        .href(url)
                        .open_with(|url, _, _, cx| cx.open_url(url))
                        .text_sm()
                        .underline()
                        .text_color(cx.theme().link)
                        .child(label),
                )
            })
            .child(
                Button::new("dismiss-notice")
                    .small()
                    .ghost()
                    .label("×")
                    .accessibility_label("Dismiss notice")
                    .tooltip("Dismiss notice")
                    .on_click(move |_, _, cx| respond(&dismiss, Choice::Dismiss, cx)),
            ),
    )
}
