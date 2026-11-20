//! Link targets use annotation rectangles, without requiring extractable text.

use super::*;

impl ReaderView {
    pub(super) fn prepare_link_hints(&mut self, window: &Window, cx: &mut Context<Self>) {
        let same_document_only =
            self.hints.active.as_ref().is_some_and(|session| {
                matches!(session.mode, Mode::Links(LinkDisposition::OtherPane))
            });
        let (top, _, height) = self.viewport();
        let visible = self.layout().visible_pages(top, height);
        if visible.clone().any(|page| self.links.failed(page)) {
            if let Some(session) = &mut self.hints.active {
                session.state =
                    State::Refused("Couldn’t read page links. Reopen the document to retry.");
            }
            return;
        }
        if visible
            .clone()
            .any(|page| self.links.visible(page).is_none())
        {
            return;
        }
        let bounds = self.scroll.bounds();
        let clip = hint::Rect::new(
            f32::from(bounds.left()) as f64,
            f32::from(bounds.top() + window.rem_size() * 2.) as f64,
            f32::from(bounds.right()) as f64,
            f32::from(bounds.bottom()) as f64,
        );
        let mut pages = Vec::new();
        let mut regions = Vec::new();
        for page in visible {
            let Some(origin) = self.hint_page_origin(page) else {
                continue;
            };
            let Some(projection) = self.page_viewport(page) else {
                continue;
            };
            let (width, height) = self.pages[page];
            pages.push(hint::Page::new(
                page,
                projection,
                hint::Rect::new(
                    origin.x,
                    origin.y,
                    origin.x + width as f64 * self.scale as f64,
                    origin.y + height as f64 * self.scale as f64,
                ),
                clip,
            ));
            for (ix, link) in self
                .links
                .visible(page)
                .unwrap_or_default()
                .iter()
                .enumerate()
            {
                if same_document_only
                    && !matches!(link.target(), Some(pdf::LinkDestination::SameDocument(_)))
                {
                    continue;
                }
                regions.push((page, ix, link.page_rect(), link.document_rect()));
            }
        }
        let Some(session) = &mut self.hints.active else {
            return;
        };
        session.state = State::Preparing;
        let metrics = session.metrics;
        let serial = self.hints.serial;
        let work = cx.background_executor().spawn(async move {
            // PDF annotation order need not be reading order, especially after rotation.
            regions.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.2[1].total_cmp(&b.2[1]))
                    .then_with(|| a.2[0].total_cmp(&b.2[0]))
            });
            let targets = regions.into_iter().map(|(page, ix, _, rect)| {
                let [left, bottom, right, top] = rect;
                hint::Target::new(
                    (page, ix),
                    page,
                    [
                        Point { x: left, y: top },
                        Point { x: right, y: top },
                        Point { x: left, y: bottom },
                        Point {
                            x: right,
                            y: bottom,
                        },
                    ],
                )
            });
            hint::Snapshot::prepare(targets, &pages, &[], metrics)
        });
        cx.spawn(async move |reader, cx| {
            let result = work.await;
            let _ = reader.update(cx, |reader, cx| {
                if reader.hints.serial != serial {
                    return;
                }
                let Some(session) = &mut reader.hints.active else {
                    return;
                };
                session.state = match result {
                    Ok(snapshot) => State::Links(Arc::new(snapshot)),
                    Err(hint::Refusal::NoText) if same_document_only => {
                        State::Refused("No visible links within this PDF")
                    }
                    Err(hint::Refusal::NoText) => State::Refused("No visible links to follow"),
                    Err(hint::Refusal::Crowded) => {
                        State::Refused("Hints are too crowded. Zoom in and try again.")
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }
}
