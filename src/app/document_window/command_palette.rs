//! Window-local command palette. The owner supplies availability, not a reader
//! reference; commands and dismissal follow the normal GPUI action path.

use std::{cell::RefCell, ops::Range, rc::Rc};

use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Entity, FocusHandle, HighlightStyle, IntoElement,
    SharedString, StyledText, Window, div, prelude::*,
};
use gpui_kit::{
    base::{Disableable, FocusTrapElement, TestSupportExt as _},
    component::{
        ActiveTheme, IndexPath,
        command::{Command, CommandItem, CommandState},
    },
};

use super::{
    ClosePane, KeepOnlyPane, NavigateBack, NavigateForward, OpenPdf, Save, SaveAs, SplitSideBySide,
    SplitStacked, SwitchPane,
    sidebar::{ShowAnnotations, ShowContents, ShowFind, ToggleSidebar},
};
use crate::{
    app::windows::{NewWindow, NextWindow, PreviousWindow},
    app::{
        commands::{OPEN_VIM_TUTOR_LABEL, OpenVimTutor},
        keymap::{self, Context as KeyContext},
        shortcut_label,
    },
    reader::{
        Continuous, Copy, FitPage, FitWidth, Highlight, NextMatch, NextPage, PreviousMatch,
        PreviousPage, ReadingMode, ShowDeleteMark, ShowJumpMark, ShowMarks, ShowSetMark,
        SinglePage, UseDefaultReadingMode, ZoomIn, ZoomOut,
    },
    reading_history::ShowRecent,
    settings_ui::ShowSettings,
};

gpui_kit::actions!(kver, [ShowCommandPalette, DismissCommandPalette]);

/// Read-only presentation input, rebuilt by the owner from current state.
#[non_exhaustive]
pub(crate) struct CommandAvailability {
    pub(crate) page_ix: usize,
    pub(crate) page_count: usize,
    pub(crate) mode_override: Option<ReadingMode>,
    pub(crate) has_selection: bool,
    pub(crate) highlight_available: bool,
    pub(crate) annotation_edit_available: bool,
    pub(crate) has_annotation_undo: bool,
    pub(crate) has_annotation_redo: bool,
    pub(crate) save_available: bool,
    pub(crate) save_as_available: bool,
    pub(crate) has_matches: bool,
    pub(crate) can_back: bool,
    pub(crate) can_forward: bool,
    pub(crate) split_side_by_side: bool,
    pub(crate) split_stacked: bool,
    pub(crate) has_split: bool,
}

#[derive(Clone)]
pub(crate) struct CommandPalette {
    command: Entity<CommandState>,
    focus: FocusHandle,
    // Kit doesn't highlight custom rows. Its query callback supplies the text
    // without reading the CommandState again while it renders those rows.
    row_query: Rc<RefCell<String>>,
    installed_order: Rc<RefCell<Vec<usize>>>,
}

impl CommandPalette {
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let command = cx.new(|cx| CommandState::new(window, cx));
        command.update(cx, |command, cx| command.focus(window, cx));
        Self {
            command,
            focus: cx.focus_handle(),
            row_query: Rc::default(),
            installed_order: Rc::default(),
        }
    }

    pub(crate) fn query(&self, cx: &App) -> SharedString {
        self.command.read(cx).query(cx)
    }

    #[cfg(test)]
    pub(crate) fn has_selection(&self, cx: &App) -> bool {
        self.command.read(cx).selected_index().is_some()
    }

    /// Keep backdrop and focus-trapped panel as siblings in the host's overlay
    /// layer. No extra container changes sizing, hit testing, or action ancestry.
    pub(crate) fn render(
        &self,
        available: CommandAvailability,
        cx: &gpui_kit::Context<super::DocumentWindow>,
    ) -> [AnyElement; 2] {
        let has_document = available.page_count > 0;
        let mut items = vec![
            command(
                "Check for Updates…",
                crate::app::commands::CheckForUpdates,
                crate::app::commands::has_update_check(cx),
            ),
            command("Open PDF…", OpenPdf, true),
            command("Open URL…", super::OpenUrl, true),
            command(OPEN_VIM_TUTOR_LABEL, OpenVimTutor, true),
            self.reader_command("Follow link…", crate::reader::FollowLink, has_document, cx),
            self.reader_command(
                "Follow link in new window…",
                crate::reader::FollowLinkInNewWindow,
                has_document,
                cx,
            ),
            self.reader_command(
                "Follow link in split…",
                crate::reader::FollowLinkInSplit,
                has_document,
                cx,
            ),
            self.reader_command(
                "Visual select…",
                crate::reader::VisualSelect,
                has_document,
                cx,
            ),
            self.reader_command(
                "Visual select line…",
                crate::reader::VisualSelectLine,
                has_document,
                cx,
            ),
            self.reader_command(
                "Toggle PDF dark mode",
                crate::reader::TogglePdfDarkMode,
                has_document,
                cx,
            ),
            self.reader_command(
                "Toggle reading ruler",
                crate::reader::ToggleReadingRuler,
                has_document,
                cx,
            ),
            self.reader_command(
                "Next ruler line",
                crate::reader::NextRulerLine,
                has_document,
                cx,
            ),
            self.reader_command(
                "Previous ruler line",
                crate::reader::PreviousRulerLine,
                has_document,
                cx,
            ),
            command("Save", Save, available.save_available),
            command("Save As…", SaveAs, available.save_as_available),
            self.reader_command(
                "Undo",
                crate::reader::UndoAnnotation,
                available.has_annotation_undo,
                cx,
            ),
            self.reader_command(
                "Redo",
                crate::reader::RedoAnnotation,
                available.has_annotation_redo,
                cx,
            ),
            self.reader_command(
                "Apply annotation",
                crate::reader::ApplyMarkup,
                available.highlight_available,
                cx,
            ),
            self.reader_command("Highlight", Highlight, available.highlight_available, cx),
            self.reader_command(
                "Underline",
                crate::reader::Underline,
                available.highlight_available,
                cx,
            ),
            self.reader_command(
                "Strikethrough",
                crate::reader::StrikeOut,
                available.highlight_available,
                cx,
            ),
            self.reader_command(
                "Squiggly underline",
                crate::reader::Squiggly,
                available.highlight_available,
                cx,
            ),
            self.reader_command(
                "Delete annotation",
                crate::reader::DeleteHighlight,
                available.annotation_edit_available,
                cx,
            ),
            command("Open Recent…", ShowRecent, true),
            command("Open marks…", ShowMarks, has_document),
            command("Set local mark…", ShowSetMark, has_document),
            command("Jump to local mark…", ShowJumpMark, has_document),
            command("Delete local mark…", ShowDeleteMark, has_document),
            command("Settings…", ShowSettings, true),
            self.reader_command("Find", ShowFind, has_document, cx),
            self.reader_command("Next match", NextMatch, available.has_matches, cx),
            self.reader_command("Previous match", PreviousMatch, available.has_matches, cx),
            self.reader_command("Contents", ShowContents, has_document, cx),
            self.reader_command("Annotations", ShowAnnotations, has_document, cx),
            self.reader_command("Back", NavigateBack, available.can_back, cx),
            self.reader_command("Forward", NavigateForward, available.can_forward, cx),
            command("Toggle Sidebar", ToggleSidebar, has_document),
            self.reader_command(
                "Split side by side",
                SplitSideBySide,
                available.split_side_by_side,
                cx,
            ),
            self.reader_command(
                "Split top and bottom",
                SplitStacked,
                available.split_stacked,
                cx,
            ),
            self.reader_command("Switch pane", SwitchPane, available.has_split, cx),
            self.reader_command("Close pane", ClosePane, available.has_split, cx),
            self.reader_command("Keep only this pane", KeepOnlyPane, available.has_split, cx),
            command("New Window", NewWindow, true),
            command("Next Window", NextWindow, cx.windows().len() >= 2),
            command("Previous Window", PreviousWindow, cx.windows().len() >= 2),
            self.reader_command(
                "Next Page",
                NextPage,
                has_document && available.page_ix + 1 < available.page_count,
                cx,
            ),
            self.reader_command(
                "Previous Page",
                PreviousPage,
                has_document && available.page_ix > 0,
                cx,
            ),
            self.reader_command(
                "Single page",
                SinglePage,
                has_document && available.mode_override != Some(ReadingMode::SinglePage),
                cx,
            ),
            self.reader_command(
                "Continuous",
                Continuous,
                has_document && available.mode_override != Some(ReadingMode::Continuous),
                cx,
            ),
            command(
                "Use default reading mode",
                UseDefaultReadingMode,
                has_document && available.mode_override.is_some(),
            ),
            self.reader_command("Zoom In", ZoomIn, has_document, cx),
            self.reader_command("Zoom Out", ZoomOut, has_document, cx),
            self.reader_command("Fit Width", FitWidth, has_document, cx),
            self.reader_command("Fit Page", FitPage, has_document, cx),
            self.reader_command("Copy", Copy, available.has_selection, cx),
        ];
        for color in crate::pdf::annotations::HighlightColor::ALL {
            items.push(self.reader_command(
                format!("Recolor annotation {}", color.label()),
                crate::reader::RecolorHighlight(color),
                available.annotation_edit_available,
                cx,
            ));
        }
        let items: Vec<_> = {
            let query = self.row_query.borrow();
            let mut items: Vec<_> = items.into_iter().enumerate().collect();
            items.sort_by(|(_, a), (_, b)| a.rank.key(&query).cmp(&b.rank.key(&query)));
            items
        };
        let (next_order, ranked): (Vec<_>, Vec<_>) = items
            .iter()
            .map(|(id, item)| (*id, item.rank.clone()))
            .unzip();
        let owner = cx.entity_id();
        let state = self.command.downgrade();
        let row_query = self.row_query.clone();
        let command = Command::new(&self.command)
            .placeholder("Search actions…")
            .items(items.into_iter().map(|(_, item)| item.command))
            .highlight_matches(true)
            .on_query(move |query, window, cx| {
                let query = query.trim().to_lowercase();
                // Kit has already filtered the installed model for this query.
                // Select the best action there too, so Enter before the next
                // paint can't dispatch a lower-ranked result.
                if let Some((row, _)) = ranked
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        item.enabled && (query.is_empty() || item.key(&query).0 != MatchRank::Other)
                    })
                    .min_by_key(|(_, item)| item.key(&query))
                {
                    let _ = state.update(cx, |state, cx| {
                        state.set_selected_index(Some(IndexPath::new(row)), window, cx);
                    });
                }
                if *row_query.borrow() != query {
                    *row_query.borrow_mut() = query;
                    cx.notify(owner);
                }
            })
            .on_confirm(|_, window, cx| dismiss(window, cx))
            .on_cancel(dismiss);
        [
            div()
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .on_mouse_down(gpui_kit::MouseButton::Left, |_, window, cx| {
                    dismiss(window, cx);
                })
                .into_any_element(),
            div()
                .id("palette-panel")
                .test_support()
                .role(gpui_kit::Role::Dialog)
                .aria_label("Commands")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .child(
                    div()
                        .w_full()
                        .max_w(gpui_kit::rems(24.))
                        .child(OrderedCommand {
                            command,
                            state: self.command.clone(),
                            installed_order: self.installed_order.clone(),
                            next_order,
                        }),
                )
                .focus_trap("reader-command-palette", &self.focus)
                .into_any_element(),
        ]
    }

    fn reader_command(
        &self,
        label: impl Into<SharedString>,
        action: impl Action,
        enabled: bool,
        cx: &App,
    ) -> PaletteItem {
        let preset = keymap::active_preset(cx);
        let fallback;
        let map = if let Some(map) = keymap::active_map(cx) {
            map
        } else {
            fallback =
                keymap::resolve(keymap::platform(cx), preset).expect("embedded keyboard policy");
            &fallback
        };
        let shortcut = map
            .shortcuts(&action, KeyContext::Reader)
            .last()
            .map(|keys| shortcut_label::format(keys, cx));
        let hint_id = SharedString::from(format!("command-palette-shortcut:{}", action.name()));
        let label = label.into();
        let mut item = command(label.clone(), action, enabled);
        let Some(shortcut) = shortcut else {
            return item;
        };
        let row_query = self.row_query.clone();
        item.command = item.command.child(move |_, cx| {
            let style = HighlightStyle {
                color: Some(cx.theme().list_highlight_foreground),
                ..Default::default()
            };
            let highlights = visible_match_ranges(&label, &row_query.borrow())
                .into_iter()
                .map(|range| (range, style));
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .w_full()
                .child(StyledText::new(label.clone()).with_highlights(highlights))
                .child(
                    div()
                        .id(hint_id.clone())
                        .test_support()
                        .aria_label(shortcut.clone())
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(shortcut.clone()),
                )
        });
        item
    }
}

struct PaletteItem {
    rank: RankLabel,
    command: CommandItem,
}

#[derive(Clone)]
struct RankLabel {
    folded: String,
    enabled: bool,
}

impl RankLabel {
    fn key(&self, query: &str) -> (MatchRank, &str) {
        let label = self.folded.as_str();
        // The trailing ellipsis signals a follow-up interaction, not part of
        // the command name a reader is expected to type.
        let name = label.trim_end_matches('…');
        let rank = if query.is_empty() {
            MatchRank::Other
        } else if label == query || name == query {
            MatchRank::Exact
        } else if label.starts_with(query) {
            MatchRank::Prefix
        } else if label.contains(query) {
            MatchRank::Substring
        } else {
            MatchRank::Other
        };
        (rank, name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchRank {
    Exact,
    Prefix,
    Substring,
    Other,
}

fn command(label: impl Into<SharedString>, action: impl Action, enabled: bool) -> PaletteItem {
    let label = label.into();
    PaletteItem {
        rank: RankLabel {
            folded: label.to_lowercase(),
            enabled,
        },
        command: CommandItem::new()
            .label(label)
            .action(Box::new(action))
            .disabled(!enabled),
    }
}

/// Kit reinstalls entries in `Command::render` and preserves selection by
/// position. Translate that position when ranking moves the selected action.
#[derive(IntoElement)]
struct OrderedCommand {
    command: Command,
    state: Entity<CommandState>,
    installed_order: Rc<RefCell<Vec<usize>>>,
    next_order: Vec<usize>,
}

impl RenderOnce for OrderedCommand {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The catalog is fixed across queries; each ID is a position in that
        // catalog, not the row's current position in Kit's ordered model.
        let previous_order = self.installed_order.replace(self.next_order);
        let next_order = self.installed_order.borrow();
        let selected = if previous_order != *next_order {
            self.state
                .read(cx)
                .selected_index()
                .and_then(|index| previous_order.get(index.row))
                .and_then(|id| next_order.iter().position(|next| next == id))
        } else {
            None
        };
        let element = self.command.render(window, cx).into_any_element();
        if let Some(row) = selected {
            self.state.update(cx, |state, cx| {
                state.set_selected_index(Some(IndexPath::new(row)), window, cx);
            });
        }
        element
    }
}

/// Highlight only whole source characters in the visible label. Kit still
/// decides which rows match, including any text the custom row does not show.
fn visible_match_ranges(label: &str, folded_query: &str) -> Vec<Range<usize>> {
    if folded_query.is_empty() {
        return Vec::new();
    }

    let folded = label.to_lowercase();
    // Lowercasing can expand a character; folded offsets aren't label offsets.
    let mut offset = 0;
    let mut boundaries: Vec<_> = label
        .char_indices()
        .map(|(original, ch)| {
            let boundary = (offset, original);
            offset += ch.to_lowercase().map(char::len_utf8).sum::<usize>();
            boundary
        })
        .collect();
    boundaries.push((folded.len(), label.len()));

    let mut ranges = Vec::new();
    let mut next_start = 0;
    for &(start, original_start) in &boundaries {
        if start < next_start || !folded[start..].starts_with(folded_query) {
            continue;
        }
        let end = start + folded_query.len();
        if let Ok(index) = boundaries.binary_search_by_key(&end, |&(offset, _)| offset) {
            ranges.push(original_start..boundaries[index].1);
            next_start = end;
        }
    }
    ranges
}

fn dismiss(window: &mut Window, cx: &mut App) {
    window.dispatch_action(Box::new(DismissCommandPalette), cx);
}

#[cfg(test)]
mod tests {
    use super::{RankLabel, visible_match_ranges};

    #[test]
    fn command_order_prefers_exact_then_prefix_then_substring() {
        let mut labels = ["reopen", "open url…", "open pdf…", "open…"].map(|label| RankLabel {
            folded: label.into(),
            enabled: true,
        });
        labels.sort_by(|a, b| a.key("open").cmp(&b.key("open")));
        assert_eq!(
            labels.map(|label| label.folded),
            ["open…", "open pdf…", "open url…", "reopen"]
        );
    }

    #[test]
    fn custom_rows_highlight_only_visible_matches() {
        assert_eq!(visible_match_ranges("Banana", "an"), vec![1..3, 3..5]);
        assert!(visible_match_ranges("Recolor annotation", "blue").is_empty());
        assert!(visible_match_ranges("Next Page", "").is_empty());
    }

    #[test]
    fn custom_row_highlights_use_original_character_boundaries() {
        assert_eq!(visible_match_ranges("Café İx", "i\u{307}x"), vec![6..9]);
        assert_eq!(visible_match_ranges("Café İx", "x"), vec![8..9]);
        assert!(visible_match_ranges("İx", "i").is_empty());
        assert!(visible_match_ranges("İx", "\u{307}x").is_empty());
        assert_eq!(visible_match_ranges("ΟΣ", "ος"), vec![0..4]);
    }
}
