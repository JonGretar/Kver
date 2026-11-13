//! Window-local browsing of the shared session's text markup.

use std::{collections::HashMap, sync::Arc};

use gpui_kit::{
    App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, IntoElement, Subscription,
    Task, WeakEntity, Window,
    assets::IconName,
    base::TestSupportExt as _,
    component::{
        ActiveTheme, Disableable as _, Icon, IndexPath, Sizable as _,
        button::{Button, ButtonVariants as _},
        list::{List, ListDelegate, ListEvent, ListItem, ListState},
        menu::{DropdownMenu as _, PopupMenuItem},
    },
    div,
    prelude::*,
};

use crate::pdf::{
    self,
    annotations::{Annotation, AnnotationId, HighlightColor, MarkupKind},
};

pub(crate) fn markup_icon(kind: MarkupKind) -> IconName {
    match kind {
        MarkupKind::Highlight => IconName::Highlighter,
        MarkupKind::Underline => IconName::Underline,
        MarkupKind::StrikeOut => IconName::Strikethrough,
        MarkupKind::Squiggly => IconName::WavesHorizontal,
    }
}

/// A text sample, not another rendering of the PDF appearance. Keep the
/// foreground readable in both themes and never decorate extraction fallbacks.
fn marked_excerpt(
    annotation: &Annotation,
    text: String,
    recovered: bool,
    window: &Window,
) -> gpui_kit::StyledText {
    let mut style = gpui_kit::HighlightStyle::default();
    if recovered && let Some([r, g, b]) = annotation.color().rgb() {
        // Imported PDF colors are data; do not normalize them to UI presets.
        let color = gpui_kit::Hsla::from(gpui_kit::Rgba { r, g, b, a: 1. });
        // Decoration thickness follows interface scale, unlike PDF-space paint.
        let thickness = window.rem_size() / 14.;
        match annotation.kind() {
            MarkupKind::Highlight => {
                style.background_color = Some(color.opacity(0.3));
            }
            MarkupKind::Underline | MarkupKind::Squiggly => {
                style.underline = Some(gpui_kit::UnderlineStyle {
                    color: Some(color),
                    thickness,
                    wavy: annotation.kind() == MarkupKind::Squiggly,
                });
            }
            MarkupKind::StrikeOut => {
                style.strikethrough = Some(gpui_kit::StrikethroughStyle {
                    color: Some(color),
                    thickness,
                });
            }
        }
    }
    let len = text.len();
    gpui_kit::StyledText::new(text).with_highlights([(0..len, style)])
}

pub(crate) struct AnnotationNavigation {
    pub(crate) generation: u64,
    pub(crate) revision: u64,
    pub(crate) annotation: Annotation,
}

#[derive(Clone)]
pub(crate) struct AnnotationEdit {
    pub(crate) generation: u64,
    pub(crate) revision: u64,
    pub(crate) id: AnnotationId,
    pub(crate) color: Option<HighlightColor>,
}

#[derive(Clone)]
enum Excerpt {
    Text(String),
    Missing,
    Failed,
}

struct AnnotationRows {
    owner: WeakEntity<AnnotationsPanel>,
    entries: Arc<Vec<Annotation>>,
    excerpts: HashMap<AnnotationId, Excerpt>,
    selected: Option<AnnotationId>,
    row_focus: HashMap<AnnotationId, RowFocus>,
}

struct RowFocus {
    row: FocusHandle,
    color: FocusHandle,
    delete: FocusHandle,
}

impl ListDelegate for AnnotationRows {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.entries.len()
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix
            .and_then(|ix| self.entries.get(ix.row))
            .map(Annotation::id);
        cx.notify();
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let annotation = self.entries.get(ix.row)?;
        let text = match self.excerpts.get(&annotation.id()) {
            Some(Excerpt::Text(text)) => text.clone(),
            _ if annotation.is_readonly() => "Malformed annotation — readonly".into(),
            Some(Excerpt::Missing) => "No marked text available".into(),
            Some(Excerpt::Failed) => "Couldn’t extract marked text".into(),
            None => "Loading marked text…".into(),
        };
        let color = annotation
            .color()
            .rgb()
            .map(|rgb| {
                let [r, g, b] = rgb.map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
                format!("#{r:02X}{g:02X}{b:02X}")
            })
            .unwrap_or_else(|| "Unspecified color".into());
        let metadata = format!(
            "{} · Page {} · {}",
            annotation.kind().label(),
            annotation.page() + 1,
            color
        );
        let author = annotation
            .author()
            .filter(|author| !author.trim().is_empty());
        let label = match author {
            Some(author) => format!("{text}; {metadata}; By {author}"),
            None => format!("{text}; {metadata}"),
        };
        let summary = match author {
            Some(author) => format!("Page {} · {author}", annotation.page() + 1),
            None => format!("Page {}", annotation.page() + 1),
        };
        let recovered = matches!(self.excerpts.get(&annotation.id()), Some(Excerpt::Text(_)));
        let preview = marked_excerpt(annotation, text, recovered, window);
        let id = format!(
            "annotation-{}-{}",
            annotation.id().epoch(),
            annotation.id().serial()
        );
        let owner = self.owner.upgrade()?;
        let panel = owner.read(cx);
        let editable = panel.editable && !annotation.is_readonly();
        let request = AnnotationEdit {
            generation: panel.generation,
            revision: panel.revision?,
            id: annotation.id(),
            color: None,
        };
        let current_color = annotation.color();
        let focus = self
            .row_focus
            .entry(annotation.id())
            .or_insert_with(|| RowFocus {
                row: cx.focus_handle(),
                color: cx.focus_handle(),
                delete: cx.focus_handle(),
            });
        let color_owner = self.owner.clone();
        let color_request = request.clone();
        let delete_owner = self.owner.clone();
        let actions = div()
            .id("annotation-actions")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            // Nested commands must not also activate/navigate the row.
            .on_click(|_, _, cx| cx.stop_propagation())
            // List's Enter action must not move focus before
            // the button's native key-up activation.
            .key_context(crate::key_context::ANNOTATION_ROW_ACTIONS)
            .child(
                Button::new("recolor-annotation")
                    .track_focus(&focus.color)
                    .icon(IconName::Palette)
                    .accessibility_label("Change annotation color")
                    .tooltip("Change color…")
                    .ghost()
                    .xsmall()
                    .disabled(!editable)
                    .dropdown_menu(move |menu, _, _| {
                        let mut menu = menu;
                        for color in HighlightColor::ALL {
                            let owner = color_owner.clone();
                            let mut request = color_request.clone();
                            request.color = Some(color);
                            menu = menu.item(
                                PopupMenuItem::new(color.label())
                                    .checked(current_color == color.color())
                                    .on_click(move |_, window, cx| {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.edit(request.clone(), window, cx)
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            )
            .child(
                Button::new("delete-annotation")
                    .track_focus(&focus.delete)
                    .icon(IconName::Trash)
                    .accessibility_label("Delete annotation")
                    .tooltip("Delete annotation")
                    .ghost()
                    .xsmall()
                    .disabled(!editable)
                    .on_click(move |_, window, cx| {
                        let _ = delete_owner
                            .update(cx, |this, cx| this.edit(request.clone(), window, cx));
                    }),
            );
        Some(
            ListItem::new(id)
                .accessibility_label(label)
                .track_focus(&focus.row)
                .h_12()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .w_full()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().flex_1().min_w_0().text_sm().truncate().child(preview))
                                .child(actions),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    Icon::new(markup_icon(annotation.kind()))
                                        .small()
                                        .flex_shrink_0(),
                                )
                                .child(div().truncate().child(summary)),
                        ),
                ),
        )
    }
}

pub(crate) struct AnnotationsPanel {
    generation: u64,
    revision: Option<u64>,
    editable: bool,
    focus: FocusHandle,
    document: Option<Arc<pdf::Document>>,
    list: Entity<ListState<AnnotationRows>>,
    catalog: Option<Result<(), String>>,
    extraction: Option<Task<()>>,
    _selection: Subscription,
}

impl EventEmitter<AnnotationNavigation> for AnnotationsPanel {}
impl EventEmitter<AnnotationEdit> for AnnotationsPanel {}

impl AnnotationsPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let owner = cx.weak_entity();
        let list = cx.new(|cx| {
            ListState::new(
                AnnotationRows {
                    owner,
                    entries: Arc::new(vec![]),
                    excerpts: HashMap::new(),
                    selected: None,
                    row_focus: HashMap::new(),
                },
                window,
                cx,
            )
            .searchable(false)
        });
        let selection = cx.subscribe(&list, |this, _, event: &ListEvent, cx| {
            if matches!(event, ListEvent::Select(_) | ListEvent::Confirm(_)) {
                this.activate_selected(cx);
                cx.notify();
            }
        });
        Self {
            generation: 0,
            revision: None,
            editable: false,
            focus: cx.focus_handle(),
            document: None,
            list,
            catalog: None,
            extraction: None,
            _selection: selection,
        }
    }

    pub(crate) fn set_document(
        &mut self,
        generation: u64,
        document: Option<Arc<pdf::Document>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.generation == generation && self.document.is_some() && document.is_some() {
            // Save As replaces the byte baseline, not this window's selection
            // or stable session annotation identities.
            self.document = document;
            return;
        }
        self.generation = generation;
        self.document = document;
        self.revision = None;
        self.editable = false;
        self.catalog = None;
        self.extraction = None;
        if self.focus.contains_focused(window, cx) {
            self.focus(window, cx);
        }
        self.list.update(cx, |list, cx| {
            let rows = list.delegate_mut();
            rows.entries = Arc::new(vec![]);
            rows.excerpts.clear();
            rows.selected = None;
            rows.row_focus.clear();
            list.set_selected_index(None, window, cx);
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn set_catalog(
        &mut self,
        revision: u64,
        editable: bool,
        catalog: Option<Result<Arc<Vec<Annotation>>, String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editable = editable;
        let Some(catalog) = catalog else { return };
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.extraction = None;
        self.catalog = Some(catalog.as_ref().map(|_| ()).map_err(Clone::clone));
        let entries = catalog.unwrap_or_default();
        let removed_focused_row =
            self.list
                .read(cx)
                .delegate()
                .row_focus
                .iter()
                .any(|(id, focus)| {
                    focus.row.contains_focused(window, cx) && !entries.iter().any(|a| a.id() == *id)
                });
        if removed_focused_row {
            // Also repair focus when another window removes a focused row button,
            // even if the row was never selected.
            // The List remains mounted and focusable even when it becomes empty.
            self.focus(window, cx);
        }
        let ids: std::collections::HashSet<_> = entries.iter().map(Annotation::id).collect();
        self.list.update(cx, |list, cx| {
            let rows = list.delegate_mut();
            rows.row_focus.retain(|id, _| ids.contains(id));
            if !entries.iter().any(|a| Some(a.id()) == rows.selected) {
                rows.selected = None;
            }
            rows.entries = entries.clone();
            rows.excerpts.clear();
            let selected = rows
                .entries
                .iter()
                .position(|a| Some(a.id()) == rows.selected)
                .map(IndexPath::new);
            list.set_selected_index(selected, window, cx);
            cx.notify();
        });
        let Some(document) = self.document.clone() else {
            cx.notify();
            return;
        };
        let generation = self.generation;
        let mut offset = 0;
        let pages: Vec<_> = entries
            .chunk_by(|a, b| a.page() == b.page())
            .map(|chunk| {
                let range = offset..offset + chunk.len();
                offset = range.end;
                (chunk[0].page(), range)
            })
            .collect();
        // One annotated page at a time. Publish partial excerpts between jobs;
        // cancellation drops the continuation, never retaining a reader/session.
        self.extraction = Some(cx.spawn(async move |this, cx| {
            for (page, range) in pages {
                let document = document.clone();
                let entries = entries.clone();
                let result = cx
                    .background_spawn(
                        async move { document.annotation_excerpts(page, &entries[range]) },
                    )
                    .await;
                let current = this
                    .update(cx, |this, cx| {
                        if this.generation != generation || this.revision != Some(revision) {
                            return false;
                        }
                        this.list.update(cx, |list, cx| {
                            let rows = list.delegate_mut();
                            match result {
                                Ok(excerpts) => {
                                    for (id, text) in excerpts {
                                        rows.excerpts.insert(
                                            id,
                                            text.map(Excerpt::Text).unwrap_or(Excerpt::Missing),
                                        );
                                    }
                                }
                                Err(_) => {
                                    for a in rows.entries.iter().filter(|a| a.page() == page) {
                                        rows.excerpts.insert(a.id(), Excerpt::Failed);
                                    }
                                }
                            }
                            cx.notify();
                        });
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !current {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub(crate) fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.focus(window, cx));
    }

    pub(crate) fn document_failed(&mut self, error: &str, cx: &mut Context<Self>) {
        self.catalog = Some(Err(error.to_owned()));
        cx.notify();
    }

    fn selected(&self, cx: &App) -> Option<Annotation> {
        let rows = self.list.read(cx).delegate();
        rows.entries
            .iter()
            .find(|a| Some(a.id()) == rows.selected)
            .cloned()
    }

    fn activate_selected(&self, cx: &mut Context<Self>) {
        if let (Some(annotation), Some(revision)) = (self.selected(cx), self.revision) {
            cx.emit(AnnotationNavigation {
                generation: self.generation,
                revision,
                annotation,
            });
        }
    }

    fn selected_text(&self, cx: &App) -> Option<String> {
        let rows = self.list.read(cx).delegate();
        match rows.excerpts.get(&rows.selected?)? {
            Excerpt::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn copy(&self, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(crate) fn edit_available(&self, cx: &App) -> bool {
        self.editable
            && self
                .selected(cx)
                .is_some_and(|annotation| !annotation.is_readonly())
    }

    pub(crate) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    fn edit_selected(
        &self,
        color: Option<HighlightColor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(annotation), Some(revision)) = (self.selected(cx), self.revision) {
            self.edit(
                AnnotationEdit {
                    generation: self.generation,
                    revision,
                    id: annotation.id(),
                    color,
                },
                window,
                cx,
            );
        }
    }

    fn edit(&self, request: AnnotationEdit, window: &mut Window, cx: &mut Context<Self>) {
        // A menu may outlive its catalog/document. Never retarget a late command.
        if !self.editable
            || request.generation != self.generation
            || Some(request.revision) != self.revision
            || !self
                .list
                .read(cx)
                .delegate()
                .entries
                .iter()
                .any(|a| a.id() == request.id && !a.is_readonly())
        {
            return;
        }
        if request.color.is_none() {
            self.focus(window, cx);
        }
        cx.emit(request);
    }
}

impl Render for AnnotationsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = match &self.catalog {
            None => Some("Loading annotations…"),
            Some(Err(_)) if self.document.is_none() => {
                Some("Couldn’t load this PDF’s annotations. Open another PDF to continue.")
            }
            Some(Err(_)) => Some("Couldn’t read annotations. You can still read this PDF."),
            Some(Ok(())) if self.list.read(cx).delegate().entries.is_empty() => {
                Some("No text markup in this PDF")
            }
            _ => None,
        };
        div()
            .id("annotations-panel")
            .test_support()
            .track_focus(&self.focus)
            .key_context(crate::key_context::ANNOTATIONS)
            .on_action(cx.listener(|this, _: &super::Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &super::DeleteHighlight, window, cx| {
                this.edit_selected(None, window, cx);
            }))
            .on_action(
                cx.listener(|this, action: &super::RecolorHighlight, window, cx| {
                    this.edit_selected(Some(action.0), window, cx);
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("Annotations")
                    .child(
                        Button::new("copy-annotation")
                            .ghost()
                            .small()
                            .label("Copy text")
                            .disabled(self.selected_text(cx).is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    ),
            )
            .when_some(status, |panel, status| {
                panel.child(
                    div()
                        .id("annotations-status")
                        .test_support()
                        .aria_label(status)
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
            })
            .child(div().flex_1().min_h_0().child(List::new(&self.list)))
    }
}
