//! GPUI Kit owns the settings layout; Kver owns preference persistence and actions.
use super::*;
use gpui_kit::AnyElement;
use gpui_kit::component::{
    group_box::GroupBoxVariant,
    input::{Input, InputEvent, InputState},
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};

pub(super) struct SettingsView {
    focus: FocusHandle,
    appearance_focus: FocusHandle,
    preset_focus: FocusHandle,
    zoom_focus: FocusHandle,
    settings: Entity<SettingsState>,
    author: Entity<InputState>,
    clearing: bool,
    clear_error: Option<String>,
}

impl SettingsView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = state(cx);
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        let author = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(settings.read(cx).annotation_author())
                .placeholder("No author")
        });
        cx.subscribe_in(&author, window, |this, input, event, _, cx| {
            if matches!(event, InputEvent::Change) {
                let author = input.read(cx).value().to_string();
                this.settings.update(cx, |settings, cx| {
                    settings.choose(SettingsChange::AnnotationAuthor(author), cx);
                });
            }
        })
        .detach();
        cx.observe_in(&settings, window, {
            let mut previous = settings.read(cx).annotation_author().to_owned();
            let author = author.clone();
            move |_, settings, window, cx| {
                let next = settings.read(cx).annotation_author();
                if next != previous {
                    previous = next.to_owned();
                    if author.read(cx).value().as_str() != next {
                        let next = next.to_owned();
                        author.update(cx, |input, cx| input.set_value(next, window, cx));
                    }
                }
            }
        })
        .detach();
        let focus = cx.focus_handle().tab_stop(true);
        let appearance_focus = cx.focus_handle().tab_stop(true);
        let preset_focus = cx.focus_handle().tab_stop(true);
        let zoom_focus = cx.focus_handle().tab_stop(true);
        window.focus(&focus, cx);
        Self {
            focus,
            appearance_focus,
            preset_focus,
            zoom_focus,
            settings,
            author,
            clearing: false,
            clear_error: None,
        }
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

fn toggle(
    title: &'static str,
    description: &'static str,
    id: &'static str,
    value: impl Fn(&SettingsState) -> bool + 'static,
    change: fn(bool) -> SettingsChange,
) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::render(move |options, _, cx| {
            let checked = value(state(cx).read(cx));
            Switch::new(id)
                .accessibility_label(title)
                .checked(checked)
                .disabled(options.is_disabled())
                .with_size(options.size())
                .on_change(move |next, _, cx| {
                    state(cx).update(cx, |settings, cx| settings.choose(change(*next), cx));
                })
        }),
    )
    .description(description)
}

struct Choice<T: 'static> {
    title: &'static str,
    description: &'static str,
    id: &'static str,
    values: &'static [T],
    label: fn(T) -> &'static str,
    current: fn(&SettingsState) -> T,
    change: fn(T) -> SettingsChange,
    reset: Option<(T, &'static str)>,
    focus: Option<FocusHandle>,
}

fn choice<T: Copy + Eq + 'static>(spec: Choice<T>) -> SettingItem {
    let Choice {
        title,
        description,
        id,
        values,
        label,
        current,
        change,
        reset,
        focus,
    } = spec;
    SettingItem::new(
        title,
        SettingField::render(move |options, _, cx| {
            let selected = current(state(cx).read(cx));
            let focus_label = format!("{title}: {}", label(selected));
            let reset_focus = focus.clone();
            div()
                .flex()
                .items_center()
                .gap_2()
                .when(
                    reset.is_some_and(|(default, _)| default != selected),
                    |row| {
                        let (default, reset_id) = reset.unwrap();
                        row.child(
                            Button::new(reset_id)
                                .ghost()
                                .with_size(options.size())
                                .icon(IconName::Undo2)
                                .accessibility_label(format!(
                                    "Reset {} to {}",
                                    title.to_lowercase(),
                                    label(default)
                                ))
                                .tooltip(format!(
                                    "Reset {} to {}",
                                    title.to_lowercase(),
                                    label(default)
                                ))
                                .on_click(move |_, window, cx| {
                                    state(cx).update(cx, |settings, cx| {
                                        settings.choose(change(default), cx)
                                    });
                                    if let Some(focus) = &reset_focus {
                                        window.focus(focus, cx);
                                    }
                                }),
                        )
                    },
                )
                .child(
                    Button::new(id)
                        .outline()
                        .with_size(options.size())
                        .label(label(selected))
                        .accessibility_label(focus_label)
                        .when_some(focus.clone(), |button, focus| button.track_focus(&focus))
                        .dropdown_caret(true)
                        .dropdown_menu_with_anchor(
                            gpui::Anchor::TopRight,
                            move |mut menu, _, _| {
                                for option in values {
                                    let option = *option;
                                    menu = menu.item(
                                        PopupMenuItem::new(label(option))
                                            .checked(option == selected)
                                            .on_click(move |_, _, cx| {
                                                state(cx).update(cx, |settings, cx| {
                                                    settings.choose(change(option), cx)
                                                });
                                            }),
                                    );
                                }
                                menu
                            },
                        ),
                )
        }),
    )
    .description(description)
}

fn status(id: &'static str, message: String, cx: &App) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .role(gpui::Role::Status)
        .aria_label(message.clone())
        .text_sm()
        .text_color(cx.theme().danger)
        .child(message)
        .into_any_element()
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings.read(cx);
        let preset = settings.keyboard_preset();
        let clear_owner = cx.weak_entity();
        let clearing = self.clearing;
        let author = self.author.clone();
        let mut history = SettingGroup::new()
            .title("Reading history")
            .item(SettingItem::new(
                "Remember reading history",
                SettingField::render(|options, _, cx| {
                    Switch::new("remember-reading-history")
                        .accessibility_label("Remember reading history")
                        .checked(state(cx).read(cx).remember_reading_history())
                        .disabled(options.is_disabled())
                        .with_size(options.size())
                        .on_change(|next, _, cx| {
                            state(cx).update(cx, |settings, cx| settings.choose_history(*next, cx));
                        })
                }),
            ).description("Keep recently opened PDFs on this device. Turning this off does not erase saved data."))
            .item(toggle(
                "Reopen at last position",
                "Resume a PDF where you stopped. Turning this off leaves the saved place intact.",
                "reopen-at-last-position",
                SettingsState::reopen_at_last_position,
                SettingsChange::ReopenAtLastPosition,
            ))
            .item(
                SettingItem::new(
                    "Erase reading history",
                    SettingField::render(move |options, _, _| {
                        let owner = clear_owner.clone();
                        Button::new("clear-reading-history")
                            .with_size(options.size())
                            .outline()
                            .label(if clearing { "Clearing…" } else { "Clear…" })
                            .disabled(clearing)
                            .on_click(move |_, window, cx| {
                                let owner = owner.clone();
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
                            })
                    }),
                )
                .description("Erase saved data for every PDF. PDF files and app preferences stay unchanged."),
            );
        if let Some(message) = self.clear_error.clone() {
            history =
                history.footer(move |_, cx| status("clear-history-error", message.clone(), cx));
        }
        let keyboard = SettingGroup::new()
            .title("Keyboard")
            .item(choice(Choice {
                title: "Keyboard preset",
                description: "Changes shortcuts, not commands.",
                id: "keyboard-preset-choice",
                values: &[Preset::Standard, Preset::Vim],
                label: Preset::label,
                current: SettingsState::keyboard_preset,
                change: SettingsChange::KeyboardPreset,
                reset: Some((Preset::Standard, "keyboard-preset-reset")),
                focus: Some(self.preset_focus.clone()),
            }))
            .item(
                SettingItem::render(move |_, _, cx| shortcut_help(cx, preset)).keywords([
                    "shortcuts",
                    "keybindings",
                    "keys",
                ]),
            );
        // The shortcut reference reflects the live keymap, including user overrides.

        let persistence = SettingPage::new("Persistence")
            .resettable(false)
            .group(history);
        let panel = Settings::new("kver-settings")
            .with_group_variant(GroupBoxVariant::Normal)
            .pages([
                SettingPage::new("Application")
                    .icon(IconName::Settings)
                    .default_open(true)
                    .resettable(false)
                    .groups([
                        SettingGroup::new().title("Theme").item(choice(Choice {
                            title: "Appearance",
                            description: "Follow the system, or always use light or dark mode.",
                            id: "appearance-choice",
                            values: &[Appearance::System, Appearance::Light, Appearance::Dark],
                            label: Appearance::label,
                            current: SettingsState::appearance,
                            change: SettingsChange::Appearance,
                            reset: Some((Appearance::System, "appearance-reset")),
                            focus: Some(self.appearance_focus.clone()),
                        })),
                        SettingGroup::new().title("Annotations").item(
                            SettingItem::new("Annotation author", SettingField::render(move |options, _, _| {
                                Input::new(&author)
                                    .id("annotation-author")
                                    .aria_label("Annotation author")
                                    .with_size(options.size())
                                    .disabled(options.is_disabled())
                                    .w_48()
                            }))
                            .description("Used only for new annotations. Leave blank to omit your name; existing authors stay unchanged."),
                        ),
                        SettingGroup::new()
                            .title("UI elements")
                            .items([
                                toggle("Show toolbar", "Show navigation and zoom controls in reader windows.", "show-toolbar", SettingsState::show_toolbar, SettingsChange::ShowToolbar),
                                toggle("Show statusbar", "Show page and zoom information at the bottom of reader windows.", "show-statusbar", SettingsState::show_statusbar, SettingsChange::ShowStatusbar),
                            ]),
                        SettingGroup::new()
                            .title("Windows")
                            .items([
                                toggle("Quit when last window closes", "When off, Kver stays open without a window on macOS. Use New Window or open a PDF to continue.", "quit-on-last-close", SettingsState::quit_when_last_window_closes, SettingsChange::QuitWhenLastWindowCloses),
                                toggle("Reopen last windows on startup", "Saved for a future release. Restoring windows is not implemented yet.", "reopen-last-windows", |settings| settings.settings.reopen_last_windows_on_startup(), SettingsChange::ReopenLastWindowsOnStartup),
                            ]),
                    ]),
                SettingPage::new("Reader")
                    .resettable(false)
                    .group(SettingGroup::new().title("Reading experience").items([
                        choice(Choice {
                            title: "Initial zoom",
                            description: "Set how newly opened PDFs fit the window. Open readers keep their zoom.",
                            id: "initial-zoom-choice",
                            values: &[InitialZoom::ActualSize, InitialZoom::FitWidth, InitialZoom::FitPage],
                            label: InitialZoom::label,
                            current: SettingsState::initial_zoom,
                            change: SettingsChange::InitialZoom,
                            reset: None,
                            focus: Some(self.zoom_focus.clone()),
                        }),
                        choice(Choice {
                            title: "Reading view",
                            description: "Set the layout for newly opened PDFs. Open readers keep their view.",
                            id: "reading-view-choice",
                            values: &[ReadingView::Continuous, ReadingView::Page],
                            label: ReadingView::label,
                            current: SettingsState::reading_view,
                            change: SettingsChange::ReadingView,
                            reset: None,
                            focus: None,
                        }),
                    ])),
                persistence,
                SettingPage::new("Keyboard")
                    .resettable(false)
                    .group(keyboard),
            ]);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .key_context(crate::key_context::SETTINGS)
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
                        .w(window.viewport_size().width)
                        .h(gpui_kit::component::TITLE_BAR_HEIGHT)
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_medium()
                        .child("Settings"),
                ),
            )
            .child(div().flex_1().min_h_0().child(panel))
            .when_some(settings.error.clone(), |view, message| {
                view.child(
                    div()
                        .px_4()
                        .py_2()
                        .child(status("settings-error", message, cx)),
                )
            })
            .track_focus(&self.focus)
    }
}
