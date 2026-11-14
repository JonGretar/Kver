//! First-launch preferences, completed before document windows are opened.

use std::path::PathBuf;

use gpui_kit as gpui;
use gpui_kit::base::{Disableable as _, StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    theme::{Theme, ThemeMode},
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, Entity, FocusHandle, KeyBinding,
    Render, Window, WindowBounds, WindowOptions, div, img, prelude::*, px, size,
};

use super::keymap::Preset;
use crate::settings::{self, Appearance, ReadingView, SettingsChange};

gpui::actions!(onboarding, [Finish, Close, Quit]);

/// Open a fixed-size Welcome window. The receiver resolves when it closes.
/// The caller creates the empty config first, then opens the reader afterwards.
pub(super) fn open(
    config: PathBuf,
    cx: &mut App,
) -> gpui_kit::Result<(AnyWindowHandle, async_channel::Receiver<()>)> {
    cx.bind_keys([
        KeyBinding::new("enter", Finish, Some("Onboarding")),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-enter"
            } else {
                "ctrl-enter"
            },
            Finish,
            Some("Onboarding"),
        ),
        KeyBinding::new("escape", Close, Some("Onboarding")),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-w"
            } else {
                "ctrl-w"
            },
            Close,
            Some("Onboarding"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            Quit,
            Some("Onboarding"),
        ),
    ]);
    #[cfg(target_os = "macos")]
    cx.set_menus(vec![
        gpui::Menu::new("Kver").items([gpui::MenuItem::action("Quit Kver", Quit)]),
    ]);
    let (finished, receiver) = async_channel::bounded(1);
    // Native window bounds are platform pixels; content uses the rem scale.
    let bounds = Bounds::centered(None, size(px(560.), px(720.)), cx);
    let (handle, _) = gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_resizable: false,
            is_minimizable: false,
            ..TitleBar::window_options()
        },
        cx,
        |window, cx| {
            let welcome = cx.new(|cx| Welcome::new(config, window, cx));
            let weak = welcome.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.upgrade()
                    .is_none_or(|welcome| !welcome.read(cx).saving)
            });
            welcome
        },
    )?;
    // Entity release can lag behind close; startup follows the window's lifetime.
    cx.on_window_closed(move |_, id| {
        if id == handle.window_id() {
            let _ = finished.try_send(());
        }
    })
    .detach();
    cx.update_window(handle, |_, window, _| window.activate_window())?;
    Ok((handle, receiver))
}

struct Welcome {
    config: PathBuf,
    focus: FocusHandle,
    author: Entity<InputState>,
    appearance: Appearance,
    preset: Preset,
    reading_view: ReadingView,
    saving: bool,
    error: Option<String>,
}

impl Welcome {
    fn new(config: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            config,
            focus,
            author: cx.new(|cx| InputState::new(window, cx).placeholder("Your name (optional)")),
            appearance: Appearance::default(),
            preset: Preset::default(),
            reading_view: ReadingView::default(),
            saving: false,
            error: None,
        }
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        self.saving = true;
        self.error = None;
        let path = self.config.clone();
        let changes = [
            SettingsChange::Appearance(self.appearance),
            SettingsChange::KeyboardPreset(self.preset),
            SettingsChange::AnnotationAuthor(self.author.read(cx).value().to_string()),
            SettingsChange::ReadingView(self.reading_view),
        ];
        let work = cx
            .background_executor()
            .spawn(async move { settings::save_changes(&path, changes) });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(_) => window.remove_window(),
                    Err(error) => {
                        this.error = Some(format!("{error}. Check the file and try again."));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn close(&self, window: &mut Window, cx: &mut App) {
        if !self.saving {
            // Discard the unsaved appearance preview along with the other choices.
            Theme::sync_system_appearance(None, cx);
            window.remove_window();
        }
    }
}

fn field(label: &'static str, control: impl IntoElement) -> gpui_kit::Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_sm().font_semibold().child(label))
        .child(control)
}

fn visual_choice(
    id: &'static str,
    label: &'static str,
    selected: bool,
    disabled: bool,
    preview: impl IntoElement,
    cx: &App,
) -> Radio {
    Radio::new(id)
        .label(label)
        .flex_1()
        .min_w_0()
        .p_2()
        .border_1()
        .border_color(if selected {
            cx.theme().primary
        } else {
            cx.theme().border
        })
        .rounded(cx.theme().radius)
        .bg(if selected {
            cx.theme().primary.opacity(0.06)
        } else {
            cx.theme().background
        })
        .when(!disabled, |radio| {
            radio.hover(|radio| radio.border_color(cx.theme().primary))
        })
        .child(preview)
}

/// A miniature reader window, drawn from the installed theme rather than a second palette.
fn appearance_preview(
    appearance: Appearance,
    light: &Theme,
    dark: &Theme,
    cx: &App,
) -> gpui_kit::Div {
    let half = |theme: &Theme, leading: bool, trailing: bool| {
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme.muted)
            .when(leading, |half| half.rounded_l(cx.theme().radius))
            .when(trailing, |half| half.rounded_r(cx.theme().radius))
            .child(div().h_3().border_b_1().border_color(theme.border))
            .child(
                div().flex_1().flex().items_center().justify_center().child(
                    div()
                        .w_6()
                        .h_8()
                        .p_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .bg(theme.background)
                        .child(div().h_1().w_full().bg(theme.foreground.opacity(0.3)))
                        .child(div().h_1().w_full().bg(theme.foreground.opacity(0.15)))
                        .child(div().h_1().w_3().bg(theme.foreground.opacity(0.15))),
                ),
            )
    };
    div()
        .h_16()
        .w_full()
        .mt_1()
        .flex()
        .items_stretch()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .overflow_hidden()
        .when(appearance != Appearance::Dark, |preview| {
            preview.child(half(light, true, appearance == Appearance::Light))
        })
        .when(appearance != Appearance::Light, |preview| {
            preview.child(half(dark, appearance == Appearance::Dark, true))
        })
}

fn reading_preview(view: ReadingView, cx: &App) -> gpui_kit::Div {
    let page = || {
        div()
            .w_10()
            .h_12()
            .flex_shrink_0()
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .child(div().h_1().w_full().bg(cx.theme().foreground.opacity(0.3)))
            .child(div().h_1().w_full().bg(cx.theme().foreground.opacity(0.15)))
            .child(div().h_1().w_3().bg(cx.theme().foreground.opacity(0.15)))
    };
    div()
        .w_full()
        .h_16()
        .mt_1()
        .flex()
        .justify_center()
        .bg(cx.theme().muted)
        .rounded(cx.theme().radius)
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .when(view == ReadingView::Page, |pages| {
                    pages.h_full().justify_center()
                })
                .child(page())
                .when(view == ReadingView::Continuous, |pages| pages.child(page())),
        )
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const APPEARANCES: [Appearance; 3] =
            [Appearance::System, Appearance::Light, Appearance::Dark];
        const PRESETS: [Preset; 2] = [Preset::Standard, Preset::Vim];
        const VIEWS: [ReadingView; 2] = [ReadingView::Continuous, ReadingView::Page];
        let mut light = cx.theme().clone();
        light.apply_config(&cx.theme().light_theme);
        let mut dark = cx.theme().clone();
        dark.apply_config(&cx.theme().dark_theme);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus)
            .key_context("Onboarding")
            .tab_group()
            .on_action(cx.listener(|this, _: &Finish, window, cx| this.finish(window, cx)))
            .on_action(cx.listener(|this, _: &Close, window, cx| this.close(window, cx)))
            .on_action(|_: &Quit, _, cx| cx.quit())
            .child(TitleBar::new())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_8()
                    .py_4()
                    .bg(cx.theme().primary.opacity(0.08))
                    .child(img("images/kver.png").size_12())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_xl().font_semibold().child("Welcome to Kver"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("A few choices. Then just read."),
                            ),
                    ),
            )
            .child(
                div()
                    .id("onboarding-form")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .px_8()
                            .py_6()
                            .child(field(
                                "Appearance",
                                RadioGroup::horizontal("onboarding-appearance")
                                    .children(APPEARANCES.map(|appearance| {
                                        visual_choice(
                                            match appearance {
                                                Appearance::Light => "appearance-light",
                                                Appearance::Dark => "appearance-dark",
                                                Appearance::System => "appearance-system",
                                            },
                                            if appearance == Appearance::System {
                                                "Follow system"
                                            } else {
                                                appearance.label()
                                            },
                                            self.appearance == appearance,
                                            self.saving,
                                            appearance_preview(appearance, &light, &dark, cx),
                                            cx,
                                        )
                                    }))
                                    .selected_index(
                                        APPEARANCES
                                            .iter()
                                            .position(|value| *value == self.appearance),
                                    )
                                    .disabled(self.saving)
                                    .on_change(cx.listener(|this, ix: &usize, _, cx| {
                                        this.appearance = APPEARANCES[*ix];
                                        match this.appearance {
                                            Appearance::System => {
                                                Theme::sync_system_appearance(None, cx)
                                            }
                                            Appearance::Light => {
                                                Theme::change(ThemeMode::Light, None, cx)
                                            }
                                            Appearance::Dark => {
                                                Theme::change(ThemeMode::Dark, None, cx)
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ))
                            .child(field(
                                "Default reading view",
                                RadioGroup::horizontal("onboarding-reading-view")
                                    .children(VIEWS.map(|view| {
                                        visual_choice(
                                            match view {
                                                ReadingView::Continuous => "view-continuous",
                                                ReadingView::Page => "view-page",
                                            },
                                            match view {
                                                ReadingView::Continuous => "Continuous",
                                                ReadingView::Page => "Page",
                                            },
                                            self.reading_view == view,
                                            self.saving,
                                            reading_preview(view, cx),
                                            cx,
                                        )
                                    }))
                                    .selected_index(
                                        VIEWS.iter().position(|value| *value == self.reading_view),
                                    )
                                    .disabled(self.saving)
                                    .on_change(cx.listener(|this, ix: &usize, _, cx| {
                                        this.reading_view = VIEWS[*ix];
                                        cx.notify();
                                    })),
                            ))
                            .child(field(
                                "Keyboard map",
                                RadioGroup::horizontal("onboarding-keyboard")
                                    .children(PRESETS.map(|preset| {
                                        let (id, label, description) = match preset {
                                            Preset::Standard => (
                                                "keyboard-standard",
                                                "OS-style",
                                                "Use familiar shortcuts and arrow keys to navigate.",
                                            ),
                                            Preset::Vim => (
                                                "keyboard-vim",
                                                "Vim-style",
                                                "Use Vim shortcuts like h, j, k, and l to navigate.",
                                            ),
                                        };
                                        visual_choice(
                                            id,
                                            label,
                                            self.preset == preset,
                                            self.saving,
                                            div()
                                                .pr_6()
                                                .text_sm()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(description),
                                            cx,
                                        )
                                    }))
                                    .selected_index(
                                        PRESETS.iter().position(|value| *value == self.preset),
                                    )
                                    .disabled(self.saving)
                                    .on_change(cx.listener(|this, ix: &usize, _, cx| {
                                        this.preset = PRESETS[*ix];
                                        cx.notify();
                                    })),
                            ))
                            .child(field(
                                "Annotation author",
                                Input::new(&self.author)
                                    .id("onboarding-author")
                                    .aria_label("Annotation author")
                                    .disabled(self.saving),
                            )),
                    ),
            )
            .when_some(self.error.clone(), |content, error| {
                content.child(
                    div()
                        .id("onboarding-error")
                        .test_support()
                        .role(gpui::Role::Status)
                        .aria_label(error.clone())
                        .px_8()
                        .pb_4()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .px_8()
                    .pb_6()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("You can change these in Settings."),
                    )
                    .child(
                        Button::new("onboarding-start")
                            .flex_shrink_0()
                            .primary()
                            .large()
                            .label(if self.saving {
                                "Saving…"
                            } else {
                                "Start reading"
                            })
                            .disabled(self.saving)
                            .on_click(cx.listener(|this, _, window, cx| this.finish(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        TestAppContext,
        test::{TestQueryExt as _, TestWindowExt as _},
    };

    #[gpui_kit::test]
    fn form_alignment_and_commit_remain_usable_at_larger_ui_sizes(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let (handle, _) = cx.update(|cx| {
            super::super::initialize_ui(cx);
            open(root.path().join("config.toml"), cx).unwrap()
        });
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            for font_size in [14., 16., 20.] {
                cx.update(|cx| {
                    Theme::change(mode, None, cx);
                    Theme::update(cx, |theme| theme.font_size = px(font_size));
                });
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    let author = window.find("onboarding-author").bounds();
                    let appearance = window.find_by_label("Follow system").bounds();
                    let light = window.find_by_label("Light").bounds();
                    let dark = window.find_by_label("Dark").bounds();
                    let keyboard = window.find_by_label("OS-style").bounds();
                    let vim = window.find_by_label("Vim-style").bounds();
                    let view = window.find_by_label("Continuous").bounds();
                    assert!(appearance.right() <= light.left());
                    assert!(light.right() <= dark.left());
                    assert_eq!(appearance.left(), author.left());
                    assert_eq!(keyboard.left(), author.left());
                    assert_eq!(vim.right(), author.right());
                    assert!((keyboard.size.width - vim.size.width).abs() <= px(1.));
                    assert_eq!(keyboard.size.height, vim.size.height);
                    assert_eq!(view.left(), author.left());
                    assert!(appearance.bottom() < view.top());
                    assert!(view.bottom() < keyboard.top());
                    assert!(keyboard.bottom() < author.top());
                    let commit = window.find("onboarding-start").bounds();
                    assert!(commit.right() <= window.viewport_size().width);
                    assert!(commit.bottom() <= window.viewport_size().height);
                    // At larger text sizes the form, not the commit, scrolls.
                    window.scroll(
                        "onboarding-form",
                        gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-1000.))),
                        cx,
                    );
                    assert!(window.find("onboarding-author").visible());
                    assert!(commit.top() > window.find("onboarding-author").bounds().bottom());
                })
                .unwrap();
            }
        }
    }

    #[gpui_kit::test]
    fn choices_preview_and_save_together_before_continuing(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        assert!(settings::create_if_missing(&path).unwrap());
        let (handle, finished) = cx.update(|cx| {
            super::super::initialize_ui(cx);
            open(path.clone(), cx).unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("tab", cx);
            assert_eq!(window.find_by_label("Follow system").focused(), Some(true));
            window.press("tab", cx);
            window.press("tab", cx);
            window.press("space", cx);
            assert_eq!(window.find_by_label("Dark").focused(), Some(true));
            assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
            window.press("tab", cx);
            window.press("tab", cx);
            window.press("space", cx);
            assert_eq!(window.find_by_label("Page").focused(), Some(true));
            window.press("tab", cx);
            window.press("tab", cx);
            window.press("space", cx);
            assert_eq!(window.find_by_label("Vim-style").focused(), Some(true));
            window.press("tab", cx);
            window.input("Jón 中文", cx);
            assert_eq!(window.find("onboarding-author").value(), Some("Jón 中文"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
            window.press("tab", cx);
            assert_eq!(window.find("onboarding-start").focused(), Some(true));
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(
            finished.try_recv().is_ok(),
            "windows: {:?}, config: {:?}",
            cx.update(|cx| cx.windows()),
            std::fs::read_to_string(&path)
        );
        assert!(cx.update(|cx| cx.windows().is_empty()));
        let saved = settings::load(&path).unwrap();
        assert_eq!(saved.appearance(), Appearance::Dark);
        assert_eq!(saved.keyboard_preset(), Preset::Vim);
        assert_eq!(saved.annotation_author(), "Jón 中文");
        assert_eq!(saved.reading_view(), ReadingView::Page);
        assert!(!settings::create_if_missing(&path).unwrap());
    }

    #[gpui_kit::test]
    fn close_leaves_empty_config_and_save_failure_keeps_form(cx: &mut TestAppContext) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        settings::create_if_missing(&path).unwrap();
        let (handle, finished) = cx.update(|cx| {
            super::super::initialize_ui(cx);
            open(path.clone(), cx).unwrap()
        });
        // A hand edit made during onboarding must never be overwritten if invalid.
        std::fs::write(&path, "[broken").unwrap();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("onboarding-author", cx);
            window.input("Keep my name", cx);
            window.click("onboarding-start", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(finished.try_recv().is_err());
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("onboarding-error").visible());
            assert_eq!(
                window.find("onboarding-author").value(),
                Some("Keep my name")
            );
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[broken");
        std::fs::write(&path, "").unwrap();
        cx.update_window(handle, |_, window, cx| window.press("escape", cx))
            .unwrap();
        cx.run_until_parked();
        assert!(finished.try_recv().is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        assert!(!settings::create_if_missing(&path).unwrap());
    }
}
