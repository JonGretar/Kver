//! Local mark interaction for one document snapshot. Saved/session marks stay
//! in reading history; navigation is emitted in document coordinates.

use std::{collections::BTreeMap, path::PathBuf};

use gpui_kit as gpui;
use gpui_kit::{
    Context, Entity, EventEmitter, FocusHandle, Subscription, Task, Window,
    assets::IconName,
    base::{Dialog, DialogPopup, Disableable, TestSupportExt as _},
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
        command::{Command, CommandItem, CommandState},
        input::{Input, InputEvent, InputState},
    },
    div,
    prelude::*,
};

use crate::reading_history::{self as history, LocalMark, ReadingPosition};

gpui::actions!(kver, [ShowMarks, ShowSetMark, ShowJumpMark, ShowDeleteMark]);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct SetLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct JumpLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct DeleteLocalMark(pub char);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarksView {
    List,
    Set,
    Jump,
    Delete,
    Name(char),
}

pub(crate) enum MarksEvent {
    Jump {
        letter: char,
        position: ReadingPosition,
    },
    Status(String),
}

/// Owned by the document's composition. Replacing a document drops this owner;
/// async replies hold it weakly and cannot affect the replacement.
pub(crate) struct LocalMarks {
    path: PathBuf,
    generation: u64,
    panel: Option<MarksPanel>,
    focus: FocusHandle,
    panel_revision: u64,
    read_revision: u64,
    read_task: Option<Task<()>>,
    jump_task: Option<Task<()>>,
    name_subscription: Option<Subscription>,
    _history_subscription: Option<Subscription>,
}

#[derive(Clone)]
struct MarksPanel {
    command: Entity<CommandState>,
    restore_focus: FocusHandle,
    view: MarksView,
    marks: BTreeMap<char, LocalMark>,
    name_input: Option<Entity<InputState>>,
    loading: bool,
    saving: bool,
    error: Option<String>,
}

impl EventEmitter<MarksEvent> for LocalMarks {}

impl LocalMarks {
    pub(crate) fn new(path: PathBuf, generation: u64, cx: &mut Context<Self>) -> Self {
        let subscription = history::revision(cx)
            .map(|revision| cx.observe(&revision, |marks, _, cx| marks.refresh(cx)));
        Self {
            path,
            generation,
            panel: None,
            focus: cx.focus_handle(),
            panel_revision: 0,
            read_revision: 0,
            read_task: None,
            jump_task: None,
            name_subscription: None,
            _history_subscription: subscription,
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn is_open(&self) -> bool {
        self.panel.is_some()
    }

    pub(crate) fn show(
        &mut self,
        view: MarksView,
        restore_focus: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hide(cx);
        let command = cx.new(|cx| CommandState::new(window, cx));
        command.update(cx, |state, cx| state.focus(window, cx));
        self.panel = Some(MarksPanel {
            command,
            restore_focus,
            view,
            marks: BTreeMap::new(),
            name_input: None,
            loading: true,
            saving: false,
            error: None,
        });
        self.refresh(cx);
    }

    /// Superseding overlays supply their own focus target.
    pub(crate) fn hide(&mut self, cx: &mut Context<Self>) {
        self.panel_revision += 1;
        self.panel = None;
        self.name_subscription = None;
        self.read_task = None;
        cx.notify();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.panel {
            window.focus(&panel.restore_focus, cx);
        }
        self.hide(cx);
    }

    fn delete_from_list(&mut self, letter: char, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &self.panel else { return };
        if panel.view != MarksView::List {
            return;
        }
        // The clicked button disappears with its row. Keep keyboard input on
        // the persistent search field, not a removed focus node.
        panel
            .command
            .update(cx, |command, cx| command.focus(window, cx));
        self.change(letter, None, cx);
    }

    fn edit_name(&mut self, letter: char, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        if panel.saving {
            return;
        }
        let Some(mark) = panel.marks.get(&letter) else {
            return;
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Optional name")
                .default_value(mark.label.clone().unwrap_or_default())
        });
        self.name_subscription =
            Some(
                cx.subscribe_in(&input, window, move |marks, _, event, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        marks.save_name(letter, cx);
                    }
                }),
            );
        self.panel_revision += 1;
        panel.view = MarksView::Name(letter);
        panel.name_input = Some(input.clone());
        panel.error = None;
        input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn cancel_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.panel.as_mut() {
            if panel.saving {
                return;
            }
            self.panel_revision += 1;
            panel.name_input = None;
            self.name_subscription = None;
            panel.view = MarksView::List;
            panel.error = None;
            panel
                .command
                .update(cx, |command, cx| command.focus(window, cx));
            cx.notify();
        }
    }

    fn save_name(&mut self, letter: char, cx: &mut Context<Self>) {
        let (Some(panel), Some(history)) = (self.panel.as_mut(), history::history(cx)) else {
            return;
        };
        if panel.loading || panel.saving || panel.view != MarksView::Name(letter) {
            return;
        }
        let Some(input) = panel.name_input.as_ref() else {
            return;
        };
        let name = input.read(cx).value().trim().to_owned();
        if name.chars().count() > 80 || name.chars().any(char::is_control) {
            panel.error = Some("Use at most 80 characters on one line.".into());
            cx.notify();
            return;
        }
        panel.saving = true;
        panel.error = None;
        let revision = self.panel_revision;
        let work = history.name_mark(
            self.path.clone(),
            letter,
            (!name.is_empty()).then_some(name),
        );
        // A submitted mutation belongs to the shared worker, not the popup.
        // Always notify other windows, even if this owner has since been dropped.
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let changed = result.is_ok();
            let _ = this.update_in(cx, |this, window, cx| {
                if this.panel_revision != revision {
                    return;
                }
                if let Some(panel) = this.panel.as_mut() {
                    panel.saving = false;
                    match result {
                        Ok(_) => this.cancel_name(window, cx),
                        Err(error) => {
                            panel.error = Some(format!("Mark name could not be saved: {error}"));
                            cx.notify();
                        }
                    }
                }
            });
            if changed {
                cx.update(history::notify_changed);
            }
        })
        .detach();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.panel.is_none() {
            return;
        }
        self.read_revision += 1;
        let revision = self.read_revision;
        self.read_task = None;
        let Some(history) = history::history(cx) else {
            if let Some(panel) = self.panel.as_mut() {
                panel.loading = false;
                panel.error = Some("Local reading history is unavailable.".into());
            }
            cx.notify();
            return;
        };
        let epoch = history.epoch();
        let work = history.marks(self.path.clone(), epoch);
        self.read_task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.read_revision != revision || history.epoch() != epoch {
                    return;
                }
                if let Some(panel) = this.panel.as_mut() {
                    panel.loading = false;
                    match result {
                        Ok(marks) => panel.marks = marks,
                        Err(error) => {
                            panel.error = Some(format!("Marks could not be loaded: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(crate) fn change(
        &mut self,
        letter: char,
        position: Option<ReadingPosition>,
        cx: &mut Context<Self>,
    ) {
        if !letter.is_ascii_lowercase() {
            return;
        }
        let Some(history) = history::history(cx) else {
            return;
        };
        let work = history.change_mark(self.path.clone(), letter, position);
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let changed = result.is_ok();
            let _ = this.update(cx, |_, cx| {
                let status = match result {
                    Ok(_) if position.is_some() => format!("Set local mark {letter}"),
                    Ok(_) => format!("Deleted local mark {letter}"),
                    Err(error) => format!("Local mark could not be saved: {error}"),
                };
                cx.emit(MarksEvent::Status(status));
            });
            // Refresh from the worker rather than applying an old mutation's
            // returned snapshot after a newer edit or explicit forget.
            if changed {
                cx.update(history::notify_changed);
            }
        })
        .detach();
    }

    pub(crate) fn jump(&mut self, letter: char, cx: &mut Context<Self>) {
        if !letter.is_ascii_lowercase() {
            return;
        }
        let Some(history) = history::history(cx) else {
            return;
        };
        let epoch = history.epoch();
        let work = history.marks(self.path.clone(), epoch);
        self.jump_task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |_, cx| {
                if history.epoch() != epoch {
                    return;
                }
                let event = match result {
                    Ok(marks) => match marks.get(&letter) {
                        Some(mark) => MarksEvent::Jump {
                            letter,
                            position: mark.position,
                        },
                        None => MarksEvent::Status(format!("Local mark {letter} is not set")),
                    },
                    Err(error) => {
                        MarksEvent::Status(format!("Local mark could not be loaded: {error}"))
                    }
                };
                cx.emit(event);
            });
        }));
    }

    #[cfg(test)]
    pub(crate) fn view(&self) -> Option<MarksView> {
        self.panel.as_ref().map(|panel| panel.view)
    }

    #[cfg(test)]
    pub(crate) fn listed_marks(&self) -> &BTreeMap<char, LocalMark> {
        &self.panel.as_ref().expect("marks popup open").marks
    }

    #[cfg(test)]
    pub(crate) fn query(&self, cx: &gpui::App) -> String {
        self.panel
            .as_ref()
            .expect("marks popup open")
            .command
            .read(cx)
            .query(cx)
            .to_string()
    }

    #[cfg(test)]
    pub(crate) fn name_input(&self) -> &Entity<InputState> {
        self.panel
            .as_ref()
            .and_then(|panel| panel.name_input.as_ref())
            .expect("name editor open")
    }
}

impl Render for LocalMarks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(panel) = self.panel.clone() else {
            return gpui::Empty.into_any_element();
        };
        let owner = cx.weak_entity();
        let panel_revision = self.panel_revision;
        let rows: Vec<_> = match panel.view {
            MarksView::Set => ('a'..='z')
                .map(|letter| (letter, panel.marks.get(&letter).cloned()))
                .collect(),
            MarksView::Name(_) => Vec::new(),
            _ => panel
                .marks
                .iter()
                .map(|(&letter, mark)| (letter, Some(mark.clone())))
                .collect(),
        };
        let mut entries = Vec::new();
        if panel.view == MarksView::List {
            entries.push(
                CommandItem::new()
                    .label("Set local mark…")
                    .keywords(["new mark"]),
            );
        }
        for (letter, mark) in &rows {
            let letter = *letter;
            let label = match mark {
                Some(mark) => format!(
                    "{letter}    {}Page {} · {:.0} pt from top",
                    mark.label
                        .as_ref()
                        .map(|label| format!("{label} · "))
                        .unwrap_or_default(),
                    mark.position.page + 1,
                    mark.position.y
                ),
                None => format!("{letter}    Unset"),
            };
            let mut entry = CommandItem::new()
                .label(label.clone())
                .keywords([letter.to_string()]);
            if panel.view == MarksView::List {
                let owner = owner.clone();
                let has_name = mark.as_ref().and_then(|mark| mark.label.as_ref()).is_some();
                entry = entry.child(move |_, _| {
                    let owner = owner.clone();
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .gap_1()
                                .child(
                                    Button::new(format!("name-mark:{letter}"))
                                        .icon(IconName::Pencil)
                                        .accessibility_label(format!(
                                            "{} local mark {letter}",
                                            if has_name { "Rename" } else { "Name" }
                                        ))
                                        .tooltip(if has_name { "Rename mark" } else { "Name mark" })
                                        .ghost()
                                        .xsmall()
                                        .on_click({
                                            let owner = owner.clone();
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                let _ = owner.update(cx, |marks, cx| {
                                                    marks.edit_name(letter, window, cx)
                                                });
                                            }
                                        }),
                                )
                                .child(
                                    Button::new(format!("delete-mark:{letter}"))
                                        .icon(IconName::Close)
                                        .accessibility_label(format!("Delete local mark {letter}"))
                                        .tooltip("Delete mark")
                                        .ghost()
                                        .xsmall()
                                        .on_click(move |_, window, cx| {
                                            cx.stop_propagation();
                                            let _ = owner.update(cx, |marks, cx| {
                                                marks.delete_from_list(letter, window, cx)
                                            });
                                        }),
                                ),
                        )
                });
            }
            entries.push(entry);
        }
        let title = match panel.view {
            MarksView::List => "Local marks",
            MarksView::Set => "Set local mark",
            MarksView::Jump => "Jump to local mark",
            MarksView::Delete => "Delete local mark",
            MarksView::Name(_) => "Name local mark",
        };
        div()
            .id("reader-marks-palette")
            .test_support()
            .aria_label(title)
            .absolute()
            .inset_0()
            .child(
                Dialog::new(cx)
                    .focus_handle(self.focus.clone())
                    .backdrop(div().absolute().inset_0().bg(cx.theme().overlay))
                    .on_ok(|_, _, _| false)
                    .on_cancel({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            owner
                                .update(cx, |this, cx| {
                                    if this.panel_revision != panel_revision {
                                        return false;
                                    }
                                    if this.panel.as_ref().is_some_and(|panel| {
                                        matches!(panel.view, MarksView::Name(_))
                                    }) {
                                        this.cancel_name(window, cx);
                                        false
                                    } else {
                                        true
                                    }
                                })
                                .unwrap_or(true)
                        }
                    })
                    .on_close(cx.listener(move |this, _, window, cx| {
                        if this.panel_revision == panel_revision {
                            this.dismiss(window, cx);
                        }
                    }))
                    .p_4()
                    .popup(
                        DialogPopup::new()
                            .w_full()
                            .max_w(gpui::rems(28.))
                            .bg(cx.theme().popover)
                            .child(div().px_3().pt_3().pb_2().child(title))
                            .when(panel.name_input.is_none(), {
                                let command_owner = owner.clone();
                                let revision = self.panel_revision;
                                move |content| {
                                    content.child(
                                        Command::new(&panel.command)
                                            .placeholder("Search marks…")
                                            .items(entries)
                                            .empty(|_, _, cx| {
                                                div().p_4()
                            .text_color(cx.theme().muted_foreground)
                            .child("No local marks match. Set one with m then a letter.")
                                            })
                                            .on_confirm({
                                                let owner = command_owner.clone();
                                                move |index, window, cx| {
                                                    let view = panel.view;
                                                    let letter = if view == MarksView::List
                                                        && index.row == 0
                                                    {
                                                        None
                                                    } else {
                                                        rows.get(
                                                            index.row
                                                                - usize::from(
                                                                    view == MarksView::List,
                                                                ),
                                                        )
                                                        .map(|(letter, _)| *letter)
                                                    };
                                                    let _ = owner.update(cx, |marks, cx| {
                                                        if marks.panel_revision != revision
                                                            || marks.panel.as_ref().is_none_or(
                                                                |panel| {
                                                                    panel.loading || panel.saving
                                                                },
                                                            )
                                                        {
                                                            return;
                                                        }
                                                        match letter {
                                                            None if view == MarksView::List => {
                                                                marks.show(
                                                                    MarksView::Set,
                                                                    panel.restore_focus.clone(),
                                                                    window,
                                                                    cx,
                                                                )
                                                            }
                                                            Some(letter) => {
                                                                marks.dismiss(window, cx);
                                                                let action: Box<dyn gpui::Action> =
                                                                    match view {
                                                                        MarksView::Set => Box::new(
                                                                            SetLocalMark(letter),
                                                                        ),
                                                                        MarksView::Delete => {
                                                                            Box::new(
                                                                                DeleteLocalMark(
                                                                                    letter,
                                                                                ),
                                                                            )
                                                                        }
                                                                        MarksView::Jump
                                                                        | MarksView::List => {
                                                                            Box::new(JumpLocalMark(
                                                                                letter,
                                                                            ))
                                                                        }
                                                                        MarksView::Name(_) => {
                                                                            return;
                                                                        }
                                                                    };
                                                                window.dispatch_action(action, cx);
                                                            }
                                                            None => {}
                                                        }
                                                    });
                                                }
                                            })
                                            .on_cancel(move |window, cx| {
                                                let _ = command_owner.update(cx, |marks, cx| {
                                                    if marks.panel_revision == revision {
                                                        marks.dismiss(window, cx);
                                                    }
                                                });
                                            }),
                                    )
                                }
                            })
                            .when_some(panel.name_input.clone(), |content, input| {
                                let MarksView::Name(letter) = panel.view else {
                                    return content;
                                };
                                content.child(
                                    div()
                                        .px_3()
                                        .pb_3()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(div().text_color(cx.theme().muted_foreground).child(
                                            format!(
                                                "Mark {letter} · Leave blank to remove its name"
                                            ),
                                        ))
                                        .child(Input::new(&input))
                                        .child(
                                            div()
                                                .flex()
                                                .justify_end()
                                                .gap_2()
                                                .child(
                                                    Button::new("cancel-mark-name")
                                                        .label("Cancel")
                                                        .ghost()
                                                        .disabled(panel.saving)
                                                        .on_click({
                                                            let owner = owner.clone();
                                                            move |_, window, cx| {
                                                                let _ = owner.update(
                                                                    cx,
                                                                    |marks, cx| {
                                                                        marks
                                                                            .cancel_name(window, cx)
                                                                    },
                                                                );
                                                            }
                                                        }),
                                                )
                                                .child(
                                                    Button::new("save-mark-name")
                                                        .label("Save")
                                                        .primary()
                                                        .disabled(panel.loading || panel.saving)
                                                        .on_click({
                                                            let owner = owner.clone();
                                                            move |_, _, cx| {
                                                                let _ = owner.update(
                                                                    cx,
                                                                    |marks, cx| {
                                                                        marks.save_name(letter, cx)
                                                                    },
                                                                );
                                                            }
                                                        }),
                                                ),
                                        ),
                                )
                            })
                            .when(panel.loading || panel.saving, |content| {
                                content.child(div().px_3().py_2().child("Loading marks…"))
                            })
                            .when_some(panel.error, |content, error| {
                                content.child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_color(cx.theme().danger)
                                        .child(error),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
