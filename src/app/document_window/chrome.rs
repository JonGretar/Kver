//! Window chrome is value-like presentation of the active document's identity.

use gpui_kit::{App, SharedString, div, prelude::*, px};
use gpui_kit::{
    assets::IconName,
    base::TestSupportExt as _,
    component::{
        ActiveTheme, Sizable, TitleBar,
        button::{Button, ButtonVariants},
        tooltip::Tooltip,
    },
};
use std::path::Path;

use super::OpenPdf;

pub(super) fn title_bar(path: Option<&Path>, cx: &App) -> impl IntoElement + use<> {
    let filename = path.map(document_name).map(SharedString::from);
    let title = filename
        .clone()
        .unwrap_or_else(|| "A quiet place to read".into());
    TitleBar::new()
        .pr_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                // TitleBar's inner bar does not shrink; give the filename a
                // zero basis so Open PDF stays visible.
                .w_0()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_color(cx.theme().primary)
                        .font_weight(gpui_kit::FontWeight::BOLD)
                        .child("K /"),
                )
                .child(
                    div()
                        .id("document-name")
                        .test_support()
                        .aria_label(title.clone())
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_sm()
                        .when_some(filename.clone(), |title, name| {
                            title.tooltip(move |window, cx| {
                                Tooltip::new(name.clone()).build(window, cx)
                            })
                        })
                        .child(title),
                ),
        )
        .child(
            Button::new("open-pdf")
                .label("Open PDF…")
                .icon(IconName::FolderOpen)
                .ghost()
                .small()
                .tooltip_with_action("Choose a PDF", &OpenPdf, None)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPdf), cx)),
        )
}

fn document_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_uses_filename_instead_of_parent_directories() {
        assert_eq!(
            document_name(Path::new("/long/parent/directory/report.pdf")),
            "report.pdf"
        );
        assert_eq!(
            document_name(Path::new("/long/parent/directory/very-long-file-name.pdf")),
            "very-long-file-name.pdf"
        );
    }
}
