//! Window chrome is value-like presentation of the active document's identity.

use gpui_kit::{App, SharedString, Window, div, prelude::*, px};
use gpui_kit::{
    assets::IconName,
    base::{Disableable, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable, Sizable, TitleBar,
        button::{Button, ButtonVariants},
        status_bar::StatusBar,
        toolbar::{Toolbar, ToolbarGroup},
        tooltip::Tooltip,
    },
};
use std::path::Path;

use super::OpenPdf;
use super::sidebar::ToggleSidebar;
use crate::{
    reader::{
        Continuous, FitPage, FitWidth, NextPage, PreviousPage, ReaderView, ReadingMode, SinglePage,
        ZoomIn, ZoomMode, ZoomOut,
    },
    settings_ui::ShowSettings,
};

pub(super) fn reading_toolbar(reader: &ReaderView, sidebar_open: bool, window: &Window) -> Toolbar {
    let has_document = reader.page_count().is_some();
    let show_fit_labels =
        f32::from(window.bounds().size.width) >= 35.0 * f32::from(window.rem_size());
    Toolbar::new("reading-toolbar")
        .px_3()
        .content(reader_button(
            "toggle-sidebar",
            if sidebar_open {
                "Hide sidebar"
            } else {
                "Show sidebar"
            },
            Some(IconName::PanelLeft),
            Box::new(ToggleSidebar),
            has_document,
            sidebar_open,
        ))
        .content(gpui_kit::component::separator::Separator::vertical().h_5())
        .content(
            ToolbarGroup::new("page-controls")
                .label("Page navigation")
                .child(reader_button(
                    "previous-page",
                    "Previous page",
                    Some(IconName::ArrowUp),
                    Box::new(PreviousPage),
                    has_document && reader.page_index() > 0,
                    false,
                ))
                .child(reader_button(
                    "next-page",
                    "Next page",
                    Some(IconName::ArrowDown),
                    Box::new(NextPage),
                    reader.page_index() + 1 < reader.page_count().unwrap_or(0),
                    false,
                )),
        )
        .content(gpui_kit::component::separator::Separator::vertical().h_5())
        .content(
            reader_button(
                "reading-mode",
                match (reader.reading_mode(), reader.mode_override()) {
                    (ReadingMode::SinglePage, Some(_)) => {
                        "Single page for this PDF · switch to continuous"
                    }
                    (ReadingMode::SinglePage, None) => {
                        "Single page (default) · switch to continuous"
                    }
                    (ReadingMode::Continuous, Some(_)) => {
                        "Continuous for this PDF · switch to single page"
                    }
                    (ReadingMode::Continuous, None) => {
                        "Continuous (default) · switch to single page"
                    }
                },
                Some(IconName::FileText),
                if reader.reading_mode() == ReadingMode::SinglePage {
                    Box::new(Continuous)
                } else {
                    Box::new(SinglePage)
                },
                has_document,
                reader.reading_mode() == ReadingMode::SinglePage,
            )
            .small()
            .when(show_fit_labels, |button| {
                button.label(if reader.reading_mode() == ReadingMode::SinglePage {
                    "Single page"
                } else {
                    "Continuous"
                })
            }),
        )
        .content(gpui_kit::component::separator::Separator::vertical().h_5())
        .content(
            ToolbarGroup::new("zoom-controls")
                .label("Zoom and fit")
                .child(reader_button(
                    "zoom-out",
                    "Zoom out",
                    Some(IconName::ZoomOut),
                    Box::new(ZoomOut),
                    has_document,
                    false,
                ))
                .child(reader_button(
                    "zoom-in",
                    "Zoom in",
                    Some(IconName::ZoomIn),
                    Box::new(ZoomIn),
                    has_document,
                    false,
                ))
                .child(
                    reader_button(
                        "fit-width",
                        "Fit width",
                        Some(IconName::MoveHorizontal),
                        Box::new(FitWidth),
                        has_document,
                        has_document && reader.zoom_mode() == ZoomMode::FitWidth,
                    )
                    .when(show_fit_labels, |button| button.label("Fit width")),
                )
                .child(
                    reader_button(
                        "fit-page",
                        "Fit page",
                        Some(IconName::Expand),
                        Box::new(FitPage),
                        has_document,
                        has_document && reader.zoom_mode() == ZoomMode::FitPage,
                    )
                    .when(show_fit_labels, |button| button.label("Fit page")),
                ),
        )
        .content(div().flex_1())
        .content(
            reader_button(
                "settings-button",
                "Settings…",
                Some(IconName::Settings),
                Box::new(ShowSettings),
                true,
                false,
            )
            .small(),
        )
}

pub(super) fn status_bar(reader: &ReaderView) -> StatusBar {
    let has_document = reader.page_count().is_some();
    StatusBar::new()
        .left(
            div()
                .id("reader-status")
                .test_support()
                .aria_label(if let Some(count) = reader.page_count() {
                    format!("Page {} of {}", reader.page_index() + 1, count)
                } else {
                    reader.status().to_owned()
                })
                .min_w(px(0.0))
                .truncate()
                .child(if let Some(count) = reader.page_count() {
                    format!("PAGE {} / {}", reader.page_index() + 1, count)
                } else {
                    reader.status().to_owned()
                }),
        )
        .when(
            has_document && !reader.status().starts_with("Page "),
            |bar| {
                bar.child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .child(reader.status().to_owned()),
                )
            },
        )
        .when(has_document, |bar| {
            bar.right(div().child(format!(
                "{}%{}",
                (reader.scale() * 100.0).round(),
                match reader.zoom_mode() {
                    ZoomMode::Fixed => "",
                    ZoomMode::FitWidth => " · Fit width",
                    ZoomMode::FitPage => " · Fit page",
                }
            )))
        })
}

fn reader_button(
    id: &'static str,
    label: &'static str,
    icon: Option<IconName>,
    action: Box<dyn gpui_kit::Action>,
    enabled: bool,
    selected: bool,
) -> Button {
    Button::new(id)
        .accessibility_label(label)
        .when_some(icon, |button, icon| button.icon(icon))
        .when(icon.is_none(), |button| button.label(label))
        .ghost()
        .selected(selected)
        .disabled(!enabled)
        .tooltip(label)
        .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

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
