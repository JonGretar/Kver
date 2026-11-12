//! Window-local browsing of the shared session's text markup.

use std::{collections::HashMap, sync::Arc};

use gpui_kit::{
    App, ClipboardItem, Context, Entity, EventEmitter, IntoElement, Subscription, Task, Window,
    base::TestSupportExt as _,
    component::{
        ActiveTheme, Disableable as _, IndexPath, Sizable as _,
        button::{Button, ButtonVariants as _},
        list::{List, ListDelegate, ListEvent, ListItem, ListState},
    },
    div,
    prelude::*,
};

use crate::pdf::{
    self,
    annotations::{Annotation, AnnotationId},
};

pub(crate) struct AnnotationNavigation {
    pub(crate) generation: u64,
    pub(crate) revision: u64,
    pub(crate) annotation: Annotation,
}

#[derive(Clone)]
enum Excerpt {
    Text(String),
    Missing,
    Failed,
}

struct AnnotationRows {
    entries: Arc<Vec<Annotation>>,
    excerpts: HashMap<AnnotationId, Excerpt>,
    selected: Option<AnnotationId>,
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
        _: &mut Window,
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
        let id = format!(
            "annotation-{}-{}",
            annotation.id().epoch(),
            annotation.id().serial()
        );
        Some(
            ListItem::new(id)
                .accessibility_label(format!("{text}; {metadata}"))
                .h_16()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .w_full()
                        .gap_1()
                        .child(div().text_sm().truncate().child(text))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .when_some(annotation.color().rgb(), |row, rgb| {
                                    // This swatch is PDF data, not an application palette.
                                    row.child(div().size_2().flex_shrink_0().bg(gpui_kit::Rgba {
                                        r: rgb[0],
                                        g: rgb[1],
                                        b: rgb[2],
                                        a: 1.,
                                    }))
                                })
                                .child(div().truncate().child(metadata)),
                        ),
                ),
        )
    }
}

pub(crate) struct AnnotationsPanel {
    generation: u64,
    revision: Option<u64>,
    document: Option<Arc<pdf::Document>>,
    list: Entity<ListState<AnnotationRows>>,
    catalog: Option<Result<(), String>>,
    extraction: Option<Task<()>>,
    _selection: Subscription,
}

impl EventEmitter<AnnotationNavigation> for AnnotationsPanel {}

impl AnnotationsPanel {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let list = cx.new(|cx| {
            ListState::new(
                AnnotationRows {
                    entries: Arc::new(vec![]),
                    excerpts: HashMap::new(),
                    selected: None,
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
        self.generation = generation;
        self.document = document;
        self.revision = None;
        self.catalog = None;
        self.extraction = None;
        self.list.update(cx, |list, cx| {
            let rows = list.delegate_mut();
            rows.entries = Arc::new(vec![]);
            rows.excerpts.clear();
            rows.selected = None;
            list.set_selected_index(None, window, cx);
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn set_catalog(
        &mut self,
        revision: u64,
        catalog: Option<Result<Arc<Vec<Annotation>>, String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(catalog) = catalog else { return };
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.extraction = None;
        self.catalog = Some(catalog.as_ref().map(|_| ()).map_err(Clone::clone));
        let entries = catalog.unwrap_or_default();
        self.list.update(cx, |list, cx| {
            let rows = list.delegate_mut();
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
            .key_context(crate::key_context::ANNOTATIONS)
            .on_action(cx.listener(|this, _: &super::Copy, _, cx| this.copy(cx)))
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
