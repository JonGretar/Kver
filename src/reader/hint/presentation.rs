use super::{Input, Label, Metrics};
use crate::coordinates::Point;
use gpui_kit::{
    App, Font, FontWeight, TextRun, Window, base::TestSupportExt as _, component::ActiveTheme as _,
    div, prelude::*, px, relative,
};

const FONT_SCALE: f32 = 0.75;

/// Measure the label renderer, including its padding and line box, at UI scale.
pub fn measure_labels(window: &Window, cx: &App) -> Metrics {
    let font_size = window.rem_size() * FONT_SCALE;
    let font = Font {
        family: cx.theme().mono_font_family.clone(),
        weight: FontWeight::BOLD,
        ..Default::default()
    };
    let width = (b'a'..=b'z')
        .map(|c| {
            window
                .text_system()
                .shape_line(
                    (c as char).to_string().into(),
                    font_size,
                    &[TextRun {
                        len: 1,
                        font: font.clone(),
                        color: cx.theme().foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width
        })
        .fold(px(0.), |a, b| a.max(b));
    let rem = f32::from(window.rem_size()) as f64;
    Metrics::new(f32::from(width) as f64 * 2. + rem * 0.5, rem, rem * 0.025)
}

impl<T> Label<T> {
    pub(crate) fn render(&self, origin: Point, input: &Input, cx: &App) -> gpui_kit::AnyElement {
        let [l, t, r, b] = self.bounds().edges();
        let (background, foreground, border) = crate::app::theme::word_hints();
        // Absolute pixel values are resolved PDF/viewport and shaped-text
        // geometry. Typography and the measured padding scale with rem.
        div()
            .id(format!("hint-{}", self.letters()))
            .test_support()
            .aria_label(self.letters().to_owned())
            .absolute()
            .left(px((l - origin.x) as f32))
            .top(px((t - origin.y) as f32))
            .w(px((r - l) as f32))
            .h(px((b - t) as f32))
            .overflow_hidden()
            .bg(background)
            .text_color(foreground)
            .border_1()
            .border_color(border)
            .rounded_sm()
            .text_size(gpui_kit::rems(FONT_SCALE))
            .font_family(cx.theme().mono_font_family.clone())
            .font_weight(FontWeight::BOLD)
            .line_height(relative(1.))
            .flex()
            .items_center()
            .justify_center()
            .children(self.letters().chars().enumerate().map(|(ix, c)| {
                div()
                    .when(ix < input.prefix().len(), |letter| letter.underline())
                    .child(c.to_string())
            }))
            .into_any_element()
    }
}
