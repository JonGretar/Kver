//! Ruler restoration is deliberately lazy and independent of viewport history.
use super::*;
use crate::reading_history::RulerAnchor;

#[derive(Default)]
pub(super) struct RulerHistory {
    remembered: Option<RulerAnchor>,
    last: Option<RulerAnchor>,
    touched: bool,
    epoch: Option<u64>,
    save_task: Option<gpui::Task<()>>,
}

impl ReaderView {
    pub(super) fn ruler_anchor(&self) -> Option<RulerAnchor> {
        let target = self.ruler.target.as_ref()?;
        let [left, bottom, right, top] = ruler::document_rect(&target.text, target.line)?;
        Some(RulerAnchor {
            page: target.page,
            x: (left + right) / 2.,
            y: (bottom + top) / 2.,
        })
    }

    pub(super) fn load_ruler_anchor(
        &mut self,
        history: &recent::RecentHistory,
        epoch: u64,
        cx: &mut Context<Self>,
    ) {
        self.ruler_history.epoch = Some(epoch);
        let Some(path) = self.record_path.clone() else {
            return;
        };
        let work = history.ruler_anchor(path, epoch);
        let generation = self.document_generation;
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_ruler_load(generation, epoch, result, cx);
            });
        })
        .detach();
    }

    pub(super) fn finish_ruler_load(
        &mut self,
        generation: u64,
        epoch: u64,
        result: Result<Option<RulerAnchor>, String>,
        cx: &mut Context<Self>,
    ) {
        if self.document_generation != generation
            || self.ruler_history.touched
            || recent::history(cx).is_none_or(|history| history.epoch() != epoch)
        {
            return;
        }
        match result {
            Ok(anchor) => {
                self.ruler_history.remembered =
                    anchor.filter(|a| a.is_valid() && a.page < self.pages.len())
            }
            Err(error) => {
                self.status = format!("Reading ruler location could not be loaded: {error}");
                cx.notify();
            }
        }
    }

    /// The target changed in this window. Never call this for restore, reload,
    /// zoom, or close: those are not newer reading movements.
    pub(super) fn remember_ruler(&mut self, cx: &mut Context<Self>) {
        let Some(anchor) = self.ruler_anchor() else {
            return;
        };
        self.ruler_history.touched = true;
        self.ruler_history.remembered = Some(anchor);
        if self.ruler_history.last == Some(anchor) {
            return;
        }
        self.ruler_history.last = Some(anchor);
        let (Some(path), Some(history), Some(epoch)) = (
            self.record_path.clone(),
            recent::history(cx),
            self.ruler_history.epoch,
        ) else {
            return;
        };
        let work = history.save_ruler_anchor(path, anchor, epoch);
        let generation = self.document_generation;
        // Replacing this waiter does not cancel the already-enqueued write.
        // It only prevents an older failure from replacing newer feedback.
        self.ruler_history.save_task = Some(cx.spawn(async move |this, cx| {
            if let Err(error) = work.await {
                let _ = this.update(cx, |this, cx| {
                    if this.document_generation == generation
                        && recent::history(cx).is_some_and(|history| history.epoch() == epoch)
                    {
                        this.status = format!("Reading ruler location could not be saved: {error}");
                        cx.notify();
                    }
                });
            }
        }));
    }

    pub(super) fn resume_ruler(&mut self, cx: &mut Context<Self>) -> bool {
        // Activation itself supersedes a delayed initial restore.
        self.ruler_history.touched = true;
        let Some(anchor) = self.ruler_history.remembered else {
            return false;
        };
        if anchor.page >= self.pages.len() {
            return false;
        }
        let (top, _, height) = self.viewport();
        if !self
            .layout()
            .visible_pages(top, height)
            .contains(&anchor.page)
        {
            return false;
        }
        self.ruler.pending = Some((
            anchor.page,
            ruler::Placement::Remembered(Point {
                x: anchor.x,
                y: anchor.y,
            }),
        ));
        self.finish_ruler_placement(cx);
        self.schedule_text(cx);
        true
    }

    pub(super) fn place_ruler_at_center(&mut self, cx: &mut Context<Self>) {
        self.ruler.target = None;
        let center = self.scroll.bounds().center();
        let (top, _, height) = self.viewport();
        let page = self.layout().current_page_at(top, height);
        self.queue_ruler_placement(page, center, false);
        self.finish_ruler_placement(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometric_anchor_resolves_columns_even_when_source_line_order_changes() {
        let mut changed = pdf::page_text(Path::new("examples/reading-order.pdf"), 2).unwrap();
        let anchors = (0..changed.lines.len())
            .map(|line| ruler::document_rect(&changed, line).unwrap())
            .collect::<Vec<_>>();
        changed.lines.reverse();
        let changed = Arc::new(changed);
        for (line, [left, bottom, right, top]) in anchors.iter().copied().enumerate() {
            let mut restored = ruler::Ruler::default();
            assert!(restored.place(
                2,
                changed.clone(),
                Point {
                    x: (left + right) / 2.,
                    y: (bottom + top) / 2.,
                },
                true
            ));
            assert_eq!(restored.target.unwrap().line, anchors.len() - line - 1);
        }
    }
}
