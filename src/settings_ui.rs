//! One application-wide Settings window and its asynchronous preference state.
use std::{cell::Cell, time::Duration};

mod panel;

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Disableable as _, StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogEntrance,
    kbd::Kbd,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    switch::Switch,
    theme::{Theme, ThemeMode},
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, Half as _,
    Keystroke, Render, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    app::{
        keymap::{self, Preset},
        shortcut_label,
    },
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

    pub fn remove_missing_recent_files(&self) -> bool {
        self.settings.remove_missing_recent_files()
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

    pub fn keyboard_preset(&self) -> crate::app::keymap::Preset {
        self.settings.keyboard_preset()
    }

    pub fn annotation_author(&self) -> &str {
        self.settings.annotation_author()
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn choose(&mut self, change: SettingsChange, cx: &mut Context<Self>) {
        let current = self.settings.clone();
        let updated = match change.clone() {
            SettingsChange::Appearance(choice) => current.with_appearance(choice),
            SettingsChange::ShowToolbar(show) => current.with_show_toolbar(show),
            SettingsChange::ShowStatusbar(show) => current.with_show_statusbar(show),
            SettingsChange::InitialZoom(choice) => current.with_initial_zoom(choice),
            SettingsChange::ReadingView(choice) => current.with_reading_view(choice),
            SettingsChange::KeyboardPreset(choice) => current.with_keyboard_preset(choice),
            SettingsChange::AnnotationAuthor(author) => current.with_annotation_author(author),
            SettingsChange::ReopenLastWindowsOnStartup(reopen) => {
                current.with_reopen_last_windows_on_startup(reopen)
            }
            SettingsChange::QuitWhenLastWindowCloses(quit) => {
                current.with_quit_when_last_window_closes(quit)
            }
            SettingsChange::RememberReadingHistory(remember) => {
                current.with_remember_reading_history(remember)
            }
            SettingsChange::RemoveMissingRecentFiles(remove) => {
                current.with_remove_missing_recent_files(remove)
            }
            SettingsChange::ReopenAtLastPosition(reopen) => {
                current.with_reopen_at_last_position(reopen)
            }
        };
        if self.settings == updated && self.pending.is_none() {
            return;
        }
        self.revision += 1;
        if self.paths.is_none() {
            if let Err(error) = self.apply_settings(updated, cx) {
                self.error = Some(error);
            }
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
            if let Err(error) = self.apply_settings(
                self.settings
                    .clone()
                    .with_remember_reading_history(remember),
                cx,
            ) {
                self.error = Some(error);
            }
            self.error = Some(
                "Settings cannot be saved: no user configuration directory is available.".into(),
            );
        } else {
            self.pending = Some(SettingsChange::RememberReadingHistory(remember));
            self.start_save(cx);
        }
        cx.notify();
    }

    fn apply_settings(
        &mut self,
        updated: AppSettings,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.settings.keyboard_preset() != updated.keyboard_preset() {
            crate::app::keymap::select_preset(cx, updated.keyboard_preset())?;
        }
        if self.settings.appearance() != updated.appearance() {
            updated.appearance().apply(cx);
        }
        let cleanup_changed =
            self.settings.remove_missing_recent_files() != updated.remove_missing_recent_files();
        if cleanup_changed && let Some(history) = recent::history(cx) {
            history.set_remove_missing_recent_files(updated.remove_missing_recent_files())?;
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
        if cleanup_changed {
            recent::notify_changed(cx);
        }
        cx.notify();
        Ok(())
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
                            this.error = this.apply_settings(updated, cx).err();
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
                if self.settings != updated
                    && let Err(error) = self.apply_settings(updated, cx)
                {
                    self.error = Some(error);
                    return;
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
    let bounds = Bounds::centered(None, size(px(850.), px(680.)), cx);
    let (handle, _) = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(600.), px(350.))),
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| cx.new(|cx| panel::SettingsView::new(window, cx)),
    )?;
    cx.update_window(handle, |_, window, _| window.activate_window())?;
    Ok(handle)
}

fn shortcut_help(cx: &App, preset: Preset) -> gpui::AnyElement {
    use keymap::Context as KeyContext;
    let platform = keymap::platform(cx);
    let fallback;
    let map = if let Some(map) = keymap::active_map(cx) {
        map
    } else {
        fallback = keymap::resolve(platform, preset).expect("embedded keyboard policy");
        &fallback
    };
    let platform_name = match platform {
        keymap::Platform::Mac => "macOS",
        keymap::Platform::Linux => "Linux",
        keymap::Platform::Windows => "Windows",
    };
    let mut sections = div()
        .id("shortcut-help")
        .test_support()
        .role(gpui::Role::Group)
        .aria_label(format!(
            "{} keyboard shortcuts for {platform_name}",
            preset.label()
        ))
        .flex()
        .flex_col()
        .gap_4();
    for (context, context_label) in [
        (KeyContext::Global, "Anywhere"),
        (KeyContext::Reader, "Reader"),
        (KeyContext::Sidebar, "Sidebar"),
        (KeyContext::Contents, "Contents"),
        (KeyContext::Annotations, "Annotations"),
        (KeyContext::Settings, "Settings"),
        (KeyContext::About, "About"),
    ] {
        let mut groups = div().flex().flex_col().gap_3();
        let mut any = false;
        for (task, task_label) in [
            ("Open", "Open and windows"),
            ("Navigate", "Navigate and scroll"),
            ("Find", "Find and contents"),
            ("View", "View and zoom"),
            ("Annotations", "Annotations"),
            ("Marks", "Marks"),
            ("Other", "Other"),
        ] {
            let mut rows: Vec<(&str, Vec<&str>)> = Vec::new();
            for (_, action, keys) in map
                .entries()
                .filter(|(scope, _, _)| *scope == context)
                .filter(|(_, action, _)| shortcut_task(action) == task)
            {
                if let Some((_, shortcuts)) = rows.iter_mut().find(|(name, _)| *name == action) {
                    shortcuts.push(keys);
                } else {
                    rows.push((action, vec![keys]));
                }
            }
            if rows.is_empty() {
                continue;
            }
            any = true;
            let mut group = div().flex().flex_col().gap_1().child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(task_label),
            );
            for (action, keys) in rows {
                let shortcuts = displayed_shortcuts(action, &keys, cx);
                let label = shortcut_label(action);
                group = group.child(
                    div()
                        .id(format!(
                            "shortcut-{context:?}-{}",
                            action.rsplit("::").next().unwrap_or(action)
                        ))
                        .test_support()
                        .aria_label(format!("{label}: {shortcuts}"))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_4()
                        .text_sm()
                        .child(label)
                        .child(shortcut_badges(action, &keys, cx)),
                );
            }
            groups = groups.child(group);
        }
        if any {
            sections = sections.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .pb_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .font_medium()
                            .child(context_label),
                    )
                    .child(groups),
            );
        }
    }
    sections.into_any_element()
}

struct ShortcutHint<'a> {
    source: &'a str,
    label: String,
    stroke: Option<Keystroke>,
}

fn vim_letter(key: &str) -> Option<char> {
    let (key, shift) = match key.strip_prefix("shift-") {
        Some(key) => (key, true),
        None => (key, false),
    };
    let letter = (key.len() == 1)
        .then(|| key.chars().next())
        .flatten()
        .filter(char::is_ascii_lowercase)?;
    Some(if shift {
        letter.to_ascii_uppercase()
    } else {
        letter
    })
}

fn shortcut_hints<'a>(action: &str, keys: &[&'a str], cx: &App) -> Vec<ShortcutHint<'a>> {
    let mark = match action.rsplit("::").next().unwrap_or(action) {
        "SetLocalMark" => Some(("m ", "m <LETTER>")),
        "JumpLocalMark" => Some(("' ", "' <LETTER>")),
        "DeleteLocalMark" => Some(("d m ", "d m <LETTER>")),
        _ => None,
    };
    let full_alphabet = mark.is_some_and(|(prefix, _)| {
        ('a'..='z').all(|letter| keys.contains(&format!("{prefix}{letter}").as_str()))
    });
    let mut hints = Vec::new();
    if let Some((_, pattern)) = mark.filter(|_| full_alphabet) {
        hints.push(ShortcutHint {
            source: pattern,
            label: pattern.to_owned(),
            stroke: None,
        });
    }
    for key in keys {
        if full_alphabet
            && mark.is_some_and(|(prefix, _)| {
                key.strip_prefix(prefix).is_some_and(|letter| {
                    letter.len() == 1 && letter.bytes().all(|byte| byte.is_ascii_lowercase())
                })
            })
        {
            continue;
        }
        let parts = key.split_whitespace().collect::<Vec<_>>();
        let letters = parts
            .iter()
            .map(|part| vim_letter(part))
            .collect::<Option<String>>();
        let (label, stroke) = if let Some(letters) = letters {
            (letters, None)
        } else if parts.len() == 1 {
            match Keystroke::parse_with_os(key, cx.platform_info().os) {
                Ok(stroke) => (Kbd::format(&stroke), Some(stroke)),
                Err(_) => ((*key).to_owned(), None),
            }
        } else {
            (shortcut_label::format(key, cx), None)
        };
        hints.push(ShortcutHint {
            source: key,
            label,
            stroke,
        });
    }
    hints
}

fn shortcut_badges(action: &str, keys: &[&str], cx: &App) -> impl IntoElement {
    let mut badges = div().flex().flex_wrap().justify_end().gap_1();
    for (index, hint) in shortcut_hints(action, keys, cx).into_iter().enumerate() {
        let badge = match hint.stroke {
            Some(stroke) => Kbd::new(stroke).into_any_element(),
            None => div()
                .text_color(cx.theme().muted_foreground)
                .bg(cx.theme().tokens.muted)
                .rounded(cx.theme().radius.half())
                .text_xs()
                .px_1()
                .py_0p5()
                .min_w_5()
                .text_center()
                .flex_shrink_0()
                .child(hint.label.clone())
                .into_any_element(),
        };
        badges = badges.child(
            div()
                .id(format!("shortcut-binding:{}", hint.source))
                .test_support()
                .aria_label(hint.label)
                .flex()
                .items_center()
                .gap_1()
                .when(index > 0, |pair| {
                    pair.child(
                        div()
                            .id("shortcut-alternative-separator")
                            .test_support()
                            .aria_label(",")
                            .child(","),
                    )
                })
                .child(badge),
        );
    }
    badges
}

fn displayed_shortcuts(action: &str, keys: &[&str], cx: &App) -> String {
    shortcut_hints(action, keys, cx)
        .into_iter()
        .map(|hint| hint.label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn shortcut_task(action: &str) -> &'static str {
    let name = action.rsplit("::").next().unwrap_or(action);
    if matches!(
        name,
        "ApplyMarkup"
            | "Highlight"
            | "Underline"
            | "StrikeOut"
            | "Squiggly"
            | "ShowAnnotations"
            | "DeleteHighlight"
            | "UndoAnnotation"
            | "RedoAnnotation"
            | "RecolorHighlight"
    ) {
        "Annotations"
    } else if name.contains("Mark") {
        "Marks"
    } else if name.contains("Window")
        || matches!(
            name,
            "OpenPdf"
                | "ShowRecent"
                | "Quit"
                | "ShowSettings"
                | "CloseSettings"
                | "CloseAbout"
                | "ShowCommandPalette"
        )
    {
        "Open"
    } else if name.contains("Find")
        || name.contains("Match")
        || name.contains("Contents")
        || name == "ToggleSidebar"
        || name == "ActivateOutline"
    {
        "Find"
    } else if name.contains("Zoom")
        || name.contains("Fit")
        || name.contains("Page") && matches!(name, "SinglePage")
        || matches!(name, "Continuous")
    {
        "View"
    } else if name.contains("Scroll")
        || name.contains("Page")
        || name.contains("Navigate")
        || name.contains("Document")
    {
        "Navigate"
    } else {
        "Other"
    }
}

fn shortcut_label(action: &str) -> String {
    let name = action.rsplit("::").next().unwrap_or(action);
    match name {
        "ApplyMarkup" => return "Apply annotation".into(),
        "StrikeOut" => return "Strikethrough".into(),
        "Squiggly" => return "Squiggly underline".into(),
        "DeleteHighlight" => return "Delete annotation".into(),
        _ => {}
    }
    let mut label = String::new();
    for (index, ch) in name.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            label.push(' ');
        }
        label.push(ch);
    }
    label.replace("Pdf", "PDF")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::document_window::DocumentWindow, reader::ShowRecent, recent::RecentStore};
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
    fn annotation_author_input_persists_and_tracks_valid_live_file_changes(
        cx: &mut TestAppContext,
    ) {
        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(cx, Some(paths.clone()));
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("annotation-author").value(), Some(""));
            window.click("annotation-author", cx);
            window.input("Jón 中文", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            settings::load(paths.config()).unwrap().annotation_author(),
            "Jón 中文"
        );
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("annotation-author").focused(), Some(true));
            window.press("home", cx);
            window.press("right", cx);
            window.input("x", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            settings::load(paths.config()).unwrap().annotation_author(),
            "Jxón 中文"
        );
        for (toml, expected, error) in [
            (
                "[application]\nannotation_author = 'Other reader'\n",
                "Other reader",
                false,
            ),
            (
                "[application]\nannotation_author = 12\n",
                "Other reader",
                true,
            ),
            ("[application]\nannotation_author = ''\n", "", false),
        ] {
            std::fs::write(paths.config(), toml).unwrap();
            poll(cx);
            cx.update(|cx| {
                assert_eq!(state(cx).read(cx).annotation_author(), expected);
                assert_eq!(state(cx).read(cx).error().is_some(), error);
            });
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(window.find("annotation-author").value(), Some(expected));
            })
            .unwrap();
        }
        cx.update_window(window, |_, window, cx| {
            window.click("annotation-author", cx);
            window.input("Restart author", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update(|cx| {
            install_with_paths(cx, Some(paths.clone()));
        });
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(state(cx).read(cx).annotation_author(), "Restart author"));
    }

    #[gpui_kit::test]
    fn shortcut_hints_preserve_vim_case_and_group_alternatives(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            assert_eq!(
                displayed_shortcuts("kver::GoToDocumentStart", &["g g"], cx),
                "gg"
            );
            assert_eq!(
                displayed_shortcuts("kver::GoToDocumentEnd", &["shift-g"], cx),
                "G"
            );
            assert_eq!(
                displayed_shortcuts("kver::GoToDocumentStart", &["g g", "shift-g"], cx),
                "gg, G"
            );
            let hints = shortcut_hints(
                "kver::GoToDocumentStart",
                &["g g", "shift-g", "cmd-alt-right"],
                cx,
            );
            let modified = shortcut_label::format("cmd-alt-right", cx);
            assert_eq!(
                hints
                    .iter()
                    .map(|hint| hint.label.as_str())
                    .collect::<Vec<_>>(),
                ["gg", "G", modified.as_str()]
            );
            assert!(hints[0].stroke.is_none());
            assert!(hints[1].stroke.is_none());
            assert!(hints[2].stroke.is_some());
        });
    }

    #[gpui_kit::test]
    fn keyboard_preset_can_be_changed_and_reset_without_pointer(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        cx.update(|cx| {
            gpui_kit::init(cx);
            keymap::install(cx);
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
                // Kit's sidebar owns page selection; the keyboard page is the fourth entry.
                frame.within("settings-sidebar").click("0-3", cx);
                assert!(frame.try_find("keyboard-preset-choice").is_some());
                assert!(frame.try_find("shortcut-help").is_some());
                let page_keys = if keymap::platform(cx) == keymap::Platform::Mac {
                    vec!["cmd-alt-right", "right", "alt-down"]
                } else {
                    vec!["cmd-alt-right", "right", "ctrl-pagedown"]
                };
                assert_eq!(
                    frame.find("shortcut-Reader-NextPage").label(),
                    Some(
                        format!(
                            "Next Page: {}",
                            page_keys
                                .iter()
                                .map(|key| shortcut_label::format(key, cx))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                        .as_str()
                    )
                );
                for _ in 0..8 {
                    if frame.find("keyboard-preset-choice").focused() == Some(true) {
                        break;
                    }
                    frame.press("tab", cx);
                }
                assert_eq!(frame.find("keyboard-preset-choice").focused(), Some(true));
                assert_eq!(
                    frame.find("keyboard-preset-choice").label(),
                    Some("Keyboard preset: Standard")
                );
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
            .unwrap()
        });
        cx.run_until_parked();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).keyboard_preset(), Preset::Vim);
            assert_eq!(keymap::active_preset(cx), Preset::Vim);
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("keyboard-preset-choice").label(),
                    Some("Keyboard preset: Vim")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-ApplyMarkup").label(),
                    Some("Apply annotation: aa")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-Squiggly").label(),
                    Some("Squiggly underline: aw")
                );
                assert!(
                    frame
                        .find("shortcut-Annotations-DeleteHighlight")
                        .label()
                        .unwrap()
                        .ends_with(", x")
                );
                assert_eq!(shortcut_task("kver::ApplyMarkup"), "Annotations");
                assert_eq!(
                    frame.find("shortcut-Reader-SetLocalMark").label(),
                    Some("Set Local Mark: m <LETTER>")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-JumpLocalMark").label(),
                    Some("Jump Local Mark: ' <LETTER>")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-DeleteLocalMark").label(),
                    Some("Delete Local Mark: d m <LETTER>")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-GoToDocumentStart").label(),
                    Some("Go To Document Start: gg")
                );
                assert_eq!(
                    frame
                        .within("shortcut-Reader-GoToDocumentStart")
                        .find("shortcut-binding:g g")
                        .label(),
                    Some("gg")
                );
                assert_eq!(
                    frame.find("shortcut-Reader-GoToDocumentEnd").label(),
                    Some("Go To Document End: G")
                );
                assert_eq!(
                    frame
                        .within("shortcut-Reader-GoToDocumentEnd")
                        .find("shortcut-binding:shift-g")
                        .label(),
                    Some("G")
                );
                assert_eq!(
                    frame
                        .within("shortcut-Reader-NextPage")
                        .find_all("shortcut-alternative-separator")
                        .len(),
                    1
                );
                assert_eq!(
                    frame.find("shortcut-Reader-NextPage").label(),
                    Some(
                        format!(
                            "Next Page: {}, J",
                            shortcut_label::format("cmd-alt-right", cx)
                        )
                        .as_str()
                    )
                );
                assert_eq!(frame.find("keyboard-preset-choice").focused(), Some(true));
                for _ in 0..8 {
                    if frame.find("keyboard-preset-reset").focused() == Some(true) {
                        break;
                    }
                    frame.press("shift-tab", cx);
                }
                assert_eq!(frame.find("keyboard-preset-reset").focused(), Some(true));
                frame.press("enter", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).keyboard_preset(), Preset::Standard);
            assert_eq!(keymap::active_preset(cx), Preset::Standard);
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(frame.try_find("keyboard-preset-reset").is_none());
                assert_eq!(frame.find("keyboard-preset-choice").focused(), Some(true));
                frame.press("escape", cx);
            })
            .unwrap();
        });
        assert_eq!(
            settings::load(&config).unwrap().keyboard_preset(),
            Preset::Standard
        );
    }

    #[gpui_kit::test]
    fn keyboard_help_and_preset_follow_live_user_bindings(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(
            &config,
            "[keyboard]\npreset = 'vim'\n[[keyboard.bindings]]\nkeys = 'ctrl-e'\ncontext = 'reader'\ncommand = 'next-page'\n",
        )
        .unwrap();
        let paths = AppPaths::for_locations(config, root.path().join("data"));
        cx.update(|cx| {
            gpui_kit::init(cx);
            keymap::install_with_paths(cx, Preset::Standard, Some(paths.clone()));
            install_with_paths(cx, Some(paths));
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            assert_eq!(keymap::active_preset(cx), Preset::Vim);
            assert_eq!(
                keymap::active_map(cx)
                    .unwrap()
                    .action("ctrl-e", keymap::Context::Reader),
                Some("kver::NextPage")
            );
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.within("settings-sidebar").click("0-3", cx);
                assert!(
                    frame
                        .find("shortcut-Reader-NextPage")
                        .label()
                        .unwrap()
                        .contains(&shortcut_label::format("ctrl-e", cx))
                );
            })
            .unwrap();
        });
        cx.update(|cx| {
            state(cx).update(cx, |settings, cx| {
                settings.choose(SettingsChange::KeyboardPreset(Preset::Standard), cx);
            });
        });
        cx.run_until_parked();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(keymap::active_preset(cx), Preset::Standard);
            assert_eq!(
                keymap::active_map(cx)
                    .unwrap()
                    .action("ctrl-e", keymap::Context::Reader),
                Some("kver::NextPage")
            );
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert!(
                    frame
                        .find("shortcut-Reader-NextPage")
                        .label()
                        .unwrap()
                        .contains(&shortcut_label::format("ctrl-e", cx))
                );
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn invalid_preset_and_override_combination_keeps_last_valid_keymap(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(
            &config,
            "[keyboard]\npreset = 'standard'\n[[keyboard.bindings]]\nkeys = 'g'\ncontext = 'reader'\ncommand = 'next-page'\n",
        )
        .unwrap();
        let paths = AppPaths::for_locations(config.clone(), root.path().join("data"));
        cx.update(|cx| {
            gpui_kit::init(cx);
            keymap::install_with_paths(cx, Preset::Standard, Some(paths.clone()));
            install_with_paths(cx, Some(paths));
        });
        cx.run_until_parked();
        cx.update(|cx| {
            state(cx).update(cx, |settings, cx| {
                settings.choose(SettingsChange::KeyboardPreset(Preset::Vim), cx);
            });
        });
        cx.run_until_parked();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(keymap::active_preset(cx), Preset::Standard);
            assert_eq!(state(cx).read(cx).keyboard_preset(), Preset::Standard);
            assert!(state(cx).read(cx).error().unwrap().contains("ambiguous"));
        });
        std::fs::write(&config, "[keyboard]\npreset = 'vim'\n").unwrap();
        poll(cx);
        cx.update(|cx| {
            assert_eq!(state(cx).read(cx).keyboard_preset(), Preset::Vim);
            assert_eq!(keymap::active_preset(cx), Preset::Vim);
        });
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
                assert!(frame.try_find("reset").is_none());
                assert!(frame.try_find("initial-zoom-choice").is_none());
                assert!(frame.try_find("remember-reading-history").is_none());
                frame.within("settings-sidebar").click("0-1", cx);
                assert!(frame.try_find("appearance-choice").is_none());
                assert!(frame.try_find("initial-zoom-choice").is_some());
                assert!(frame.try_find("reading-view-choice").is_some());
                assert!(frame.try_find("remember-reading-history").is_none());
                frame.within("settings-sidebar").click("0-2", cx);
                assert!(frame.try_find("appearance-choice").is_none());
                assert!(frame.try_find("initial-zoom-choice").is_none());
                assert!(frame.try_find("remember-reading-history").is_some());
                frame.within("settings-sidebar").click("0-0", cx);
                frame.within("settings-sidebar").click("0-0-0", cx);
                assert!(frame.try_find("appearance-choice").is_some());
                assert!(frame.try_find("initial-zoom-choice").is_none());
                // Kit scrolls to a selected group; earlier groups may be virtualized.
                frame.within("settings-sidebar").click("0-0-1", cx);
                assert!(frame.try_find("show-toolbar").is_some());
                assert!(frame.try_find("show-statusbar").is_some());
                frame.within("settings-sidebar").click("0-0-2", cx);
                assert!(frame.try_find("quit-on-last-close").is_some());
                // Selecting a Kit page changes only the content area.
                frame.within("settings-sidebar").click("0-1", cx);
                assert!(frame.try_find("initial-zoom-choice").is_some());
                frame.within("settings-sidebar").click("0-2", cx);
                assert!(frame.try_find("remember-reading-history").is_some());
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn searching_settings_filters_pages_without_changing_preferences(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install_with_paths(cx, None);
            cx.dispatch_action(&ShowSettings);
        });
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update_window(window, |_, frame, cx| {
            frame.render_frame(cx);
            frame.within("settings-sidebar").click("input-state", cx);
            for key in ["h", "i", "s", "t", "o", "r", "y"] {
                frame.press(key, cx);
            }
            assert!(frame.try_find("appearance-choice").is_none());
            assert!(frame.try_find("remember-reading-history").is_some());
            assert_eq!(frame.find("remember-reading-history").checked(), Some(true));
            frame.within("settings-sidebar").click("input-state", cx);
            frame.press(
                if keymap::platform(cx) == keymap::Platform::Mac {
                    "cmd-a"
                } else {
                    "ctrl-a"
                },
                cx,
            );
            frame.press("backspace", cx);
            frame.within("settings-sidebar").click("0-0", cx);
            assert!(frame.try_find("show-toolbar").is_some());
        })
        .unwrap();
        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
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
                        cx.new(|cx| DocumentWindow::new(None, window, cx))
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
                        assert_eq!(frame.try_find("toggle-sidebar").is_some(), toolbar);
                        assert!(frame.try_find("settings-button").is_none());
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
                    assert!(frame.try_find("toggle-sidebar").is_none());
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
                frame.within("settings-sidebar").click("0-1", cx);
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
                frame.within("settings-sidebar").click("0-0", cx);
                assert_eq!(frame.find("reopen-last-windows").checked(), Some(false));
            })
            .unwrap();
        });
    }

    #[gpui_kit::test]
    fn missing_recent_cleanup_switch_persists_and_tracks_valid_file_edits(cx: &mut TestAppContext) {
        fn search(cx: &mut TestAppContext) -> Vec<crate::recent::RecentFile> {
            let result = std::sync::Arc::new(std::sync::Mutex::new(None));
            let output = result.clone();
            cx.update(|cx| {
                let work = recent::history(cx)
                    .unwrap()
                    .search_excluding(String::new(), None);
                cx.spawn(async move |_| *output.lock().unwrap() = Some(work.await.unwrap()))
                    .detach();
            });
            cx.run_until_parked();
            result.lock().unwrap().take().unwrap()
        }

        let root = tempfile::tempdir().unwrap();
        let paths =
            AppPaths::for_locations(root.path().join("config.toml"), root.path().join("data"));
        let missing = root.path().join("missing.pdf");
        RecentStore::new(&paths).record(&missing).unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            recent::install(cx, Some(paths.clone()));
            install_with_paths(cx, Some(paths.clone()));
            cx.dispatch_action(&ShowSettings);
        });
        cx.run_until_parked();
        assert_eq!(search(cx).len(), 1);
        let window = cx.update(|cx| cx.windows()[0]);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                frame.within("settings-sidebar").click("0-2", cx);
                assert_eq!(
                    frame.find("remove-missing-recent-files").checked(),
                    Some(false)
                );
                frame.click("remove-missing-recent-files", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            settings::load(paths.config())
                .unwrap()
                .remove_missing_recent_files()
        );
        assert!(search(cx).is_empty());
        std::fs::write(
            paths.config(),
            "[persistence]\nremove_missing_recent_files = 'yes'\n",
        )
        .unwrap();
        poll(cx);
        cx.update(|cx| assert!(state(cx).read(cx).remove_missing_recent_files()));
        std::fs::write(
            paths.config(),
            "[persistence]\nremove_missing_recent_files = false\n",
        )
        .unwrap();
        poll(cx);
        cx.update(|cx| drop(recent::history(cx).unwrap().record(missing.clone())));
        assert_eq!(search(cx).len(), 1);
        cx.update(|cx| {
            cx.update_window(window, |_, frame, cx| {
                frame.render_frame(cx);
                assert_eq!(
                    frame.find("remove-missing-recent-files").checked(),
                    Some(false)
                );
                // The clicked switch retains keyboard focus across file edits.
                frame.press("space", cx);
            })
            .unwrap();
        });
        cx.run_until_parked();
        assert!(
            settings::load(paths.config())
                .unwrap()
                .remove_missing_recent_files()
        );
        assert!(search(cx).is_empty());
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
                frame.within("settings-sidebar").click("0-2", cx);
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
                frame.within("settings-sidebar").click("0-2", cx);
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
                frame.within("settings-sidebar").click("0-0-0", cx);
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
        let reader = cx.add_window(|window, cx| DocumentWindow::new(None, window, cx));
        cx.update(|cx| {
            cx.update_window(reader.into(), |_, frame, cx| {
                frame.render_frame(cx);
                frame.press("cmd-,", cx);
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
                frame.within("settings-sidebar").click("0-1", cx);
                assert_eq!(
                    frame.find("initial-zoom-choice").label(),
                    Some("Initial zoom: Fit Width")
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
                    Some("Initial zoom: Fit Page")
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
                    Some("Initial zoom: 100%")
                );
            })
            .unwrap();
        });
    }
}
