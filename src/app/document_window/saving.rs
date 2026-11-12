//! Window-owned Save/Save As and deliberate continuations away from edits.
//! Document bytes, revisions, and the write itself belong to DocumentSession.

use gpui_kit::{
    Context, Entity, Window,
    component::{
        Disableable as _, WindowExt as _,
        button::{Button, ButtonVariants as _},
        dialog::{DialogEntrance, DialogFooter},
        notification::Notification,
    },
    prelude::*,
};
use std::{path::PathBuf, sync::Arc};

use super::{DocumentWindow, windows};
use crate::reader::document_session::{Destination, DocumentSession, SaveError};

gpui_kit::actions!(kver, [Save, SaveAs]);

pub(super) enum Leave {
    Close,
    Replace(PathBuf),
    Quit,
}

impl DocumentWindow {
    fn cancel_pending_leave(&mut self, cx: &mut Context<Self>) {
        let pending = self.pending_leave.take();
        if matches!(pending, Some(Leave::Quit)) {
            crate::app::quit_guard::cancel_request();
        }
        self.save_prompt_open = false;
        if let Some(Leave::Replace(path)) = pending
            && self
                .recent_open
                .as_ref()
                .is_some_and(|pending| pending.path == path)
            && let Some(pending) = self.recent_open.take()
        {
            let _ = pending
                .picker
                .update(cx, |picker, cx| picker.cancel_open(pending.request, cx));
        }
        cx.notify();
    }

    pub(crate) fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_leave(Leave::Quit, window, cx);
    }

    pub(super) fn request_leave(
        &mut self,
        intent: Leave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session = self.reader.read(cx).session();
        if matches!(intent, Leave::Quit) {
            if self.pending_leave.is_some() {
                if !matches!(self.pending_leave, Some(Leave::Quit)) {
                    let prompt_open = self.save_prompt_open;
                    self.cancel_pending_leave(cx);
                    self.save_prompt_open = prompt_open;
                    self.pending_leave = Some(Leave::Quit);
                }
                return;
            }
            if self.save_prompt_open {
                self.pending_leave = Some(Leave::Quit);
                return;
            }
        }
        if self.pending_leave.is_some() || self.save_prompt_open {
            return;
        }
        if let Some(session) = &session {
            let (saving, dirty, participants) = {
                let session = session.read(cx);
                (
                    session.active_save_ticket(),
                    session.is_dirty(),
                    session.participant_count(),
                )
            };
            if let Some(ticket) = saving {
                if matches!(intent, Leave::Quit) {
                    self.pending_leave = Some(Leave::Quit);
                    self.observe_save(session.clone(), ticket, false, window, cx);
                    return;
                }
                window.push_notification(
                    Notification::warning("Wait for this PDF to finish saving before leaving it."),
                    cx,
                );
                return;
            }
            if dirty && (matches!(intent, Leave::Quit) || participants <= 1) {
                self.pending_leave = Some(intent);
                self.confirm_unsaved(window, cx);
                return;
            }
        }
        self.perform_leave(intent, window, cx);
    }

    fn perform_leave(&mut self, intent: Leave, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            Leave::Close => {
                self.reader
                    .update(cx, |reader, cx| reader.detach_session(cx));
                window.remove_window();
            }
            Leave::Replace(path) => self.replace_path(path, cx),
            Leave::Quit => cx.defer(windows::request_quit),
        }
    }

    fn confirm_unsaved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let filename = self
            .reader
            .read(cx)
            .path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "this PDF".into());
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let saving = owner
                .upgrade()
                .is_some_and(|owner| owner.read(cx).reader.read(cx).is_saving(cx));
            let save = owner.clone();
            let discard = owner.clone();
            let cancel = owner.clone();
            let cancelled = owner.clone();
            let confirm = owner.clone();
            dialog
                .title(format!("Save changes to “{filename}”?"))
                .entrance(DialogEntrance::None)
                .close_button(false)
                .overlay_closable(false)
                .child("Discard removes changes made since the last successful save.")
                .on_cancel(move |_, _, cx| {
                    let _ = cancelled.update(cx, |this, cx| this.cancel_pending_leave(cx));
                    true
                })
                .on_ok(move |_, window, cx| {
                    let _ = confirm.update(cx, |this, cx| this.save_before_leaving(window, cx));
                    false
                })
                .footer(
                    DialogFooter::new()
                        .child(Button::new("unsaved-cancel").label("Cancel").on_click(
                            move |_, window, cx| {
                                let _ = cancel.update(cx, |this, cx| this.cancel_pending_leave(cx));
                                window.close_dialog(cx);
                            },
                        ))
                        .child(
                            Button::new("unsaved-discard")
                                .label("Discard")
                                .danger()
                                .disabled(saving)
                                .on_click(move |_, window, cx| {
                                    let _ = discard.update(cx, |this, cx| {
                                        if this.reader.read(cx).is_saving(cx) {
                                            return;
                                        }
                                        let intent = this.pending_leave.take();
                                        if let Some(session) = this.reader.read(cx).session() {
                                            session.update(cx, |session, cx| session.discard(cx));
                                        }
                                        window.close_dialog(cx);
                                        if let Some(intent) = intent {
                                            this.perform_leave(intent, window, cx);
                                        }
                                    });
                                }),
                        )
                        .child(
                            Button::new("unsaved-save")
                                .label(if saving { "Saving…" } else { "Save" })
                                .primary()
                                .disabled(saving)
                                .on_click(move |_, window, cx| {
                                    let _ = save.update(cx, |this, cx| {
                                        this.save_before_leaving(window, cx)
                                    });
                                }),
                        ),
                )
        });
    }

    fn save_before_leaving(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
        self.save(window, cx);
    }

    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_prompt_open {
            return;
        }
        let Some(session) = self.reader.read(cx).session() else {
            return;
        };
        let active = session.read(cx).active_save_ticket();
        if let Some(ticket) = active {
            if self.pending_leave.is_some() {
                self.observe_save(session, ticket, false, window, cx);
            }
            return;
        }
        if !session.read(cx).is_dirty() {
            if let Some(intent) = self.pending_leave.take() {
                self.request_leave(intent, window, cx);
            }
            return;
        }
        match session.update(cx, |session, cx| session.start_save(cx)) {
            Ok(ticket) => self.observe_save(session, ticket, true, window, cx),
            Err(error) => self.save_failed(error, window, cx),
        }
    }

    pub(super) fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_prompt_open || self.reader.read(cx).is_saving(cx) {
            return;
        }
        let Some(source) = self.reader.read(cx).path().map(ToOwned::to_owned) else {
            return;
        };
        let Some(session) = self.reader.read(cx).session() else {
            return;
        };
        if let Some(reason) = session.read(cx).editing_rejection() {
            window.push_notification(Notification::warning(reason.to_owned()), cx);
            self.cancel_pending_leave(cx);
            return;
        }
        let generation = self.reader.read(cx).document_generation();
        self.save_prompt_open = true;
        let suggested = format!(
            "{}-annotated.pdf",
            source.file_stem().unwrap_or_default().to_string_lossy()
        );
        let choice = cx.prompt_for_new_path(
            source.parent().unwrap_or(std::path::Path::new(".")),
            Some(&suggested),
        );
        self.save_picker = Some(cx.spawn(async move |this, cx| {
            let choice = match choice.await {
                Ok(Ok(Some(path))) => {
                    cx.background_spawn(async move {
                        let destination = Destination::inspect(path)?;
                        if destination.is_source(&source) {
                            return Err("Use Save to update the current PDF; Save As needs a different filename.".into());
                        }
                        Ok(Arc::new(destination))
                    }).await.map(Some)
                }
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update(cx, |this, cx| {
                this.save_picker = None;
                if this.reader.read(cx).document_generation() != generation
                    || this.reader.read(cx).session().as_ref() != Some(&session)
                {
                    this.cancel_pending_leave(cx);
                    return;
                }
                let handle = this.window;
                let owner = cx.weak_entity();
                cx.defer(move |cx| {
                    let _ = cx.update_window(handle, |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| match choice {
                            Ok(Some(destination)) => this.confirm_destination(destination, window, cx),
                            Ok(None) => this.cancel_pending_leave(cx),
                            Err(error) => this.save_failed(error, window, cx),
                        });
                    });
                });
            });
        }));
    }

    fn confirm_destination(
        &mut self,
        destination: Arc<Destination>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !destination.needs_overwrite() {
            self.start_save(destination, window, cx);
            return;
        }
        let owner = cx.weak_entity();
        let name = destination
            .path()
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        window.open_dialog(cx, move |dialog, _, _| {
            let replace = owner.clone();
            let cancel = owner.clone();
            let cancelled = owner.clone();
            let confirmed = owner.clone();
            let confirmed_destination = destination.clone();
            let destination = destination.clone();
            dialog.title(format!("Replace “{name}”?"))
                .child("The existing PDF will be replaced only if it has not changed since this confirmation.")
                .close_button(false).overlay_closable(false).entrance(DialogEntrance::None)
                .on_cancel(move |_, _, cx| {
                    let _ = cancelled.update(cx, |this, cx| this.cancel_pending_leave(cx));
                    true
                })
                .on_ok(move |_, window, cx| {
                    let _ = confirmed.update(cx, |this, cx| {
                        window.close_dialog(cx);
                        this.start_save(confirmed_destination.clone(), window, cx);
                    });
                    false
                })
                .footer(DialogFooter::new()
                    .child(Button::new("overwrite-cancel").label("Cancel").on_click(move |_, window, cx| {
                        let _ = cancel.update(cx, |this, cx| this.cancel_pending_leave(cx));
                        window.close_dialog(cx);
                    }))
                    .child(Button::new("overwrite-confirm").label("Replace").danger().on_click(move |_, window, cx| {
                        let _ = replace.update(cx, |this, cx| {
                            window.close_dialog(cx);
                            this.start_save(destination.clone(), window, cx);
                        });
                    })))
        });
    }

    fn start_save(
        &mut self,
        destination: Arc<Destination>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_prompt_open = false;
        let Some(session) = self.reader.read(cx).session() else {
            return;
        };
        let ticket = match session.update(cx, |session, cx| session.start_save_as(destination, cx))
        {
            Ok(ticket) => ticket,
            Err(error) => {
                self.save_failed(error, window, cx);
                return;
            }
        };
        self.observe_save(session, ticket, false, window, cx);
    }

    fn observe_save(
        &mut self,
        session: Entity<DocumentSession>,
        ticket: u64,
        current: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self.reader.read(cx).document_generation();
        self.save_observation =
            Some(
                cx.observe_in(&session, window, move |this, session, window, cx| {
                    if this.reader.read(cx).document_generation() != generation
                        || this.reader.read(cx).session().as_ref() != Some(&session)
                    {
                        this.save_observation = None;
                        return;
                    }
                    let Some((finished, result)) = session.read(cx).save_outcome() else {
                        return;
                    };
                    if finished != ticket {
                        return;
                    }
                    this.save_observation = None;
                    match result {
                        Ok(()) => {
                            if let Some(intent) = this.pending_leave.take() {
                                this.request_leave(intent, window, cx);
                            }
                        }
                        Err(SaveError::Readonly(_)) if current => this.save_as(window, cx),
                        Err(SaveError::Conflict(message)) if current => {
                            this.cancel_pending_leave(cx);
                            this.offer_save_copy(message, window, cx);
                        }
                        Err(error) => this.save_failed(error.to_string(), window, cx),
                    }
                }),
            );
    }

    fn offer_save_copy(&mut self, message: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_prompt_open = true;
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let cancelled = owner.clone();
            let cancel = owner.clone();
            let save = owner.clone();
            let confirmed = owner.clone();
            dialog
                .title("PDF changed outside Kver")
                .child(message.clone())
                .close_button(false)
                .overlay_closable(false)
                .entrance(DialogEntrance::None)
                .on_cancel(move |_, _, cx| {
                    let _ = cancelled.update(cx, |this, cx| this.cancel_pending_leave(cx));
                    true
                })
                .on_ok(move |_, window, cx| {
                    let _ =
                        confirmed.update(cx, |this, cx| this.save_copy_from_recovery(window, cx));
                    false
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("save-conflict-cancel")
                                .label("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ =
                                        cancel.update(cx, |this, cx| this.cancel_pending_leave(cx));
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("save-conflict-copy")
                                .label("Save As…")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    let _ = save.update(cx, |this, cx| {
                                        this.save_copy_from_recovery(window, cx)
                                    });
                                }),
                        ),
                )
        });
    }

    fn save_copy_from_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
        self.save_prompt_open = false;
        self.save_as(window, cx);
    }

    fn save_failed(&mut self, error: String, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_pending_leave(cx);
        window.push_notification(
            Notification::error(format!(
                "Couldn’t save PDF. {error} Your edits remain available; use Save As… to save elsewhere."
            )),
            cx,
        );
        cx.notify();
    }
}
