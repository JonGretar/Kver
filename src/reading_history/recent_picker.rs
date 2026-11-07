//! Open Recent owns its modal, query, and history operations. The document
//! owner handles open requests and replies to the originating picker/request.

use std::{cell::Cell, path::PathBuf};

use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render, Subscription,
    Task, Window, div, prelude::*, px,
};
use gpui_kit::{
    assets::IconName,
    base::{Disableable, TestSupportExt as _},
    component::{
        ActiveTheme, Sizable, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants},
        command::{Command, CommandItem, CommandState},
        dialog::DialogEntrance,
    },
};

use super::{RecentFile, history, notify_changed, revision};

gpui_kit::actions!(kver, [ShowRecent]);

pub(crate) enum RecentPickerEvent {
    Open { path: PathBuf, request: u64 },
    Closed,
}

pub(crate) struct RecentPicker {
    command: Entity<CommandState>,
    files: Vec<RecentFile>,
    excluded: Option<PathBuf>,
    error: Option<String>,
    open: bool,
    searching: bool,
    removing: bool,
    query_revision: u64,
    search_task: Option<Task<()>>,
    next_request: u64,
    pending_open: Option<u64>,
    _history_subscription: Option<Subscription>,
}

impl EventEmitter<RecentPickerEvent> for RecentPicker {}

impl RecentPicker {
    pub(crate) fn new(
        excluded: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = revision(cx).map(|revision| {
            cx.observe(&revision, |picker, _, cx| {
                if picker.open {
                    picker.search(picker.query(cx), cx);
                }
            })
        });
        Self {
            command: cx.new(|cx| CommandState::new(window, cx)),
            files: Vec::new(),
            excluded,
            error: None,
            open: false,
            searching: false,
            removing: false,
            query_revision: 0,
            search_task: None,
            next_request: 0,
            pending_open: None,
            _history_subscription: subscription,
        }
    }

    pub(crate) fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return;
        }
        self.open = true;
        self.search(String::new(), cx);
        let owner = cx.weak_entity();
        let command = self.command.clone();
        let initial_focus = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if initial_focus.replace(false) {
                command.update(cx, |state, cx| state.focus(window, cx));
            }
            let title_command = command.clone();
            let dialog_owner = owner.clone();
            let content = owner.upgrade();
            dialog
                .title(
                    div()
                        .id("recent-header")
                        .test_support()
                        .px_3()
                        .pt_3()
                        .pb_2()
                        .on_click(move |_, window, cx| {
                            title_command.update(cx, |state, cx| state.focus(window, cx));
                        })
                        .child("Open Recent"),
                )
                .close_button(false)
                .centered()
                .entrance(DialogEntrance::None)
                .p_0()
                .w(px(544.)) // Dialog's width API accepts Pixels, not rem-based lengths.
                .on_close(move |_, _, cx| {
                    let _ = dialog_owner.update(cx, |picker, cx| picker.closed(cx));
                })
                .content(move |body, _, _| body.children(content.clone()))
        });
        cx.notify();
    }

    fn closed(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.query_revision += 1;
        self.search_task = None;
        self.pending_open = None;
        cx.emit(RecentPickerEvent::Closed);
        cx.notify();
    }

    /// Update the normalized active PDF without giving the picker ownership of
    /// document identity. An outstanding open remains busy during any refresh.
    pub(crate) fn set_active_path(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.excluded != path {
            self.excluded = path;
            if self.open {
                self.search(self.query(cx), cx);
            }
        }
    }

    pub(crate) fn files(&self) -> &[RecentFile] {
        &self.files
    }

    pub(crate) fn query(&self, cx: &App) -> String {
        self.command.read(cx).query(cx).to_string()
    }

    #[cfg(test)]
    pub(crate) fn selected_row(&self, cx: &App) -> Option<usize> {
        self.command
            .read(cx)
            .selected_index()
            .map(|index| index.row)
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.searching || self.removing || self.pending_open.is_some()
    }

    fn search(&mut self, query: String, cx: &mut Context<Self>) {
        self.query_revision += 1;
        let revision = self.query_revision;
        self.search_task = None;
        self.searching = true;
        self.error = None;
        let Some(history) = history(cx) else {
            self.error = Some("Local reading history is unavailable.".into());
            self.searching = false;
            cx.notify();
            return;
        };
        let work = history.search_excluding(query, self.excluded.clone());
        self.search_task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                if this.open && this.query_revision == revision {
                    this.searching = false;
                    match result {
                        Ok(files) => this.files = files,
                        Err(error) => {
                            this.files.clear();
                            this.error = Some(format!(
                                "{error}. Check the local data directory and try again."
                            ));
                        }
                    }
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    fn request_open(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.open || self.is_loading() {
            return;
        }
        let Some(file) = self.files.get(ix) else {
            return;
        };
        self.next_request += 1;
        self.pending_open = Some(self.next_request);
        self.error = None;
        cx.emit(RecentPickerEvent::Open {
            path: file.path.clone(),
            request: self.next_request,
        });
        cx.notify();
    }

    pub(crate) fn cancel_open(&mut self, request: u64, cx: &mut Context<Self>) {
        if self.pending_open == Some(request) {
            self.pending_open = None;
            cx.notify();
        }
    }

    pub(crate) fn finish_open(
        &mut self,
        request: u64,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open || self.pending_open != Some(request) {
            return;
        }
        self.pending_open = None;
        match result {
            Ok(()) => {
                self.closed(cx);
                window.close_dialog(cx);
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not open this PDF: {error}. Use the X on its row to forget its record without deleting the PDF."
                ));
                cx.notify();
            }
        }
    }

    /// Called only after confirmation. The queued history mutation must finish
    /// and notify other windows even if this modal closes before the reply.
    pub(crate) fn remove(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.open || self.is_loading() || !self.files.iter().any(|file| file.path == path) {
            return;
        }
        let Some(history) = history(cx) else {
            return;
        };
        self.removing = true;
        cx.notify();
        let work = history.remove(path.clone());
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let changed = result.is_ok();
            let _ = this.update(cx, |this, cx| {
                this.removing = false;
                if this.open {
                    match result {
                        Ok(_) => {
                            this.files.retain(|file| file.path != path);
                            this.error = None;
                        }
                        Err(error) => {
                            this.error = Some(format!("Cannot remove saved data: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
            if changed {
                cx.update(notify_changed);
            }
        })
        .detach();
    }

    fn confirm_remove(&self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_loading() {
            return;
        }
        let name = document_name(&path);
        let owner = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let owner = owner.clone();
            let focus_owner = owner.clone();
            let path = path.clone();
            alert
                .title(format!("Forget \"{name}\"?"))
                .description("Remove this PDF from Open Recent and erase its saved reading data. The PDF file will not be deleted.")
                .confirm()
                .centered()
                .entrance(DialogEntrance::None)
                .ok_text("Forget PDF")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    let _ = owner.update(cx, |picker, cx| picker.remove(path.clone(), cx));
                    true
                })
                .on_close(move |_, window, _| {
                    let owner = focus_owner.clone();
                    window.on_next_frame(move |window, cx| {
                        let _ = owner.update(cx, |picker, cx| {
                            if picker.open {
                                picker.command.update(cx, |state, cx| state.focus(window, cx));
                            }
                        });
                    });
                })
        });
    }
}

impl Focusable for RecentPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.command.read(cx).focus_handle(cx)
    }
}

impl Render for RecentPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        let loading = self.is_loading();
        let entries = self
            .files()
            .iter()
            .map(|file| {
                let path = file.path.display().to_string();
                let name = document_name(&file.path);
                let file_path = file.path.clone();
                let owner = owner.clone();
                CommandItem::new()
                    .label(name.clone())
                    .keywords([path.clone()])
                    .disabled(loading)
                    .child(move |_, cx| {
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().truncate().child(name.clone()))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(path.clone()),
                                    ),
                            )
                            .child(
                                Button::new(format!("forget-pdf:{path}"))
                                    .icon(IconName::Close)
                                    .accessibility_label(format!("Forget {}", name))
                                    .tooltip("Forget PDF")
                                    .ghost()
                                    .xsmall()
                                    .disabled(loading)
                                    .on_click({
                                        let owner = owner.clone();
                                        let path = file_path.clone();
                                        move |_, window, cx| {
                                            cx.stop_propagation();
                                            let _ = owner.update(cx, |picker, cx| {
                                                picker.confirm_remove(path.clone(), window, cx);
                                            });
                                        }
                                    }),
                            )
                    })
            })
            .collect::<Vec<_>>();
        div()
            .id("recent-panel")
            .test_support()
            .role(gpui_kit::Role::Dialog)
            .aria_label("Open Recent")
            .w_full()
            .child(
                Command::new(&self.command)
                    .bordered(false)
                    .placeholder("Search stored PDFs…")
                    .items(entries)
                    .empty(|_, _, cx| {
                        div()
                            .p_4()
                            .text_color(cx.theme().muted_foreground)
                            .child("No stored PDFs match this search")
                    })
                    .on_query({
                        let owner = owner.clone();
                        move |query, _, cx| {
                            let _ =
                                owner.update(cx, |picker, cx| picker.search(query.to_owned(), cx));
                        }
                    })
                    .on_confirm(move |index, _, cx| {
                        let _ = owner.update(cx, |picker, cx| picker.request_open(index.row, cx));
                    }),
            )
            .when(loading, |content| {
                content.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading stored PDFs…"),
                )
            })
            .when_some(self.error().map(str::to_owned), |content, error| {
                content.child(
                    div()
                        .id("recent-error")
                        .test_support()
                        .role(gpui_kit::Role::Status)
                        .aria_label(error.clone())
                        .px_3()
                        .py_2()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
    }
}

fn document_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests;
