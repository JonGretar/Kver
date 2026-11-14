//! Window-local annotation targeting. Mutations stay in the shared session.
use super::annotations::{Excerpt, excerpt_text, marked_excerpt};
use super::*;
use crate::pdf::annotations::{Annotation, AnnotationId, HighlightColor};
use gpui_kit::{
    base::{Dialog, DialogPopup, Disableable, StyledExt as _},
    component::{
        Sizable, ThemeStyled as _,
        button::ButtonVariants,
        command::{Command, CommandItem, CommandState},
        menu::DropdownMenu as _,
    },
};
use std::collections::HashMap;

pub(super) struct AnnotationChoice {
    command: Entity<CommandState>,
    focus: FocusHandle,
    candidates: Vec<Annotation>,
    excerpts: HashMap<AnnotationId, Excerpt>,
    _extraction: Option<gpui::Task<()>>,
}

impl ReaderView {
    pub(crate) fn selected_page_annotation(&self, cx: &App) -> Option<Annotation> {
        let id = self.selected_annotation?;
        self.session
            .as_ref()?
            .read(cx)
            .annotations()?
            .ok()?
            .iter()
            .find(|a| a.id() == id)
            .cloned()
    }

    pub(crate) fn annotation_edit_available(&self, cx: &App) -> bool {
        self.annotation_choice.is_none()
            && self.annotation_press.is_none()
            && self.annotation_editing_rejection(cx).is_none()
            && self
                .selected_page_annotation(cx)
                .is_some_and(|a| !a.is_readonly())
    }

    fn select_page_annotation(&mut self, id: AnnotationId, cx: &mut Context<Self>) {
        self.selected_annotation = Some(id);
        self.annotation_identification = self.selected_page_annotation(cx).and_then(|annotation| {
            self.document
                .as_ref()?
                .annotation_bounds(&annotation)
                .map(|bounds| (annotation.id(), annotation.page(), bounds))
        });
        self.status = "Annotation selected".into();
        cx.notify();
    }

    pub(super) fn finish_annotation_click(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((page, _, dragged)) = self.annotation_press.take() else {
            return;
        };
        // Keep the selected controls mounted until the gesture finishes: removing
        // them on mouse-down would move the PDF underneath a text drag.
        self.selected_annotation = None;
        cx.notify();
        if dragged {
            return;
        }
        let Some(at) = self.pointer_document(page, position) else {
            return;
        };
        let Some(catalog) = self
            .session
            .as_ref()
            .and_then(|session| session.read(cx).annotations())
            .and_then(Result::ok)
        else {
            return;
        };
        let start = catalog.partition_point(|a| a.page() < page);
        let end = catalog.partition_point(|a| a.page() <= page);
        let candidates: Vec<_> = catalog[start..end]
            .iter()
            .filter(|a| a.quads().iter().any(|q| q.contains(at)))
            .cloned()
            .collect();
        match candidates.as_slice() {
            [] => {}
            [annotation] => {
                self.selection.clear();
                self.select_page_annotation(annotation.id(), cx);
            }
            _ => {
                self.selection.clear();
                self.open_annotation_choice(candidates, window, cx);
            }
        }
    }

    pub(super) fn open_annotation_choice(
        &mut self,
        candidates: Vec<Annotation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = cx.new(|cx| CommandState::new(window, cx));
        command.update(cx, |command, cx| command.focus(window, cx));
        let generation = self.document_generation;
        let extraction = self
            .document
            .clone()
            .zip(candidates.first().map(Annotation::page))
            .map(|(document, page)| {
                let candidates = candidates.clone();
                let command = command.clone();
                cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_spawn(
                            async move { document.annotation_excerpts(page, &candidates) },
                        )
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.document_generation != generation {
                            return;
                        }
                        let Some(choice) = this
                            .annotation_choice
                            .as_mut()
                            .filter(|choice| choice.command == command)
                        else {
                            return;
                        };
                        choice.excerpts = match result {
                            Ok(excerpts) => excerpts
                                .into_iter()
                                .map(|(id, text)| {
                                    (id, text.map(Excerpt::Text).unwrap_or(Excerpt::Missing))
                                })
                                .collect(),
                            Err(_) => choice
                                .candidates
                                .iter()
                                .map(|a| (a.id(), Excerpt::Failed))
                                .collect(),
                        };
                        cx.notify();
                    });
                })
            });
        self.annotation_choice = Some(AnnotationChoice {
            command,
            focus: cx.focus_handle(),
            candidates,
            excerpts: HashMap::new(),
            _extraction: extraction,
        });
        cx.notify();
    }

    pub(super) fn edit_selected_highlight(
        &mut self,
        color: Option<HighlightColor>,
        cx: &mut Context<Self>,
    ) {
        if !self.annotation_edit_available(cx) {
            return;
        }
        let Some(id) = self.selected_annotation else {
            return;
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        match session.update(cx, |session, cx| session.edit_annotation(id, color, cx)) {
            Ok(()) if color.is_none() => {
                self.selected_annotation = None;
                self.annotation_identification = None;
                self.status = "Annotation deleted".into();
            }
            Ok(()) => self.status = "Annotation recolored".into(),
            Err(error) => self.status = error,
        }
        cx.notify();
    }

    pub(crate) fn edit_annotation_from_list(
        &mut self,
        edit: &super::AnnotationEdit,
        cx: &mut Context<Self>,
    ) {
        if edit.generation != self.document_generation {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        if session.read(cx).revision() != edit.revision {
            return;
        }
        match session.update(cx, |session, cx| {
            session.edit_annotation(edit.id, edit.color, cx)
        }) {
            Ok(()) if edit.color.is_none() => {
                self.status = "Annotation deleted".into();
            }
            Ok(()) => self.status = "Annotation recolored".into(),
            Err(error) => self.status = error,
        }
        cx.notify();
    }

    pub(super) fn finish_pointer_gesture(&mut self, cx: &mut Context<Self>) {
        self.selection.finish();
        if self.annotation_press.take().is_some() {
            // Releasing outside the PDF is still the end of a text gesture,
            // never a deferred annotation click or a permanently disabled edit.
            self.selected_annotation = None;
            self.annotation_identification = None;
            cx.notify();
        }
    }

    pub(super) fn deselect_annotation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_annotation = None;
        self.annotation_identification = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn render_annotation_controls(&self, cx: &Context<Self>) -> impl IntoElement {
        let annotation = self.selected_page_annotation(cx);
        let editable = self.annotation_edit_available(cx);
        let current_color = annotation.as_ref().map(Annotation::color);
        let color_name = annotation
            .as_ref()
            .map_or_else(|| "Unspecified".to_owned(), color_label);
        div()
            .id("selected-highlight-controls")
            .test_support()
            .key_context(crate::key_context::READER)
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .min_w_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Selected annotation"),
            )
            .when_some(annotation.as_ref(), |strip, annotation| {
                let label = annotation.kind().label();
                strip.child(
                    div()
                        .id("selected-annotation-type")
                        .test_support()
                        .aria_label(annotation.kind().label())
                        .child(
                            gpui_kit::component::Icon::new(annotations::markup_icon(
                                annotation.kind(),
                            ))
                            .small(),
                        )
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                        }),
                )
            })
            .child(
                Button::new("recolor-highlight")
                    .accessibility_label(format!("Selected annotation color: {color_name}"))
                    .tooltip(format!("Change selected annotation color: {color_name}"))
                    .ghost()
                    .small()
                    .min_w_0()
                    .max_w_full()
                    .disabled(!editable)
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .size_3()
                                    .flex_shrink_0()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    // The swatch is PDF data, including imported colors.
                                    .when_some(
                                        current_color.and_then(|c| c.rgb()),
                                        |swatch, [r, g, b]| {
                                            swatch.bg(gpui_kit::Rgba { r, g, b, a: 1. })
                                        },
                                    ),
                            )
                            .child(div().min_w_0().truncate().text_sm().child(color_name)),
                    )
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        let mut menu = menu.check_side(gpui_kit::component::Side::Right);
                        for color in HighlightColor::ALL {
                            menu = menu.item(
                                PopupMenuItem::new(color.label())
                                    .icon(annotations::markup_color_icon(color))
                                    .checked(current_color == Some(color.color()))
                                    .action(Box::new(RecolorHighlight(color))),
                            );
                        }
                        menu
                    }),
            )
            .child(
                Button::new("delete-highlight")
                    .label("Delete")
                    .icon(IconName::Trash)
                    .accessibility_label("Delete annotation")
                    .ghost()
                    .small()
                    .disabled(!editable)
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(DeleteHighlight), cx);
                    })),
            )
            .child(
                Button::new("deselect-annotation")
                    .icon(IconName::Close)
                    .ml_auto()
                    .accessibility_label("Deselect annotation")
                    .tooltip("Deselect annotation (Escape)")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.deselect_annotation(window, cx);
                    })),
            )
    }

    fn dismiss_annotation_choice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.annotation_choice = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn render_annotation_choice(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(choice) = &self.annotation_choice else {
            return gpui::Empty.into_any_element();
        };
        let owner = cx.weak_entity();
        let command = choice.command.clone();
        let candidates = choice.candidates.clone();
        let items: Vec<_> = candidates
            .iter()
            .enumerate()
            .map(|(ix, annotation)| {
                // The ordinal distinguishes even identical imported annotations.
                // Comments are metadata, never a substitute for marked text.
                let heading = format!(
                    "{} {} · {}",
                    annotation.kind().label(),
                    ix + 1,
                    color_label(annotation),
                );
                let mut metadata = heading.clone();
                if let Some(author) = annotation.author() {
                    metadata.push_str(&format!(" · Author: {author}"));
                }
                if let Some(comment) = annotation.comment() {
                    metadata.push_str(&format!(" · Comment: {comment}"));
                }
                let excerpt = choice.excerpts.get(&annotation.id());
                let text = excerpt_text(annotation, excerpt);
                let recovered = matches!(excerpt, Some(Excerpt::Text(_)));
                let label = format!("{text}; {metadata}");
                let annotation = annotation.clone();
                CommandItem::new().label(label).child(move |window, cx| {
                    div()
                        .id(format!(
                            "annotation-choice-{}-{}",
                            annotation.id().epoch(),
                            annotation.id().serial()
                        ))
                        .test_support()
                        .flex()
                        .items_start()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .child(
                            gpui_kit::component::Icon::new(annotations::markup_icon(
                                annotation.kind(),
                            ))
                            .small()
                            .flex_shrink_0(),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .id("annotation-choice-preview")
                                        .test_support()
                                        .aria_label(text.clone())
                                        .truncate()
                                        .child(marked_excerpt(
                                            &annotation,
                                            text.clone(),
                                            recovered,
                                            window,
                                        )),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .truncate()
                                        .child(metadata.clone()),
                                ),
                        )
                })
            })
            .collect();
        div().id("highlight-overlap-choice").test_support().absolute().inset_0()
            .child(
                Dialog::new(cx)
                    .focus_handle(choice.focus.clone())
                    .backdrop(div().absolute().inset_0().bg(cx.theme().overlay))
                    .on_ok(|_, _, _| false)
                    .on_close(cx.listener(|this, _, window, cx| this.dismiss_annotation_choice(window, cx)))
                    .p_4()
                    .popup(DialogPopup::new().w_full().max_w(gpui::rems(32.)).popover_style(cx)
                        .child(div().px_3().pt_3().pb_2().font_semibold().child("Choose an annotation"))
                        .child(Command::new(&choice.command)
                            .bordered(false)
                            .placeholder("Choose an overlapping annotation…")
                            .items(items)
                            .on_confirm({
                                let owner = owner.clone();
                                let command = command.clone();
                                move |index, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.annotation_choice.as_ref().is_none_or(|choice| choice.command != command) { return; }
                                        let id = candidates.get(index.row).map(Annotation::id);
                                        this.dismiss_annotation_choice(window, cx);
                                        if let Some(id) = id {
                                            this.select_page_annotation(id, cx);
                                            if this.selected_page_annotation(cx).is_none() {
                                                this.selected_annotation = None;
                                                this.status = "This annotation is no longer available.".into();
                                            }
                                        }
                                    });
                                }
                            })
                            .on_cancel(move |window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.annotation_choice.as_ref().is_some_and(|choice| choice.command == command) {
                                        this.dismiss_annotation_choice(window, cx);
                                    }
                                });
                            }))
                    )
            ).into_any_element()
    }
}

fn color_label(annotation: &Annotation) -> String {
    if let Some(color) = HighlightColor::ALL
        .into_iter()
        .find(|color| color.color() == annotation.color())
    {
        color.label().to_owned()
    } else if let Some([r, g, b]) = annotation.color().rgb() {
        format!(
            "#{:02X}{:02X}{:02X}",
            (r * 255.).round() as u8,
            (g * 255.).round() as u8,
            (b * 255.).round() as u8
        )
    } else {
        "Unspecified".to_owned()
    }
}
