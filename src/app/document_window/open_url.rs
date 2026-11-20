use std::cell::Cell;

use super::*;
use crate::app::remote_pdf::{self, DownloadError};
use gpui_kit::base::actions::{Cancel, Confirm};
use gpui_kit::component::{
    button::{Button, ButtonVariants as _},
    dialog::{DialogEntrance, DialogFooter},
    input::{Input, InputState},
};

impl DocumentWindow {
    pub(crate) fn window_handle(&self) -> AnyWindowHandle {
        self.window
    }

    pub(crate) fn is_empty(&self, cx: &App) -> bool {
        !self.remote_pending && self.reader.read(cx).path().is_none()
    }

    pub(crate) fn prompt_url(&self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx
            .new(|cx| InputState::new(window, cx).placeholder("https://example.org/document.pdf"));
        let owner = cx.weak_entity();
        let focus = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if focus.replace(false) {
                input.focus_handle(cx).focus(window, cx);
            }
            let field = input.clone();
            let owner = owner.clone();
            let closed = owner.clone();
            dialog
                .title("Open URL")
                .entrance(DialogEntrance::None)
                .centered()
                .child("PDF URL")
                .child(Input::new(&input))
                .on_close(move |_, window, cx| {
                    if let Some(owner) = closed.upgrade() {
                        owner
                            .read(cx)
                            .reader
                            .read(cx)
                            .focus_handle(cx)
                            .focus(window, cx);
                    }
                })
                .on_ok(
                    move |_, window, cx| match remote_pdf::parse_url(&field.read(cx).value()) {
                        Ok(url) => {
                            let _ = owner.update(cx, |this, cx| {
                                this.open_remote_pdf(url, windows::OpenMode::Default, false, cx)
                            });
                            true
                        }
                        Err(error) => {
                            window.push_notification(Notification::warning(error), cx);
                            false
                        }
                    },
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("url-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(Cancel), cx)
                                }),
                        )
                        .child(Button::new("url-open").label("Open").primary().on_click(
                            |_, window, cx| {
                                window.dispatch_action(Box::new(Confirm { secondary: false }), cx)
                            },
                        )),
                )
        });
    }

    pub(crate) fn open_remote_pdf(
        &mut self,
        url: url::Url,
        mode: windows::OpenMode,
        browser_fallback: bool,
        cx: &mut Context<Self>,
    ) {
        let reader = self.reader.clone();
        let generation = reader.read(cx).document_generation();
        let handle = self.window;
        let reuse = mode.resolve(cx) == crate::settings::OpenFiles::Reuse;
        self.remote_pending = true;
        self.linked_open_request = self.linked_open_request.wrapping_add(1);
        let request = self.linked_open_request;
        let target = url.clone();
        cx.defer(move |cx| {
            let _ = cx.update_window(handle, |_, window, cx| {
                window.activate_window();
                window.push_notification(
                    Notification::info("Opening PDF from URL…").id::<OpenUrl>(),
                    cx,
                );
            });
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { remote_pdf::download(target) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if request != this.linked_open_request {
                    return;
                }
                this.remote_pending = false;
                let _ = cx.update_window(handle, |_, window, cx| {
                    window.remove_notification::<OpenUrl>(cx)
                });
                if reader != this.reader || generation != reader.read(cx).document_generation() {
                    return;
                }
                let error = match result {
                    Ok(document) => {
                        let session =
                            crate::reader::document_session::DocumentSession::preview(document, cx);
                        let source = DocumentSource::Session(session);
                        if reuse {
                            this.open_source(source, cx);
                            return;
                        }
                        windows::open_source(cx, Some(source), None).err()
                    }
                    Err(DownloadError::NotPdf) if browser_fallback => {
                        cx.open_url(url.as_str());
                        return;
                    }
                    Err(DownloadError::NotPdf) => Some("This URL does not return a PDF.".into()),
                    Err(DownloadError::Failed(error)) => Some(error),
                };
                if let Some(error) = error {
                    let _ = cx.update_window(handle, |_, window, cx| {
                        window.push_notification(Notification::warning(error), cx);
                    });
                }
            });
        })
        .detach();
    }
}
