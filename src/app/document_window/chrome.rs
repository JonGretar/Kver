//! Window chrome is value-like presentation of the active document's identity.

use gpui_kit::{App, SharedString, div, prelude::*, px};
use gpui_kit::{
    assets::IconName,
    base::{Disableable, TestSupportExt as _},
    component::{
        ActiveTheme, Selectable, Sizable, TitleBar,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        menu::{DropdownMenu as _, PopupMenuItem},
        status_bar::StatusBar,
        toolbar::{Toolbar, ToolbarGroup},
        tooltip::Tooltip,
    },
};
use std::path::Path;

use super::sidebar::ToggleSidebar;
use super::{NavigateBack, NavigateForward, OpenPdf};
use crate::{
    reader::{
        Continuous, FitPage, FitWidth, NextPage, PreviousPage, ReaderView, ReadingMode, SinglePage,
        ZoomIn, ZoomMode, ZoomOut,
    },
    settings_ui::ShowSettings,
};

pub(super) fn reading_toolbar(
    reader: &ReaderView,
    sidebar_open: bool,
    can_back: bool,
    can_forward: bool,
    available_rems: f32,
    cx: &App,
) -> Toolbar {
    let has_document = reader.page_count().is_some();
    // A resized sidebar consumes the same space that controls need. Both
    // thresholds use the remaining pane width at the current interface scale.
    let show_fit_labels = available_rems >= 44.;
    let overflow = available_rems < 28.;
    let history = ToolbarGroup::new("history-controls")
        .label("Navigation history")
        .content(reader_button(
            "navigate-back",
            "Back",
            IconName::ArrowLeft,
            Box::new(NavigateBack),
            can_back,
            false,
            cx,
        ))
        .content(reader_button(
            "navigate-forward",
            "Forward",
            IconName::ArrowRight,
            Box::new(NavigateForward),
            can_forward,
            false,
            cx,
        ));
    let pages = ToolbarGroup::new("page-controls")
        .label("Page navigation")
        .content(reader_button(
            "previous-page",
            "Previous page",
            IconName::ArrowUp,
            Box::new(PreviousPage),
            has_document && reader.page_index() > 0,
            false,
            cx,
        ))
        .content(reader_button(
            "next-page",
            "Next page",
            IconName::ArrowDown,
            Box::new(NextPage),
            reader.page_index() + 1 < reader.page_count().unwrap_or(0),
            false,
            cx,
        ));
    let single_page = reader.reading_mode() == ReadingMode::SinglePage;
    let mode_label = match (reader.reading_mode(), reader.mode_override()) {
        (ReadingMode::SinglePage, Some(_)) => "Single page for this PDF · switch to continuous",
        (ReadingMode::SinglePage, None) => "Single page (default) · switch to continuous",
        (ReadingMode::Continuous, Some(_)) => "Continuous for this PDF · switch to single page",
        (ReadingMode::Continuous, None) => "Continuous (default) · switch to single page",
    };
    let mode = reader_button(
        "reading-mode",
        mode_label,
        IconName::FileText,
        if single_page {
            Box::new(Continuous)
        } else {
            Box::new(SinglePage)
        },
        has_document,
        single_page,
        cx,
    )
    .when(show_fit_labels, |button| {
        button.label(if single_page {
            "Single page"
        } else {
            "Continuous"
        })
    });
    let zoom = reader.zoom_mode();
    let zoom_controls = ToolbarGroup::new("zoom-controls")
        .label("Zoom and fit")
        .content(reader_button(
            "zoom-out",
            "Zoom out",
            IconName::ZoomOut,
            Box::new(ZoomOut),
            has_document,
            false,
            cx,
        ))
        .content(reader_button(
            "zoom-in",
            "Zoom in",
            IconName::ZoomIn,
            Box::new(ZoomIn),
            has_document,
            false,
            cx,
        ))
        .content(
            reader_button(
                "fit-width",
                "Fit width",
                IconName::MoveHorizontal,
                Box::new(FitWidth),
                has_document,
                has_document && zoom == ZoomMode::FitWidth,
                cx,
            )
            .when(show_fit_labels, |button| button.label("Fit width")),
        )
        .content(
            reader_button(
                "fit-page",
                "Fit page",
                IconName::Expand,
                Box::new(FitPage),
                has_document,
                has_document && zoom == ZoomMode::FitPage,
                cx,
            )
            .when(show_fit_labels, |button| button.label("Fit page")),
        );
    Toolbar::new("reading-toolbar")
        .px_3()
        .content(reader_button(
            "toggle-sidebar",
            if sidebar_open {
                "Hide sidebar"
            } else {
                "Show sidebar"
            },
            IconName::PanelLeft,
            Box::new(ToggleSidebar),
            has_document,
            sidebar_open,
            cx,
        ))
        .when(!overflow, |toolbar| {
            toolbar
                .content(gpui_kit::component::separator::Separator::vertical().h_5())
                .content(history)
        })
        .content(pages)
        .when(!overflow, |toolbar| {
            toolbar
                .content(gpui_kit::component::separator::Separator::vertical().h_5())
                .content(mode)
                .content(gpui_kit::component::separator::Separator::vertical().h_5())
                .content(zoom_controls)
        })
        .content(div().flex_1())
        .when(overflow, |toolbar| {
            toolbar.content(
                Button::new("reader-overflow")
                    .ghost()
                    .small()
                    .icon(IconName::Ellipsis)
                    .accessibility_label("More reader controls")
                    .tooltip("More reader controls")
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::new("Back")
                                .action(Box::new(NavigateBack))
                                .disabled(!can_back),
                        )
                        .item(
                            PopupMenuItem::new("Forward")
                                .action(Box::new(NavigateForward))
                                .disabled(!can_forward),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Single page")
                                .action(Box::new(SinglePage))
                                .checked(single_page)
                                .disabled(!has_document),
                        )
                        .item(
                            PopupMenuItem::new("Continuous")
                                .action(Box::new(Continuous))
                                .checked(!single_page)
                                .disabled(!has_document),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Zoom in")
                                .action(Box::new(ZoomIn))
                                .disabled(!has_document),
                        )
                        .item(
                            PopupMenuItem::new("Zoom out")
                                .action(Box::new(ZoomOut))
                                .disabled(!has_document),
                        )
                        .item(
                            PopupMenuItem::new("Fit width")
                                .action(Box::new(FitWidth))
                                .checked(zoom == ZoomMode::FitWidth)
                                .disabled(!has_document),
                        )
                        .item(
                            PopupMenuItem::new("Fit page")
                                .action(Box::new(FitPage))
                                .checked(zoom == ZoomMode::FitPage)
                                .disabled(!has_document),
                        )
                    }),
            )
        })
        .content(reader_button(
            "settings-button",
            "Settings…",
            IconName::Settings,
            Box::new(ShowSettings),
            true,
            false,
            cx,
        ))
}

pub(super) fn status_bar(reader: &ReaderView) -> StatusBar {
    let has_document = reader.page_count().is_some();
    let status = reader.page_count().map_or_else(
        || reader.status().to_owned(),
        |count| format!("Page {} of {count}", reader.page_index() + 1),
    );
    StatusBar::new()
        .font_features(gpui_kit::FontFeatures(vec![("tnum".into(), 1)].into()))
        .left(
            div()
                .id("reader-status")
                .test_support()
                .aria_label(status.clone())
                .min_w_0()
                .truncate()
                .child(status),
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
            let zoom = format!(
                "{}%{}",
                (reader.scale() * 100.0).round(),
                match reader.zoom_mode() {
                    ZoomMode::Fixed => "",
                    ZoomMode::FitWidth => " · Fit width",
                    ZoomMode::FitPage => " · Fit page",
                }
            );
            bar.right(
                div()
                    .id("reader-zoom")
                    .test_support()
                    .aria_label(zoom.clone())
                    .child(zoom),
            )
        })
}

fn reader_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    action: Box<dyn gpui_kit::Action>,
    enabled: bool,
    selected: bool,
    cx: &App,
) -> Button {
    Button::new(id)
        .accessibility_label(label)
        .icon(icon)
        .ghost()
        // Sized explicitly because Toolbar::child resets custom variants to
        // ghost. Keep the toolbar's compact geometry without losing selection.
        .small()
        .compact()
        .when(selected, |button| {
            button.custom(
                ButtonCustomVariant::new(cx)
                    .foreground(cx.theme().primary)
                    .active(cx.theme().selection),
            )
        })
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
