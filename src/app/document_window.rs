//! Per-window composition and native file-open routing.
//!
//! The reader owns the active document and its load/restoration lifecycle.
//! This owner mounts it once and handles commands shared by all focused regions.

mod chrome;
pub mod command_palette;
mod jumplist;
mod saving;
pub mod sidebar;

use std::path::PathBuf;

use gpui_kit::{
    Action, AnyWindowHandle, App, Context, Entity, Focusable as _, Subscription, Task, Window,
    base::TestSupportExt as _,
    component::{ActiveTheme, WindowExt as _, notification::Notification, theme::Theme},
    div,
    prelude::*,
};

use super::{
    menu,
    windows::{self, CloseWindow, NextWindow, PreviousWindow},
};
use crate::{
    reader::{
        Continuous, Copy, DeleteLocalMark, DocumentEvent, FitPage, FitWidth, JumpEvent,
        JumpLocalMark, MarksView, NextMatch, NextPage, PreviousMatch, PreviousPage, ReaderView,
        ScrollDown, ScrollScreenDown, ScrollScreenUp, ScrollUp, SetLocalMark, ShowAnnotations,
        ShowContents, ShowDeleteMark, ShowFind, ShowJumpMark, ShowMarks, ShowSetMark, SinglePage,
        ToggleSidebar, UseDefaultReadingMode, ZoomIn, ZoomOut,
    },
    reading_history::{
        ShowRecent,
        recent_picker::{RecentPicker, RecentPickerEvent},
    },
    settings::Appearance,
    settings_ui,
};
use command_palette::{
    CommandAvailability, CommandPalette, DismissCommandPalette, ShowCommandPalette,
};
use jumplist::Jumplist;
pub(crate) use jumplist::{NavigateBack, NavigateForward};
use saving::Leave;
pub use saving::{Save, SaveAs};
use sidebar::{ReaderSidebar, SidebarPanel};

gpui_kit::actions!(kver, [OpenPdf]);

pub struct DocumentWindow {
    window: AnyWindowHandle,
    reader: Entity<ReaderView>,
    jumplist: Jumplist,
    file_picker: Option<Task<()>>,
    recent: Option<Entity<RecentPicker>>,
    recent_subscription: Option<Subscription>,
    recent_open: Option<PendingRecentOpen>,
    linked_open_request: u64,
    initial_link_page: Option<usize>,
    palette: Option<CommandPalette>,
    sidebar: Option<Entity<ReaderSidebar>>,
    panel_subscriptions: Vec<Subscription>,
    annotation_session: Option<Subscription>,
    save_picker: Option<Task<()>>,
    save_prompt_open: bool,
    save_observation: Option<Subscription>,
    pending_leave: Option<Leave>,
    _reader_observation: Subscription,
    _reader_focus_subscription: Subscription,
    _settings_observation: Subscription,
    _document_subscription: Subscription,
    _jump_subscription: Subscription,
    _appearance_subscription: Subscription,
    _activation_subscription: Subscription,
}

struct PendingRecentOpen {
    generation: u64,
    path: PathBuf,
    picker: gpui_kit::WeakEntity<RecentPicker>,
    request: u64,
}

struct LinkedOpen {
    request: u64,
    generation: u64,
    path: PathBuf,
    page: usize,
    source_window: AnyWindowHandle,
}

impl DocumentWindow {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_page(path, None, window, cx)
    }

    pub(crate) fn new_with_page(
        path: Option<PathBuf>,
        page: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let appearance_subscription = cx.observe_window_appearance(window, |_, window, cx| {
            if settings_ui::state(cx).read(cx).appearance() == Appearance::System {
                Theme::sync_system_appearance(Some(window), cx);
            }
            cx.notify();
        });
        let activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            this.refresh_navigation_menu(window, cx);
        });
        let reader = cx.new(|cx| ReaderView::new(path, window.scale_factor(), cx));
        chrome::sync_window_identity(reader.read(cx), window, cx);
        let owner = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = owner.update(cx, |this: &mut Self, cx| {
                this.request_leave(Leave::Close, window, cx)
            });
            false
        });
        let owner = cx.weak_entity();
        windows::register_document(window.window_handle(), owner, cx);
        let reader_focus_subscription = cx.on_focus(
            &reader.read(cx).focus_handle(cx),
            window,
            |this, window, cx| {
                this.refresh_navigation_menu(window, cx);
                // A document-owned modal restores focus just before it unmounts.
                // Re-evaluate after its dismissal so its native menu item is
                // not left disabled for the rest of the reader session.
                let owner = cx.weak_entity();
                window.on_next_frame(move |window, cx| {
                    let _ = owner.update(cx, |this, cx| this.refresh_navigation_menu(window, cx));
                });
            },
        );
        let document_subscription =
            cx.subscribe_in(&reader, window, |this, _, event, window, cx| {
                this.on_document_event(event, window, cx);
            });
        let jump_subscription =
            cx.subscribe_in(&reader, window, |this, _, jump: &JumpEvent, window, cx| {
                if this.reader.read(cx).jump_location().as_ref() == Some(&jump.to) {
                    this.jumplist.record(jump);
                    this.refresh_navigation_menu(window, cx);
                    cx.notify();
                }
            });
        let reader_observation = cx.observe_in(&reader, window, |this, reader, window, cx| {
            chrome::sync_window_identity(reader.read(cx), window, cx);
            this.refresh_navigation_menu(window, cx);
            cx.notify();
        });
        let settings = settings_ui::state(cx);
        let settings_observation = cx.observe_in(&settings, window, |this, _, window, cx| {
            this.schedule_refit(window, cx);
        });
        Self {
            window: window.window_handle(),
            reader,
            jumplist: Jumplist::default(),
            file_picker: None,
            recent: None,
            recent_subscription: None,
            recent_open: None,
            linked_open_request: 0,
            initial_link_page: page,
            palette: None,
            sidebar: None,
            panel_subscriptions: Vec::new(),
            annotation_session: None,
            save_picker: None,
            save_prompt_open: false,
            save_observation: None,
            pending_leave: None,
            _reader_observation: reader_observation,
            _reader_focus_subscription: reader_focus_subscription,
            _settings_observation: settings_observation,
            _document_subscription: document_subscription,
            _jump_subscription: jump_subscription,
            _appearance_subscription: appearance_subscription,
            _activation_subscription: activation_subscription,
        }
    }

    /// The window owns the reader; callers use its existing navigation and
    /// diagnostic interface rather than a second copy of document state.
    pub fn reader(&self) -> &Entity<ReaderView> {
        &self.reader
    }

    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    pub fn palette_query(&self, cx: &App) -> Option<gpui_kit::SharedString> {
        self.palette.as_ref().map(|palette| palette.query(cx))
    }

    fn show_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() || self.recent.is_some() {
            return;
        }
        self.reader.update(cx, |reader, cx| reader.hide_marks(cx));
        self.palette = Some(CommandPalette::new(window, cx));
        self.refresh_navigation_menu(window, cx);
        cx.notify();
    }

    fn dismiss_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The selected action may already have replaced the palette with marks
        // or Recent before Command's deferred confirmation runs.
        if self.palette.take().is_some() {
            self.restore_focus(window, cx);
            self.refresh_navigation_menu(window, cx);
            cx.notify();
        }
    }

    fn restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sidebar) = &self.sidebar
            && let Some(panel) = sidebar.read(cx).active()
        {
            sidebar.update(cx, |sidebar, cx| sidebar.open(panel, window, cx));
        } else {
            window.focus(&self.reader.read(cx).focus_handle(cx), cx);
        }
    }

    fn schedule_refit(&self, window: &mut Window, cx: &mut Context<Self>) {
        let reader = self.reader.downgrade();
        window.on_next_frame(move |_, cx| {
            let _ = reader.update(cx, |reader, cx| reader.sync_viewport(cx));
        });
        cx.notify();
    }

    pub fn sidebar_panel(&self, cx: &App) -> Option<SidebarPanel> {
        self.sidebar
            .as_ref()
            .and_then(|sidebar| sidebar.read(cx).active())
    }

    fn show_sidebar(&mut self, panel: SidebarPanel, window: &mut Window, cx: &mut Context<Self>) {
        if self.reader.read(cx).page_count().is_none() {
            return;
        }
        if self.sidebar.is_none() {
            let sidebar = cx.new(|cx| ReaderSidebar::new(window, cx));
            let contents = sidebar.read(cx).contents().clone();
            let find = sidebar.read(cx).find().clone();
            let annotations = sidebar.read(cx).annotations().clone();
            let query_focus = find.read(cx).query_focus(cx);
            self.panel_subscriptions = vec![
                cx.on_focus(&query_focus, window, |this, window, cx| {
                    this.refresh_navigation_menu(window, cx);
                }),
                cx.on_blur(&query_focus, window, |this, window, cx| {
                    this.refresh_navigation_menu(window, cx);
                }),
                cx.observe_in(&sidebar, window, |this, _, window, cx| {
                    this.schedule_refit(window, cx)
                }),
                cx.observe(&find, |this, find, cx| {
                    let find = find.read(cx);
                    let generation = find.generation();
                    let highlight = find.highlight().map(|(hit, pulse)| (hit.clone(), pulse));
                    this.reader.update(cx, |reader, cx| {
                        reader.set_find_highlight(generation, highlight, cx)
                    });
                    cx.notify();
                }),
                cx.subscribe(&find, |this, find, navigation, cx| {
                    if navigation.document_generation()
                        == this.reader.read(cx).document_generation()
                        && find.read(cx).is_current(navigation)
                    {
                        this.reader
                            .update(cx, |reader, cx| reader.navigate_find(navigation.hit(), cx));
                    }
                }),
                cx.subscribe(&contents, |this, _, navigation, cx| {
                    if navigation.generation() == this.reader.read(cx).document_generation() {
                        this.reader.update(cx, |reader, cx| {
                            reader.navigate_outline(navigation.destination(), cx)
                        });
                    }
                }),
                cx.subscribe(&annotations, |this, _, navigation, cx| {
                    this.reader.update(cx, |reader, cx| {
                        reader.navigate_annotation(
                            navigation.generation,
                            navigation.revision,
                            &navigation.annotation,
                            cx,
                        );
                    });
                }),
            ];
            self.sidebar = Some(sidebar);
            self.sync_sidebar_document(window, cx);
        }
        if let Some(sidebar) = &self.sidebar {
            sidebar.update(cx, |sidebar, cx| sidebar.open(panel, window, cx));
        }
        if panel == SidebarPanel::Annotations {
            self.load_annotations(window, cx);
        }
        self.refresh_navigation_menu(window, cx);
    }

    fn load_annotations(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.reader.read(cx).session() {
            session.update(cx, |session, cx| session.ensure_annotations(cx));
            self.sync_annotations(window, cx);
        }
    }

    fn sync_annotations(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sidebar) = &self.sidebar else {
            return;
        };
        let Some(session) = self.reader.read(cx).session() else {
            return;
        };
        let session = session.read(cx);
        let revision = session.revision();
        let catalog = session.annotations();
        sidebar
            .read(cx)
            .annotations()
            .clone()
            .update(cx, |panel, cx| {
                panel.set_catalog(revision, catalog, window, cx)
            });
    }

    fn sync_sidebar_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sidebar) = &self.sidebar else { return };
        let reader = self.reader.read(cx);
        let generation = reader.document_generation();
        let count = reader.page_count().unwrap_or(0);
        let document = reader.document();
        let session = reader.session();
        sidebar.update(cx, |sidebar, cx| {
            sidebar.contents().update(cx, |contents, cx| {
                contents.set_document(generation, document.clone(), cx);
                if sidebar.active() == Some(SidebarPanel::Contents) {
                    contents.ensure_loaded(cx);
                }
            });
            sidebar.find().update(cx, |find, cx| {
                find.set_document(generation, document.clone(), count, cx)
            });
            sidebar.annotations().update(cx, |panel, cx| {
                panel.set_document(generation, document, window, cx);
            });
        });
        self.annotation_session = session.map(|session| {
            cx.observe_in(&session, window, |this, _, window, cx| {
                this.sync_annotations(window, cx);
            })
        });
        if self.sidebar_panel(cx) == Some(SidebarPanel::Annotations) {
            self.load_annotations(window, cx);
        }
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_panel(cx).is_some() {
            let focus = self.reader.read(cx).focus_handle(cx);
            if let Some(sidebar) = &self.sidebar {
                sidebar.update(cx, |sidebar, cx| sidebar.close(window, &focus, cx));
            }
        } else {
            let panel = self
                .sidebar
                .as_ref()
                .map_or(SidebarPanel::default(), |sidebar| sidebar.read(cx).last());
            self.show_sidebar(panel, window, cx);
        }
    }

    fn step_match(&mut self, direction: i32, cx: &mut Context<Self>) {
        if let Some(sidebar) = &self.sidebar {
            let find = sidebar.read(cx).find().clone();
            find.update(cx, |find, cx| find.step_match(direction, cx));
        }
    }

    fn has_matches(&self, cx: &App) -> bool {
        self.sidebar
            .as_ref()
            .is_some_and(|sidebar| sidebar.read(cx).find().read(cx).has_matches())
    }

    fn refresh_navigation_menu(&self, window: &Window, cx: &App) {
        if cx.active_window() != Some(window.window_handle()) {
            return;
        }
        let find_has_focus = self.sidebar.as_ref().is_some_and(|sidebar| {
            sidebar
                .read(cx)
                .find()
                .read(cx)
                .query_focus(cx)
                .is_focused(window)
        });
        let marks_open = self
            .reader
            .read(cx)
            .marks_overlay()
            .is_some_and(|marks| marks.read(cx).is_open());
        let accepts_shortcut =
            !find_has_focus && self.palette.is_none() && self.recent.is_none() && !marks_open;
        menu::set_authoring_availability(
            cx,
            accepts_shortcut && self.reader.read(cx).highlight_available(cx),
            self.reader.read(cx).save_available(cx),
            self.reader.read(cx).save_as_available(cx),
        );
        menu::refresh_navigation(
            cx,
            accepts_shortcut && self.jumplist.can_back(),
            accepts_shortcut && self.jumplist.can_forward(),
        );
    }

    fn show_marks(&mut self, view: MarksView, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_some() {
            return;
        }
        if self
            .reader
            .update(cx, |reader, cx| reader.show_marks(view, window, cx))
        {
            self.palette = None;
            self.refresh_navigation_menu(window, cx);
            cx.notify();
        }
    }

    /// Sibling overlays use the reader's existing Action handlers without
    /// moving focus or duplicating navigation/availability policy here.
    fn route_reader_action<A: Action>(
        &mut self,
        action: &A,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reader
            .read(cx)
            .focus_handle(cx)
            .dispatch_action(action, window, cx);
    }

    fn traverse_jumplist(&mut self, direction: i32, window: &mut Window, cx: &mut Context<Self>) {
        self.jumplist.traverse(direction, |location| {
            self.reader
                .update(cx, |reader, cx| reader.restore_jump(location, cx))
        });
        self.refresh_navigation_menu(window, cx);
        cx.notify();
    }

    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.reader.read(cx).is_dirty(cx) || self.reader.read(cx).is_saving(cx) {
            let owner = cx.weak_entity();
            let window = self.window;
            cx.defer(move |cx| {
                let _ = cx.update_window(window, |_, window, cx| {
                    let _ = owner.update(cx, |this, cx| {
                        this.request_leave(Leave::Replace(path), window, cx)
                    });
                });
            });
            return;
        }
        self.replace_path(path, cx);
    }

    fn replace_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.file_picker = None;
        self.save_picker = None;
        self.save_prompt_open = false;
        self.save_observation = None;
        self.initial_link_page = None;
        let target = path.clone();
        self.reader
            .update(cx, |reader, cx| reader.open_path(path, cx));
        if let Some(pending) = &mut self.recent_open
            && pending.path == target
        {
            pending.generation = self.reader.read(cx).document_generation();
        }
    }

    fn open_linked_document(
        &mut self,
        generation: u64,
        path: PathBuf,
        page: usize,
        source_window: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if self.reader.read(cx).document_generation() != generation {
            return;
        }
        self.linked_open_request = self.linked_open_request.wrapping_add(1);
        let request = self.linked_open_request;
        let check = path.clone();
        let open = LinkedOpen {
            request,
            generation,
            path,
            page,
            source_window,
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let pages = crate::pdf::document_pages(&check)?;
                    if page >= pages.len() {
                        return Err(format!(
                            "PDF link page {} is outside the target document",
                            page + 1
                        ));
                    }
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.finish_linked_open(open, result, cx);
            });
        })
        .detach();
    }

    fn finish_linked_open(
        &mut self,
        open: LinkedOpen,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if self.linked_open_request != open.request
            || self.reader.read(cx).document_generation() != open.generation
        {
            return;
        }
        // Future settings can supply this policy without changing PDF decoding
        // or the ReaderView's link activation contract.
        let replace_active = self.linked_pdf_replaces_active();
        let result = result.and_then(|()| {
            if replace_active {
                self.open_path(open.path.clone(), cx);
                self.initial_link_page = Some(open.page);
                Ok(())
            } else {
                windows::open_link(cx, open.path.clone(), open.page).map(|_| ())
            }
        });
        if let Err(error) = result {
            let message = format!("Cannot open linked PDF {}: {error}", open.path.display());
            self.reader.update(cx, |reader, cx| {
                reader.report_link_error(message.clone(), cx)
            });
            let _ = cx.update_window(open.source_window, |_, window, cx| {
                window.push_notification(Notification::warning(message), cx);
            });
        }
    }

    fn linked_pdf_replaces_active(&self) -> bool {
        false
    }

    pub fn open_dialog(&mut self, cx: &mut Context<Self>) {
        let generation = self.reader.read(cx).document_generation();
        let choice = rfd::AsyncFileDialog::new()
            .set_title("Open PDF")
            .add_filter("PDF documents", &["pdf"])
            .pick_file();
        self.file_picker = Some(cx.spawn(async move |this, cx| {
            let path = choice.await.map(|file| file.path().to_path_buf());
            let _ = this.update(cx, |this, cx| {
                this.finish_file_choice(generation, path, cx);
            });
        }));
    }

    fn finish_file_choice(
        &mut self,
        generation: u64,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        // Recent and platform opens share the reader's document identity.
        // Zoom/raster invalidation is not a new document and must not discard
        // a valid picker result.
        if self.reader.read(cx).document_generation() != generation {
            return;
        }
        if let Some(path) = path {
            self.open_path(path, cx);
        }
    }

    fn show_recent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recent.is_some() {
            return;
        }
        self.palette = None;
        let excluded = self.reader.read(cx).record_path().map(ToOwned::to_owned);
        let picker = cx.new(|cx| RecentPicker::new(excluded, window, cx));
        let recent_subscription =
            cx.subscribe_in(&picker, window, |this, picker, event, window, cx| {
                if this.recent.as_ref() != Some(picker) {
                    return;
                }
                match event {
                    RecentPickerEvent::Open { path, request } => {
                        this.open_path(path.clone(), cx);
                        this.recent_open = Some(PendingRecentOpen {
                            generation: this.reader.read(cx).document_generation(),
                            path: path.clone(),
                            picker: picker.downgrade(),
                            request: *request,
                        });
                    }
                    RecentPickerEvent::Closed => {
                        this.recent = None;
                        this.recent_subscription = None;
                    }
                }
                this.refresh_navigation_menu(window, cx);
                cx.notify();
            });
        self.recent_subscription = Some(recent_subscription);
        self.recent = Some(picker.clone());
        picker.update(cx, |picker, cx| picker.show(window, cx));
        self.refresh_navigation_menu(window, cx);
        cx.notify();
    }

    fn on_document_event(
        &mut self,
        event: &DocumentEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let DocumentEvent::OpenLinkedDocument {
            generation,
            path,
            page,
        } = event
        {
            self.open_linked_document(*generation, path.clone(), *page, window.window_handle(), cx);
            return;
        }
        let generation = match event {
            DocumentEvent::Replaced { generation }
            | DocumentEvent::Opened { generation, .. }
            | DocumentEvent::Saved { generation } => *generation,
            DocumentEvent::OpenLinkedDocument { .. } => unreachable!(),
        };
        if generation != self.reader.read(cx).document_generation() {
            return;
        }
        if matches!(event, DocumentEvent::Replaced { .. }) {
            self.jumplist.clear();
            self.initial_link_page = None;
            self.refresh_navigation_menu(window, cx);
            cx.notify();
        }
        if let DocumentEvent::Opened { result: Ok(()), .. } = event
            && let Some(page) = self.initial_link_page.take()
        {
            self.reader
                .update(cx, |reader, cx| reader.select_page(page, cx));
        }
        self.sync_sidebar_document(window, cx);
        if let DocumentEvent::Opened {
            result: Err(error), ..
        } = event
            && let Some(sidebar) = &self.sidebar
        {
            sidebar
                .read(cx)
                .annotations()
                .clone()
                .update(cx, |panel, cx| panel.document_failed(error, cx));
        }
        if self
            .recent_open
            .as_ref()
            .is_some_and(|pending| pending.generation != generation)
            && let Some(pending) = self.recent_open.take()
        {
            let _ = pending
                .picker
                .update(cx, |picker, cx| picker.cancel_open(pending.request, cx));
        }
        if let Some(picker) = &self.recent {
            let excluded = self.reader.read(cx).record_path().map(ToOwned::to_owned);
            picker.update(cx, |picker, cx| picker.set_active_path(excluded, cx));
        }
        if let DocumentEvent::Opened { result, .. } = event
            && let Some(pending) = self.recent_open.take()
        {
            // Reply to the initiating picker, never to whichever modal happens
            // to be open now. A reopened dialog has a different entity identity.
            let _ = pending.picker.update(cx, |picker, cx| {
                picker.finish_open(pending.request, result.clone(), window, cx);
            });
        }
    }
}

impl Render for DocumentWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = settings_ui::state(cx);
        let reader = self.reader.read(cx);
        let title = chrome::title_bar(reader.path(), reader.is_dirty(cx), window, cx);
        let marks = reader.marks_overlay();
        let toolbar = settings.read(cx).show_toolbar().then(|| {
            let sidebar_rems = self
                .sidebar
                .as_ref()
                .map_or(0., |sidebar| sidebar.read(cx).width_rems(window));
            chrome::reading_toolbar(
                reader,
                self.sidebar_panel(cx).is_some(),
                self.jumplist.can_back(),
                self.jumplist.can_forward(),
                f32::from(window.bounds().size.width) / f32::from(window.rem_size()) - sidebar_rems,
                cx,
            )
        });
        let statusbar = settings
            .read(cx)
            .show_statusbar()
            .then(|| chrome::status_bar(reader, cx));
        div()
            .id("document-window")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &OpenPdf, _, cx| this.open_dialog(cx)))
            .on_action(cx.listener(|this, _: &ShowRecent, window, cx| this.show_recent(window, cx)))
            .on_action(
                cx.listener(|this, _: &ShowCommandPalette, window, cx| {
                    this.show_palette(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &DismissCommandPalette, window, cx| {
                this.dismiss_palette(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowMarks, window, cx| {
                this.show_marks(MarksView::List, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSetMark, window, cx| {
                this.show_marks(MarksView::Set, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowJumpMark, window, cx| {
                this.show_marks(MarksView::Jump, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowDeleteMark, window, cx| {
                this.show_marks(MarksView::Delete, window, cx)
            }))
            .on_action(cx.listener(Self::route_reader_action::<NextPage>))
            .on_action(cx.listener(Self::route_reader_action::<PreviousPage>))
            .on_action(cx.listener(Self::route_reader_action::<ScrollDown>))
            .on_action(cx.listener(Self::route_reader_action::<ScrollUp>))
            .on_action(cx.listener(Self::route_reader_action::<ScrollScreenDown>))
            .on_action(cx.listener(Self::route_reader_action::<ScrollScreenUp>))
            .on_action(cx.listener(Self::route_reader_action::<ZoomIn>))
            .on_action(cx.listener(Self::route_reader_action::<ZoomOut>))
            .on_action(cx.listener(Self::route_reader_action::<FitWidth>))
            .on_action(cx.listener(Self::route_reader_action::<FitPage>))
            .on_action(cx.listener(Self::route_reader_action::<SinglePage>))
            .on_action(cx.listener(Self::route_reader_action::<Continuous>))
            .on_action(cx.listener(Self::route_reader_action::<UseDefaultReadingMode>))
            .on_action(cx.listener(Self::route_reader_action::<Copy>))
            .on_action(cx.listener(Self::route_reader_action::<crate::reader::Highlight>))
            .on_action(cx.listener(|this, _: &ShowContents, window, cx| {
                this.show_sidebar(SidebarPanel::Contents, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowFind, window, cx| {
                this.show_sidebar(SidebarPanel::Find, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowAnnotations, window, cx| {
                this.show_sidebar(SidebarPanel::Annotations, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, window, cx| this.toggle_sidebar(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NextMatch, _, cx| this.step_match(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousMatch, _, cx| this.step_match(-1, cx)))
            .on_action(cx.listener(Self::route_reader_action::<SetLocalMark>))
            .on_action(cx.listener(Self::route_reader_action::<JumpLocalMark>))
            .on_action(cx.listener(Self::route_reader_action::<DeleteLocalMark>))
            .on_action(cx.listener(|this, _: &NavigateBack, window, cx| {
                this.traverse_jumplist(-1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NavigateForward, window, cx| {
                this.traverse_jumplist(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                this.request_leave(Leave::Close, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save_as(window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(window, cx)))
            .on_action(|_: &NextWindow, window, cx| windows::cycle(window, cx, 1))
            .on_action(|_: &PreviousWindow, window, cx| windows::cycle(window, cx, -1))
            .child(title)
            .child(
                div()
                    .id("reader-workspace")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .items_stretch()
                    .children(
                        self.sidebar
                            .as_ref()
                            .filter(|sidebar| sidebar.read(cx).active().is_some())
                            .cloned(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .children(toolbar)
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .child(self.reader.clone()),
                            )
                            .children(statusbar),
                    ),
            )
            .when_some(self.palette.clone(), |root, palette| {
                let reader = self.reader.read(cx);
                root.children(palette.render(
                    CommandAvailability {
                        page_ix: reader.page_index(),
                        page_count: reader.page_count().unwrap_or(0),
                        mode_override: reader.mode_override(),
                        has_selection: reader.selected_text().is_some(),
                        highlight_available: reader.highlight_available(cx),
                        save_available: reader.save_available(cx),
                        save_as_available: reader.save_as_available(cx),
                        has_matches: self.has_matches(cx),
                        can_back: self.jumplist.can_back(),
                        can_forward: self.jumplist.can_forward(),
                    },
                    cx,
                ))
            })
            .children(marks)
    }
}

#[cfg(test)]
mod tests;
