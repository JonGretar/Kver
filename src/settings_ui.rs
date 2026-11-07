//! One application-wide Settings window and its asynchronous preference state.
use std::{cell::Cell, time::Duration};

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Disableable as _, StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogEntrance,
    list::ListItem,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    scroll::ScrollableElement as _,
    switch::Switch,
    theme::{Theme, ThemeMode},
    tree::{TreeItem, TreeState, tree},
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Render, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    reader_windows::CloseWindow,
    recent,
    settings::{self, AppPaths, AppSettings, Appearance, InitialZoom, ReadingView, SettingsChange},
};

gpui::actions!(kver, [ShowSettings, CloseSettings]);

pub struct SettingsState {
    settings: AppSettings,
    paths: Option<AppPaths>,
    error: Option<String>,
    ready: bool,
    revision: u64,
    saving: bool,
    pending: Option<SettingsChange>,
    recording_requested: bool,
}

impl SettingsState {
    fn new(paths: Option<AppPaths>) -> Self {
        Self {
            settings: AppSettings::default(),
            ready: paths.is_none(),
            paths,
            error: None,
            revision: 0,
            saving: false,
            pending: None,
            recording_requested: true,
        }
    }

    pub fn appearance(&self) -> Appearance {
        self.settings.appearance()
    }

    pub fn show_toolbar(&self) -> bool {
        self.settings.show_toolbar()
    }

    pub fn show_statusbar(&self) -> bool {
        self.settings.show_statusbar()
    }

    pub fn remember_reading_history(&self) -> bool {
        self.settings.remember_reading_history()
    }

    pub fn reopen_at_last_position(&self) -> bool {
        self.settings.reopen_at_last_position()
    }

    pub fn quit_when_last_window_closes(&self) -> bool {
        self.settings.quit_when_last_window_closes()
    }

    pub fn initial_zoom(&self) -> InitialZoom {
        self.settings.initial_zoom()
    }

    pub fn reading_view(&self) -> ReadingView {
        self.settings.reading_view()
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn choose(&mut self, change: SettingsChange, cx: &mut Context<Self>) {
        let updated = match change {
            SettingsChange::Appearance(choice) => self.settings.with_appearance(choice),
            SettingsChange::ShowToolbar(show) => self.settings.with_show_toolbar(show),
            SettingsChange::ShowStatusbar(show) => self.settings.with_show_statusbar(show),
            SettingsChange::InitialZoom(choice) => self.settings.with_initial_zoom(choice),
            SettingsChange::ReadingView(choice) => self.settings.with_reading_view(choice),
            SettingsChange::ReopenLastWindowsOnStartup(reopen) => {
                self.settings.with_reopen_last_windows_on_startup(reopen)
            }
            SettingsChange::QuitWhenLastWindowCloses(quit) => {
                self.settings.with_quit_when_last_window_closes(quit)
            }
            SettingsChange::RememberReadingHistory(remember) => {
                self.settings.with_remember_reading_history(remember)
            }
            SettingsChange::ReopenAtLastPosition(reopen) => {
                self.settings.with_reopen_at_last_position(reopen)
            }
        };
        if self.settings == updated && self.pending.is_none() {
            return;
        }
        self.revision += 1;
        if self.paths.is_none() {
            self.apply_settings(updated, cx);
            self.error = Some(
                "Settings cannot be saved: no user configuration directory is available.".into(),
            );
        } else {
            self.pending = Some(change);
            self.start_save(cx);
        }
        cx.notify();
    }

    fn choose_history(&mut self, remember: bool, cx: &mut Context<Self>) {
        if self.remember_reading_history() == remember && self.pending.is_none() {
            return;
        }
        self.revision += 1;
        // Stop writes as soon as the reader requests privacy. Re-enabling
        // waits for the TOML save so a failed preference edit cannot leak.
        if !remember && let Some(history) = recent::history(cx) {
            self.recording_requested = false;
            let work = history.set_recording(remember);
            cx.spawn(async move |this, cx| {
                if let Err(error) = work.await {
                    let _ = this.update(cx, |this, cx| {
                        this.error = Some(format!("Reading history could not be updated: {error}"));
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        if self.paths.is_none() {
            self.apply_settings(self.settings.with_remember_reading_history(remember), cx);
            self.error = Some(
                "Settings cannot be saved: no user configuration directory is available.".into(),
            );
        } else {
            self.pending = Some(SettingsChange::RememberReadingHistory(remember));
            self.start_save(cx);
        }
        cx.notify();
    }

    fn apply_settings(&mut self, updated: AppSettings, cx: &mut Context<Self>) {
        if self.settings.appearance() != updated.appearance() {
            updated.appearance().apply(cx);
        }
        if self.recording_requested != updated.remember_reading_history()
            && let Some(history) = recent::history(cx)
        {
            self.recording_requested = updated.remember_reading_history();
            let work = history.set_recording(updated.remember_reading_history());
            cx.spawn(async move |this, cx| {
                if let Err(error) = work.await {
                    let _ = this.update(cx, |this, cx| {
                        this.error = Some(format!("Reading history could not be updated: {error}"));
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        self.settings = updated;
        cx.notify();
    }

    fn start_save(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(path) = self
            .paths
            .as_ref()
            .map(|paths| paths.config().to_path_buf())
        else {
            return;
        };
        let Some(change) = self.pending.take() else {
            return;
        };
        self.saving = true;
        let revision = self.revision;
        let work = cx
            .background_executor()
            .spawn(async move { settings::save(&path, change) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                this.ready = true;
                if revision != this.revision {
                    this.start_save(cx);
                } else {
                    match result {
                        Ok(updated) => {
                            this.apply_settings(updated, cx);
                            this.error = None;
                        }
                        Err(error) => {
                            if let Some(history) = recent::history(cx) {
                                this.recording_requested = this.remember_reading_history();
                                let rollback =
                                    history.set_recording(this.remember_reading_history());
                                cx.spawn(async move |_, _| {
                                    let _ = rollback.await;
                                })
                                .detach();
                            }
                            this.error =
                                Some(format!("{error}. Check the settings file and try again."));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn accept_file(&mut self, result: Result<AppSettings, String>, cx: &mut Context<Self>) {
        let became_ready = !self.ready;
        self.ready = true;
        if became_ready {
            cx.notify();
        }
        if self.saving || self.pending.is_some() {
            return;
        }
        match result {
            Ok(updated) => {
                if self.settings != updated {
                    self.apply_settings(updated, cx);
                }
                if self.error.take().is_some() {
                    cx.notify();
                }
            }
            Err(error) => {
                let message = format!("{error}. Correct the settings file to apply your changes.");
                if self.error.as_deref() != Some(&message) {
                    self.error = Some(message);
                    cx.notify();
                    if let Some(window) = cx.active_window() {
                        let _ = cx.update_window(window, |_, window, cx| {
                            window.push_notification(
                                Notification::warning("Settings file could not be applied. Correct the file; your last valid settings remain active."),
                                cx,
                            );
                        });
                    }
                }
            }
        }
    }
}

impl Appearance {
    fn apply(self, cx: &mut App) {
        match self {
            Self::System => Theme::sync_system_appearance(None, cx),
            Self::Light => Theme::change(ThemeMode::Light, None, cx),
            Self::Dark => Theme::change(ThemeMode::Dark, None, cx),
        }
    }
}

struct SharedSettings(Entity<SettingsState>);
impl gpui::Global for SharedSettings {}

pub fn state(cx: &mut App) -> Entity<SettingsState> {
    if !cx.has_global::<SharedSettings>() {
        let settings = cx.new(|_| SettingsState::new(None));
        cx.set_global(SharedSettings(settings));
    }
    cx.global::<SharedSettings>().0.clone()
}

/// Call once at startup; loading is off the UI thread and creates no file.
pub fn install(cx: &mut App) {
    install_with_paths(cx, AppPaths::resolve());
}

/// Explicit storage locations for isolated app integration tests.
pub fn install_with_paths(cx: &mut App, paths: Option<AppPaths>) {
    let settings = cx.new(|_| SettingsState::new(paths.clone()));
    cx.set_global(SharedSettings(settings.clone()));
    let open_window = Cell::<Option<AnyWindowHandle>>::new(None);
    cx.on_action(move |_: &ShowSettings, cx| {
        if let Some(handle) = open_window.get()
            && (cx.active_window() == Some(handle)
                || cx
                    .update_window(handle, |_, window, _| window.activate_window())
                    .is_ok())
        {
            return;
        }
        match open(cx) {
            Ok(handle) => open_window.set(Some(handle)),
            Err(error) => eprintln!("Cannot open Settings: {error}"),
        }
    });
    if let Some(paths) = paths {
        let path = paths.config().to_path_buf();
        cx.spawn(async move |cx| {
            loop {
                let revision = cx.update(|cx| settings.read(cx).revision);
                let path = path.clone();
                let result = cx
                    .background_spawn(async move { settings::load(&path) })
                    .await;
                cx.update(|cx| {
                    settings.update(cx, |settings, cx| {
                        if settings.revision == revision {
                            settings.accept_file(result, cx);
                        }
                    });
                });
                cx.background_executor()
                    .timer(Duration::from_millis(350))
                    .await;
            }
        })
        .detach();
    } else {
        settings.update(cx, |settings, cx| {
            settings.ready = true;
            settings.error = Some(
                "Settings cannot be loaded or saved: no user configuration directory is available."
                    .into(),
            );
            cx.notify();
        });
    }
}

fn open(cx: &mut App) -> gpui_kit::Result<AnyWindowHandle> {
    // Window bounds are platform pixels; controls and spacing use rem-based tiers.
    let bounds = Bounds::centered(None, size(px(700.), px(640.)), cx);
    let (handle, _) = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(500.), px(280.))),
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle().tab_stop(true);
                let choice_focus = cx.focus_handle().tab_stop(true);
                let zoom_focus = cx.focus_handle().tab_stop(true);
                focus.focus(window, cx);
                let settings = state(cx);
                cx.observe(&settings, |_, _, cx| cx.notify()).detach();
                let navigation = cx.new(|cx| {
                    let mut tree = TreeState::new(cx).items(vec![
                        TreeItem::new("application", "Application")
                            .expanded(true)
                            .children([
                                TreeItem::new("theme", "Theme"),
                                TreeItem::new("ui-elements", "UI Elements"),
                                TreeItem::new("windows", "Windows"),
                            ]),
                        TreeItem::new("reader", "Reader")
                            .expanded(true)
                            .child(TreeItem::new("reading-experience", "Reading Experience")),
                        TreeItem::new("persistence", "Persistence")
                            .expanded(true)
                            .child(TreeItem::new("reading-history", "Reading History")),
                    ]);
                    tree.set_selected_index(Some(0), cx);
                    tree
                });
                cx.observe(&navigation, |_, _, cx| cx.notify()).detach();
                SettingsView {
                    focus,
                    choice_focus,
                    zoom_focus,
                    navigation,
                    settings,
                    clearing: false,
                    clear_error: None,
                }
            })
        },
    )?;
    cx.update_window(handle, |_, window, _| window.activate_window())?;
    Ok(handle)
}

struct SettingsView {
    focus: FocusHandle,
    choice_focus: FocusHandle,
    zoom_focus: FocusHandle,
    navigation: Entity<TreeState>,
    settings: Entity<SettingsState>,
    clearing: bool,
    clear_error: Option<String>,
}

impl SettingsView {
    fn page(&self, cx: &App) -> SettingsPage {
        match self
            .navigation
            .read(cx)
            .selected_item()
            .map(|item| item.id.as_ref())
        {
            Some("reader" | "reading-experience") => SettingsPage::Reader,
            Some("persistence" | "reading-history") => SettingsPage::Persistence,
            _ => SettingsPage::Application,
        }
    }

    fn render_navigation(&self) -> impl IntoElement {
        let selection = self.navigation.clone();
        tree(&self.navigation, move |index, entry, _, _, _| {
            let selection = selection.clone();
            let disclosure = selection.clone();
            let expanded = entry.is_expanded();
            ListItem::new(entry.item().id.clone())
                .accessibility_label(entry.item().label.clone())
                .h_8()
                .text_xs()
                .pl(gpui::rems(entry.depth() as f32))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(if entry.is_folder() {
                            Button::new(format!("disclose-{}", entry.item().id))
                                .ghost()
                                .xsmall()
                                .icon(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .accessibility_label(format!(
                                    "{} {}",
                                    if expanded { "Collapse" } else { "Expand" },
                                    entry.item().label
                                ))
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    disclosure.update(cx, |tree, cx| {
                                        tree.set_selected_index(Some(index), cx);
                                        tree.focus(window, cx);
                                    });
                                    if expanded {
                                        window.dispatch_action(
                                            Box::new(gpui::base::actions::SelectLeft),
                                            cx,
                                        );
                                    } else {
                                        window.dispatch_action(
                                            Box::new(gpui::base::actions::SelectRight),
                                            cx,
                                        );
                                    }
                                })
                                .into_any_element()
                        } else {
                            div().w_5().flex_shrink_0().into_any_element()
                        })
                        .child(entry.item().label.clone()),
                )
                .on_click(move |_, window, cx| {
                    // Selecting a page must not also collapse its child groups.
                    // Tree retains arrow-key selection and disclosure behavior.
                    cx.stop_propagation();
                    selection.update(cx, |tree, cx| {
                        tree.set_selected_index(Some(index), cx);
                        tree.focus(window, cx);
                    });
                })
        })
    }

    fn clear_history(&mut self, cx: &mut Context<Self>) {
        if self.clearing {
            return;
        }
        let Some(history) = recent::history(cx) else {
            self.clear_error = Some("Local reading history is unavailable.".into());
            cx.notify();
            return;
        };
        self.clearing = true;
        self.clear_error = None;
        cx.notify();
        let work = history.clear();
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.clearing = false;
                match result {
                    Ok(()) => recent::notify_changed(cx),
                    Err(error) => {
                        this.clear_error = Some(format!("Could not clear reading history: {error}"))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsPage {
    Application,
    Reader,
    Persistence,
}

impl SettingsPage {
    fn label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::Reader => "Reader",
            Self::Persistence => "Persistence",
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page(cx);
        let settings = self.settings.read(cx);
        let selected = settings.appearance();
        let quit_on_close = settings.quit_when_last_window_closes();
        let initial_zoom = settings.initial_zoom();
        let error = settings.error.clone();
        let state = self.settings.clone();
        let close_state = self.settings.clone();
        let zoom_state = self.settings.clone();
        let reading_view = settings.reading_view();
        let view_state = self.settings.clone();
        let reopen = settings.settings.reopen_last_windows_on_startup();
        let reopen_state = self.settings.clone();
        let show_toolbar = settings.show_toolbar();
        let show_statusbar = settings.show_statusbar();
        let toolbar_state = self.settings.clone();
        let statusbar_state = self.settings.clone();
        let reset_state = self.settings.clone();
        let reset_focus = self.choice_focus.clone();
        let remember = settings.remember_reading_history();
        let history_state = self.settings.clone();
        let reopen_position = settings.reopen_at_last_position();
        let position_state = self.settings.clone();
        let clear_owner = cx.weak_entity();
        let clearing = self.clearing;
        let clear_error = self.clear_error.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .key_context("KverSettings")
            .on_action(|_: &CloseSettings, window, _| window.remove_window())
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .child(
                TitleBar::new().pl_0().relative().child(
                    div()
                        .id("settings-window-title")
                        .test_support()
                        .absolute()
                        .top_0()
                        .left_0()
                        // Center on the window, not the lane beside native controls.
                        .w(window.viewport_size().width)
                        .h(gpui_kit::component::TITLE_BAR_HEIGHT)
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_medium()
                        .child("Settings"),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(
                        div()
                            .id("settings-sidebar")
                            .test_support()
                            .role(gpui::Role::Navigation)
                            .aria_label("Settings pages")
                            .w_56()
                            .flex_shrink_0()
                            .h_full()
                            .p_2()
                            .bg(cx.theme().sidebar)
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(self.render_navigation()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .p_4()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .font_medium()
                                    .child(page.label()),
                            )
                            .children((page != SettingsPage::Persistence).then(||
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(
                                        div()
                                            .pb_2()
                                            .border_b_1()
                                            .border_color(cx.theme().border)
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(if page == SettingsPage::Application { "Theme" } else { "Reading Experience" }),
                                    )
                                    .children((page == SettingsPage::Application).then(||
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_4()
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(div().text_sm().font_medium().child("Appearance"))
                                                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Follow the system, or always use light or dark mode.")),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .when(selected != Appearance::System, |row| {
                                                        row.child(
                                                            Button::new("appearance-reset")
                                                                .small()
                                                                .ghost()
                                                                .icon(IconName::Undo2)
                                                                .accessibility_label("Reset appearance to System")
                                                                .tooltip("Reset appearance to System")
                                                                .on_click(move |_, window, cx| {
                                                                    reset_state.update(cx, |settings, cx| settings.choose(SettingsChange::Appearance(Appearance::System), cx));
                                                                    window.focus(&reset_focus, cx);
                                                                }),
                                                        )
                                                    })
                                                    .child(
                                                        Button::new("appearance-choice")
                                                            .small()
                                                            .outline()
                                                            .label(selected.label())
                                                            .accessibility_label(format!(
                                                                "Appearance: {}",
                                                                selected.label()
                                                            ))
                                                            .dropdown_caret(true)
                                                            .track_focus(&self.choice_focus)
                                                            .dropdown_menu_with_anchor(
                                                                gpui::Anchor::TopRight,
                                                                move |mut menu, _, _| {
                                                                    for choice in [
                                                                        Appearance::System,
                                                                        Appearance::Light,
                                                                        Appearance::Dark,
                                                                    ] {
                                                                        let state = state.clone();
                                                                        menu = menu.item(
                                                                            PopupMenuItem::new(choice.label())
                                                                                .checked(choice == selected)
                                                                                .on_click(move |_, _, cx| {
                                                                                    state.update(cx, |settings, cx| settings.choose(SettingsChange::Appearance(choice), cx));
                                                                                }),
                                                                        );
                                                                    }
                                                                    menu
                                                                },
                                                            ),
                                                    ),
                                            ),
                                    ))
                                    .children((page == SettingsPage::Reader).then(||
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap_4()
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(div().text_sm().font_medium().child("Initial Zoom"))
                                                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Set how newly opened PDFs fit the window. Open readers keep their zoom.")),
                                            )
                                            .child(
                                                Button::new("initial-zoom-choice")
                                                    .small()
                                                    .outline()
                                                    .label(initial_zoom.label())
                                                    .accessibility_label(format!("Initial Zoom: {}", initial_zoom.label()))
                                                    .dropdown_caret(true)
                                                    .track_focus(&self.zoom_focus)
                                                    .dropdown_menu_with_anchor(
                                                        gpui::Anchor::TopRight,
                                                        move |mut menu, _, _| {
                                                            for choice in [
                                                                InitialZoom::ActualSize,
                                                                InitialZoom::FitWidth,
                                                                InitialZoom::FitPage,
                                                            ] {
                                                                let state = zoom_state.clone();
                                                                menu = menu.item(
                                                                    PopupMenuItem::new(choice.label())
                                                                        .checked(choice == initial_zoom)
                                                                        .on_click(move |_, _, cx| {
                                                                            state.update(cx, |settings, cx| settings.choose(SettingsChange::InitialZoom(choice), cx));
                                                                        }),
                                                                );
                                                            }
                                                            menu
                                                        },
                                                    ),
                                            ),
                                    ))
                                    .children((page == SettingsPage::Reader).then(||
                                        preference_row(
                                            "Reading view",
                                            "Set the layout for newly opened PDFs. Open readers keep their view.",
                                            Button::new("reading-view-choice")
                                                .small()
                                                .outline()
                                                .label(reading_view.label())
                                                .accessibility_label(format!("Reading view: {}", reading_view.label()))
                                                .dropdown_caret(true)
                                                .dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |mut menu, _, _| {
                                                    for choice in [ReadingView::Continuous, ReadingView::Page] {
                                                        let state = view_state.clone();
                                                        menu = menu.item(PopupMenuItem::new(choice.label())
                                                            .checked(choice == reading_view)
                                                            .on_click(move |_, _, cx| {
                                                                state.update(cx, |settings, cx| settings.choose(SettingsChange::ReadingView(choice), cx));
                                                            }));
                                                    }
                                                    menu
                                                }),
                                            cx,
                                        )
                                    )),
                            ))
                            .children((page == SettingsPage::Application).then(||
                                div()
                                    .mx_4()
                                    .pb_2()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("UI Elements"),
                            ))
                            .children((page == SettingsPage::Application).then(||
                                div().p_4().flex().flex_col().gap_4()
                                    .child(preference_row(
                                        "Show Toolbar",
                                        "Show navigation and zoom controls in reader windows.",
                                        Switch::new("show-toolbar")
                                            .accessibility_label("Show Toolbar")
                                            .checked(show_toolbar)
                                            .on_change(move |show, _, cx| {
                                                toolbar_state.update(cx, |settings, cx| settings.choose(SettingsChange::ShowToolbar(*show), cx));
                                            }),
                                        cx,
                                    ))
                                    .child(preference_row(
                                        "Show Statusbar",
                                        "Show page and zoom information at the bottom of reader windows.",
                                        Switch::new("show-statusbar")
                                            .accessibility_label("Show Statusbar")
                                            .checked(show_statusbar)
                                            .on_change(move |show, _, cx| {
                                                statusbar_state.update(cx, |settings, cx| settings.choose(SettingsChange::ShowStatusbar(*show), cx));
                                            }),
                                        cx,
                                    ))
                            ))
                            .children((page == SettingsPage::Application).then(||
                                div()
                                    .mx_4()
                                    .pb_2()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Windows"),
                            ))
                            .children((page == SettingsPage::Application).then(||
                                div()
                                    .p_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_4()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_sm().font_medium().child("Quit When Last Window Closes"))
                                            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("When off, Kver stays open without a window on macOS. Use New Window or open a PDF to continue.")),
                                    )
                                    .child(
                                        Switch::new("quit-on-last-close")
                                            .accessibility_label("Quit When Last Window Closes")
                                            .checked(quit_on_close)
                                            .on_change(move |quit, _, cx| {
                                                close_state.update(cx, |settings, cx| settings.choose(SettingsChange::QuitWhenLastWindowCloses(*quit), cx));
                                            }),
                                    ),
                            ))
                            .children((page == SettingsPage::Application).then(||
                                div().px_4().pb_4().child(preference_row(
                                    "Reopen last windows on startup",
                                    "Saved for a future release. Restoring windows is not implemented yet.",
                                    Switch::new("reopen-last-windows")
                                        .accessibility_label("Reopen last windows on startup")
                                        .checked(reopen)
                                        .on_change(move |reopen, _, cx| {
                                            reopen_state.update(cx, |settings, cx| settings.choose(SettingsChange::ReopenLastWindowsOnStartup(*reopen), cx));
                                        }),
                                    cx,
                                ))
                            ))
                            .children((page == SettingsPage::Persistence).then(||
                                div()
                                    .mx_4()
                                    .mt_4()
                                    .pb_2()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Reading History"),
                            ))
                            .children((page == SettingsPage::Persistence).then(||
                                div()
                                    .p_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_4()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_sm().font_medium().child("Remember Reading History"))
                                            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Keep recently opened PDFs on this device. Turning this off does not erase saved data.")),
                                    )
                                    .child(
                                        Switch::new("remember-reading-history")
                                            .accessibility_label("Remember Reading History")
                                            .checked(remember)
                                            .on_change(move |next, _, cx| {
                                                history_state.update(cx, |settings, cx| settings.choose_history(*next, cx));
                                            }),
                                    ),
                            ))
                            .children((page == SettingsPage::Persistence).then(||
                                div().px_4().pb_4().child(preference_row(
                                    "Reopen at Last Position",
                                    "Resume a PDF where you stopped. When off, new opens start at the beginning without erasing the saved place.",
                                    Switch::new("reopen-at-last-position")
                                        .accessibility_label("Reopen at Last Position")
                                        .checked(reopen_position)
                                        .on_change(move |next, _, cx| {
                                            position_state.update(cx, |settings, cx| settings.choose(SettingsChange::ReopenAtLastPosition(*next), cx));
                                        }),
                                    cx,
                                ))
                            ))
                            .children((page == SettingsPage::Persistence).then(||
                                div()
                                    .p_4()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_4()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_sm().font_medium().child("Erase reading history"))
                                            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("Erase saved data for every PDF. PDF files and app preferences stay unchanged.")),
                                    )
                                    .child(
                                        Button::new("clear-reading-history")
                                            .small()
                                            .outline()
                                            .label(if clearing { "Clearing…" } else { "Clear…" })
                                            .disabled(clearing)
                                            .on_click(move |_, window, cx| {
                                                let owner = clear_owner.clone();
                                                window.open_alert_dialog(cx, move |alert, _, _| {
                                                    let owner = owner.clone();
                                                    alert
                                                        .title("Clear reading history?")
                                                        .description("Erase saved data for every PDF, including this session? PDF files and app preferences will not be changed.")
                                                        .confirm()
                                                        .centered()
                                                        .entrance(DialogEntrance::None)
                                                        .ok_text("Clear history")
                                                        .ok_variant(ButtonVariant::Danger)
                                                        .on_ok(move |_, _, cx| {
                                                            let _ = owner.update(cx, |view, cx| view.clear_history(cx));
                                                            true
                                                        })
                                                });
                                            }),
                                    ),
                            ))
                            .when_some(clear_error.filter(|_| page == SettingsPage::Persistence), |view, message| {
                                view.child(
                                    div()
                                        .id("clear-history-error")
                                        .test_support()
                                        .role(gpui::Role::Status)
                                        .aria_label(message.clone())
                                        .px_4()
                                        .py_2()
                                        .text_sm()
                                        .text_color(cx.theme().danger)
                                        .child(message),
                                )
                            })
                            .when_some(error, |view, message| {
                                view.child(
                                    div()
                                        .id("settings-error")
                                        .test_support()
                                        .role(gpui::Role::Status)
                                        .aria_label(message.clone())
                                        .px_4()
                                        .py_2()
                                        .text_sm()
                                        .text_color(cx.theme().danger)
                                        .child(message),
                                )
                            }),
                    ),
            )
            .track_focus(&self.focus)
    }
}

fn preference_row(
    label: &'static str,
    description: &'static str,
    control: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_medium().child(label))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                ),
        )
        .child(control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        reader::{Preview, ShowRecent},
        recent::RecentStore,
    };
    use gpui_kit::{TestAppContext, test::TestWindowExt as _};

    fn choose(cx: &mut TestAppContext, window: AnyWindowHandle, option: usize) {
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("appearance-choice", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.within("popup-menu").click(option, cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
    }

    fn poll(cx: &mut TestAppContext) {
        cx.background_executor
            .advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn settings_pages_show_only_the_selected_page(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(cx, None);
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                let title = frame.find("settings-window-title").bounds();
                let titlebar = frame.find("title-bar").bounds();
                assert_eq!(title.center().x, titlebar.center().x);
                assert!(frame.try_find("appearance-choice").is_some());
                assert!(frame.try_find("initial-zoom-choice").is_none());
                assert!(frame.try_find("remember-reading-history").is_none());
                for target in ["reader", "reading-experience"] {
                    frame.click(target, cx);
                    assert!(frame.try_find("appearance-choice").is_none());
                    assert!(frame.try_find("initial-zoom-choice").is_some());
                    assert!(frame.try_find("reading-view-choice").is_some());
                    assert!(frame.try_find("remember-reading-history").is_none());
                }
                for target in ["persistence", "reading-history"] {
                    frame.click(target, cx);
                    assert!(frame.try_find("appearance-choice").is_none());
                    assert!(frame.try_find("initial-zoom-choice").is_none());
                    assert!(frame.try_find("remember-reading-history").is_some());
                }
                for target in ["application", "theme", "ui-elements", "windows"] {
                    frame.click(target, cx);
                    assert!(frame.try_find("appearance-choice").is_some());
                    assert!(frame.try_find("show-toolbar").is_some());
                    assert!(frame.try_find("show-statusbar").is_some());
                    assert!(frame.try_find("quit-on-last-close").is_some());
                    assert!(frame.try_find("initial-zoom-choice").is_none());
                }
                // Arrow-key selection switches pages without leaving the tree.
                frame.press("down", cx);
                assert!(frame.try_find("initial-zoom-choice").is_some());
                frame.press("down", cx);
                frame.press("down", cx);
                assert!(frame.try_find("remember-reading-history").is_some());
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn ui_elements_update_all_readers_and_follow_file_edits(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let settings_window = cx.update(|cx| cx.windows()[0]);
        let readers = cx.update(|cx| {
            (0..2)
                .map(|_| {
                    gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
                    })
                    .unwrap()
                    .0
                })
                .collect::<Vec<_>>()
        });
        cx.run_until_parked();
        for (toolbar, statusbar, toggle) in [
            (true, true, None),
            (false, true, Some("show-toolbar")),
            (false, false, Some("show-statusbar")),
            (true, false, Some("show-toolbar")),
            (true, true, Some("show-statusbar")),
        ] {
            if let Some(toggle) = toggle {
                cx.update(|cx| {
                    cx.update_window(settings_window, |_, frame, cx| {
                        frame.render_frame(cx);
                        frame.click(toggle, cx);
                    })
                    .unwrap();
                });
                cx.run_until_parked();
                let saved = settings::load(&config).unwrap();
                assert_eq!(saved.show_toolbar(), toolbar);
                assert_eq!(saved.show_statusbar(), statusbar);
            }
            for reader in &readers {
                cx.update(|cx| {
                    cx.update_window(*reader, |_, frame, cx| {
                        frame.render_frame(cx);
                        assert_eq!(frame.try_find("settings-button").is_some(), toolbar);
                        assert_eq!(frame.try_find("reader-status").is_some(), statusbar);
                        assert!(frame.find("page").visible());
                    })
                    .unwrap();
                });
            }
        }
        std::fs::write(
            &config,
            "[application]\nshow_toolbar = false\nshow_statusbar = false\n",
        )
        .unwrap();
        poll(cx);
        for reader in readers {
            cx.update(|cx| {
                cx.update_window(reader, |_, frame, cx| {
                    frame.render_frame(cx);
                    assert!(frame.try_find("settings-button").is_none());
                    assert!(frame.try_find("reader-status").is_none());
                })
                .unwrap();
            });
        }
    }

    #[gpui_kit::test]
    fn new_preferences_persist_and_follow_live_file_edits(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "show-statusbar",
                    gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
                    cx,
                );
                frame.click("reopen-last-windows", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            settings::load(&config)
                .unwrap()
                .reopen_last_windows_on_startup()
        );
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.click("reader", cx);
                frame.click("reading-view-choice", cx);
                frame.press("down", cx);
                frame.press("down", cx);
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert_eq!(
            settings::load(&config).unwrap().reading_view(),
            ReadingView::Page
        );
        std::fs::write(&config, "[application]\nreopen_last_windows_on_startup = false\n[reader]\nreading_view = 'continuous'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("reading-view-choice").label(),
                    Some("Reading view: Continuous")
                );
                frame.click("application", cx);
                assert_eq!(frame.find("reopen-last-windows").checked(), Some(false));
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn restore_position_switch_persists_and_tracks_valid_file_edits(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            install_with_paths(cx, Some(paths.clone()));
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("reading-history", cx);
                assert_eq!(frame.find("reopen-at-last-position").checked(), Some(true));
                frame.click("reopen-at-last-position", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            !settings::load(paths.config())
                .unwrap()
                .reopen_at_last_position()
        );
        std::fs::write(
            paths.config(),
            "[persistence]\nreopen_at_last_position = true\n",
        )
        .unwrap();
        poll(cx);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(frame.find("reopen-at-last-position").checked(), Some(true));
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn clear_all_requires_confirmation_and_refreshes_reader_windows(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
        let store = RecentStore::new(&paths);
        let pdf =
            RecentStore::normalize(std::path::Path::new("examples/readme-sample.pdf")).unwrap();
        let missing = root.path().join("missing.pdf");
        store.record(&pdf).unwrap();
        store.record(&missing).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            install_with_paths(cx, Some(paths.clone()));
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let settings_window = cx.update(|cx| cx.windows()[0]);
        let (reader_window, document) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| {
                    crate::app::document_window::DocumentWindow::new(Some(pdf.clone()), window, cx)
                })
            })
            .unwrap()
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(reader_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.dispatch_action(Box::new(ShowRecent), cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(reader_window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .try_find(format!("forget-pdf:{}", missing.display()))
                        .is_some()
                );
            })
            .unwrap()
        });
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("reading-history", cx);
                frame.click("remember-reading-history", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            !settings::load(paths.config())
                .unwrap()
                .remember_reading_history()
        );
        assert!(
            store.contains(&missing).unwrap(),
            "turning off erased old data"
        );
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "remember-reading-history",
                    gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-800.))),
                    cx,
                );
                frame.click("clear-reading-history", cx);
                assert_eq!(frame.find("ok").label(), Some("Clear history"));
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(store.contains(&pdf).unwrap());
        assert!(store.contains(&missing).unwrap());
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| frame.click("cancel", cx))
                .unwrap();
        });
        cx.run_until_parked();
        assert!(store.contains(&pdf).unwrap());
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.click("clear-reading-history", cx);
                frame.click("ok", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(store.search("").unwrap().is_empty());
        assert!(std::path::Path::new("examples/readme-sample.pdf").exists());
        assert!(
            !settings::load(paths.config())
                .unwrap()
                .remember_reading_history()
        );
        cx.update(|cx| {
            cx.update_window(reader_window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .try_find(format!("forget-pdf:{}", missing.display()))
                        .is_none()
                );
                assert!(frame.find("recent-panel").visible());
            })
            .unwrap()
        });
        document.read_with(cx, |document, cx| {
            assert_eq!(document.reader().read(cx).path(), Some(pdf.as_path()))
        });
        cx.update(|cx| {
            cx.update_window(settings_window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("remember-reading-history", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            settings::load(paths.config())
                .unwrap()
                .remember_reading_history()
        );
        assert!(
            store.search("").unwrap().is_empty(),
            "re-enabling restored cleared data"
        );
    }

    #[gpui_kit::test]
    fn last_close_switch_persists_and_follows_live_toml(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            assert!(state(cx).read(cx).quit_when_last_window_closes());
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.scroll(
                    "show-statusbar",
                    gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
                    cx,
                );
                frame.click("quit-on-last-close", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(!cx.update(|cx| state(cx).read(cx).quit_when_last_window_closes()));
        assert!(
            !settings::load(&config)
                .unwrap()
                .quit_when_last_window_closes()
        );

        std::fs::write(&config, "[windows]\nquit_when_last_window_closes = true\n").unwrap();
        poll(cx);
        assert!(cx.update(|cx| state(cx).read(cx).quit_when_last_window_closes()));

        std::fs::write(&config, "[windows]\nquit_when_last_window_closes = 'bad'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert!(state(cx).read(cx).quit_when_last_window_closes());
            assert!(state(cx).read(cx).error().is_some());
        });
        std::fs::write(&config, "[windows]\nquit_when_last_window_closes = false\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert!(!state(cx).read(cx).quit_when_last_window_closes());
            assert!(state(cx).read(cx).error().is_none());
        });
    }

    #[gpui_kit::test]
    fn external_edits_recover_and_panel_saves_latest_file(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(&config, "[appearance]\ntheme_mode = 'light'\n").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| assert_eq!(state(cx).read(cx).appearance(), Appearance::Light));

        std::fs::write(&config, "[appearance]\ntheme_mode = 'dark'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark);
            assert!(cx.global::<Theme>().is_dark());
        });

        // A partial in-place save cannot replace the last valid value, including
        // when the user tries the panel while the editor still holds invalid TOML.
        std::fs::write(&config, "[appearance]\ntheme_mode = '").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark);
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: Dark")
                );
                assert!(
                    frame
                        .find("settings-error")
                        .label()
                        .unwrap()
                        .contains("Correct")
                );
            })
            .unwrap();
        });
        choose(cx, window, 1);
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "[appearance]\ntheme_mode = '"
        );
        cx.update(|cx| assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark));

        // Atomic editor replacement must be followed even if the original inode
        // is no longer present. Subsequent panel writes start from that file.
        let replacement = root.path().join("editor.tmp");
        std::fs::write(
            &replacement,
            "# editor note\n[appearance]\ntheme_mode = 'light'\n[other]\nkey = 'keep'\n",
        )
        .unwrap();
        std::fs::rename(replacement, &config).unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Light);
            assert!(!cx.global::<Theme>().is_dark());
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: Light")
                );
                assert!(frame.try_find("settings-error").is_none());
            })
            .unwrap();
        });
        // Another valid hand edit arrives before the panel's next choice.
        std::fs::write(
            &config,
            "# newest edit\n[appearance]\ntheme_mode = 'light'\n[other]\nkey = 'newest'\n",
        )
        .unwrap();
        choose(cx, window, 2);
        assert_eq!(
            settings::load(&config).unwrap().appearance(),
            Appearance::Dark
        );
        let content = std::fs::read_to_string(&config).unwrap();
        assert!(content.contains("# newest edit"));
        assert!(content.contains("key = 'newest'"));

        std::fs::write(&config, "[appearance]\ntheme_mode = 'purple'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark);
            assert!(
                state(cx)
                    .read(cx)
                    .error()
                    .unwrap()
                    .contains("Unknown appearance")
            );
        });
        std::fs::write(&config, "[appearance]\ntheme_mode = 'system'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::System);
            assert!(state(cx).read(cx).error().is_none());
        });
    }

    #[gpui_kit::test]
    fn settings_action_opens_one_window_and_choice_survives_restart(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config/config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| {
            assert_eq!(cx.windows().len(), 1);
            cx.windows()[0]
        });
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: System")
                );
                assert!(frame.try_find("appearance-reset").is_none());
                frame.within("settings-sidebar").click("theme", cx);
                for _ in 0..6 {
                    if frame.find("appearance-choice").focused() == Some(true) {
                        break;
                    }
                    frame.press("tab", cx);
                }
                assert_eq!(frame.find("appearance-choice").focused(), Some(true));
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.press("down", cx);
                frame.press("down", cx);
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: Light")
                );
                assert_eq!(
                    frame.find("appearance-reset").label(),
                    Some("Reset appearance to System")
                );
                frame.click("appearance-reset", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: System")
                );
                assert!(frame.try_find("appearance-reset").is_none());
            })
            .unwrap();
        });
        assert_eq!(
            settings::load(&config).unwrap().appearance(),
            Appearance::System
        );
        choose(cx, window, 2);
        cx.update(|cx| assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark));
        cx.run_until_parked();
        assert_eq!(
            settings::load(&config).unwrap().appearance(),
            Appearance::Dark
        );
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert!(cx.update(|cx| cx.windows().is_empty()));

        // Loading the same isolated location on a second app instance is the
        // startup/restart contract; it must not depend on a real home directory.
        assert_eq!(
            settings::load(&config).unwrap().appearance(),
            Appearance::Dark
        );
    }

    #[gpui_kit::test]
    fn startup_reads_grouped_appearance_into_open_settings(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(&config, "[appearance]\ntheme_mode = 'dark'\n").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(config, root.path().join("data"))),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark);
            assert!(cx.global::<Theme>().is_dark());
            cx.update_window(cx.windows()[0], |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("appearance-choice").label(),
                    Some("Appearance: Dark")
                );
                assert!(frame.find("appearance-reset").visible());
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn save_failure_keeps_last_valid_choice_and_explains_recovery(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(&config, "appearance = [").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .find("settings-error")
                        .label()
                        .unwrap()
                        .contains("Cannot read settings")
                );
            })
            .unwrap();
        });
        choose(cx, window, 1);
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).appearance(), Appearance::System);
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .find("settings-error")
                        .label()
                        .unwrap()
                        .contains("Cannot read settings")
                );
            })
            .unwrap();
        });
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "appearance = [");
    }

    #[gpui_kit::test]
    fn reader_entry_opens_settings_and_updates_app_theme(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::app::keymap::install(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    root.path().join("config.toml"),
                    root.path().join("data"),
                )),
            );
        });
        let reader = cx
            .add_window(|window, cx| crate::reader::Preview::new(None, window.scale_factor(), cx));
        cx.update(|cx| {
            cx.update_window(reader.into(), |_, frame, cx| {
                frame.render_frame(cx);
                frame.click("settings-button", cx);
            })
            .unwrap();
        });
        let settings = cx.update(|cx| {
            assert_eq!(cx.windows().len(), 2);
            *cx.windows()
                .iter()
                .find(|&&handle| handle != reader.into())
                .unwrap()
        });
        choose(cx, settings, 2);
        cx.update(|cx| assert!(cx.global::<Theme>().is_dark()));
        cx.run_until_parked();
        assert_eq!(
            settings::load(&root.path().join("config.toml"))
                .unwrap()
                .appearance(),
            Appearance::Dark
        );
    }

    #[gpui_kit::test]
    fn unavailable_home_still_allows_session_choice(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(cx, None);
            cx.dispatch_action(&ShowSettings);
        });
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .find("settings-error")
                        .label()
                        .unwrap()
                        .contains("no user configuration directory")
                );
            })
            .unwrap();
        });
        choose(cx, window, 2);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .find("settings-error")
                        .label()
                        .unwrap()
                        .contains("cannot be saved")
                );
            })
            .unwrap();
            assert_eq!(state(cx).read(cx).appearance(), Appearance::Dark);
            assert!(cx.global::<Theme>().is_dark());
        });
    }

    #[gpui_kit::test]
    fn initial_zoom_control_is_keyboard_operable_and_tracks_file_edits(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(&config, "[reader]\ninitial_zoom = 'fit_width'\n").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(
                cx,
                Some(AppPaths::for_locations(
                    config.clone(),
                    root.path().join("data"),
                )),
            );
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame
                    .within("settings-sidebar")
                    .click("reading-experience", cx);
                assert_eq!(
                    frame.find("initial-zoom-choice").label(),
                    Some("Initial Zoom: Fit Width")
                );
                for _ in 0..6 {
                    if frame.find("initial-zoom-choice").focused() == Some(true) {
                        break;
                    }
                    frame.press("tab", cx);
                }
                assert_eq!(frame.find("initial-zoom-choice").focused(), Some(true));
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.press("down", cx);
                frame.press("down", cx);
                frame.press("down", cx);
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("initial-zoom-choice").label(),
                    Some("Initial Zoom: Fit Page")
                );
                assert_eq!(frame.find("initial-zoom-choice").focused(), Some(true));
            })
            .unwrap();
            assert_eq!(state(cx).read(cx).initial_zoom(), InitialZoom::FitPage);
        });
        assert_eq!(
            settings::load(&config).unwrap().initial_zoom(),
            InitialZoom::FitPage
        );
        std::fs::write(&config, "[reader]\ninitial_zoom = '100_percent'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("initial-zoom-choice").label(),
                    Some("Initial Zoom: 100%")
                );
            })
            .unwrap();
        });
    }
}
