//! One application-wide Settings window and its asynchronous preference state.
use std::{cell::Cell, time::Duration};

use gpui_kit as gpui;
use gpui_kit::assets::IconName;
use gpui_kit::base::{StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    sidebar::{Sidebar, SidebarMenu, SidebarMenuItem},
    theme::{Theme, ThemeMode},
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, KeyBinding,
    Render, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    reader_windows::CloseWindow,
    settings::{self, AppPaths, AppSettings, Appearance, SettingsChange},
};

gpui::actions!(kver, [ShowSettings, CloseSettings]);

pub struct SettingsState {
    settings: AppSettings,
    paths: Option<AppPaths>,
    error: Option<String>,
    revision: u64,
    saving: bool,
    pending: Option<SettingsChange>,
}

impl SettingsState {
    fn new(paths: Option<AppPaths>) -> Self {
        Self {
            settings: AppSettings::default(),
            paths,
            error: None,
            revision: 0,
            saving: false,
            pending: None,
        }
    }

    pub fn appearance(&self) -> Appearance {
        self.settings.appearance()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn choose(&mut self, choice: Appearance, cx: &mut Context<Self>) {
        if self.appearance() == choice && self.pending.is_none() {
            return;
        }
        self.revision += 1;
        if self.paths.is_none() {
            self.apply_settings(self.settings.with_appearance(choice), cx);
            self.error = Some(
                "Settings cannot be saved: no user configuration directory is available.".into(),
            );
        } else {
            self.pending = Some(SettingsChange::Appearance(choice));
            self.start_save(cx);
        }
        cx.notify();
    }

    fn apply_settings(&mut self, updated: AppSettings, cx: &mut Context<Self>) {
        if self.settings.appearance() != updated.appearance() {
            updated.appearance().apply(cx);
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
                if revision != this.revision {
                    this.start_save(cx);
                } else {
                    match result {
                        Ok(updated) => {
                            this.apply_settings(updated, cx);
                            this.error = None;
                        }
                        Err(error) => {
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
    cx.bind_keys([
        KeyBinding::new("cmd-,", ShowSettings, None),
        KeyBinding::new("escape", CloseSettings, Some("KverSettings")),
    ]);
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
    let bounds = Bounds::centered(None, size(px(700.), px(360.)), cx);
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
                focus.focus(window, cx);
                let settings = state(cx);
                cx.observe(&settings, |_, _, cx| cx.notify()).detach();
                SettingsView {
                    focus,
                    choice_focus,
                    settings,
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
    settings: Entity<SettingsState>,
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings.read(cx);
        let selected = settings.appearance();
        let error = settings.error.clone();
        let state = self.settings.clone();
        let choice_focus = self.choice_focus.clone();
        let reset_state = self.settings.clone();
        let reset_focus = self.choice_focus.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .key_context("KverSettings")
            .on_action(|_: &CloseSettings, window, _| window.remove_window())
            .on_action(|_: &CloseWindow, window, _| window.remove_window())
            .child(TitleBar::new().child(div().pl_16().font_medium().child("Settings")))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(
                        Sidebar::new("settings-sidebar")
                            .w_48()
                            .h_full()
                            .collapsible(false)
                            .collapsed(false)
                            .child(
                                SidebarMenu::new().child(
                                    SidebarMenuItem::new("Appearance")
                                        .active(true)
                                        .default_open(true)
                                        .click_to_open(true)
                                        .on_click({
                                            let focus = choice_focus.clone();
                                            move |_, window, cx| window.focus(&focus, cx)
                                        })
                                        .children([SidebarMenuItem::new("Theme").on_click({
                                            let focus = choice_focus.clone();
                                            move |_, window, cx| window.focus(&focus, cx)
                                        })]),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .p_4()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .font_medium()
                                    .child("Appearance"),
                            )
                            .child(
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
                                            .child("Theme"),
                                    )
                                    .child(
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
                                                                    reset_state.update(cx, |settings, cx| settings.choose(Appearance::System, cx));
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
                                                                                    state.update(cx, |settings, cx| settings.choose(choice, cx));
                                                                                }),
                                                                        );
                                                                    }
                                                                    menu
                                                                },
                                                            ),
                                                    ),
                                            ),
                                    ),
                            )
                            .when_some(error, |view, message| {
                                view.child(
                                    div()
                                        .id("settings-error")
                                        .test_support()
                                        .role(gpui::Role::Status)
                                        .aria_label(message.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
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
                assert_eq!(frame.find("appearance-choice").focused(), Some(true));
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
            crate::reader::bind_reader_keys(cx);
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
}
