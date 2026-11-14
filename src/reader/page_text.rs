//! One window-local, bounded text source for selection and the reading ruler.
//! The reader executes at most one extraction off-thread; replies carry document
//! identity rather than raster/zoom identity. Consumers retain only active pages.

use std::{collections::HashMap, ops::Range, sync::Arc};

use crate::pdf::PageText;

#[derive(Default)]
pub(super) struct PageTexts {
    pages: HashMap<usize, Result<Arc<PageText>, String>>,
    in_flight: Option<TextRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextRequest {
    page: usize,
    generation: u64,
}

impl TextRequest {
    pub(super) fn page(self) -> usize {
        self.page
    }
}

impl PageTexts {
    pub(super) fn get(&self, page: usize) -> Option<&Arc<PageText>> {
        self.pages.get(&page)?.as_ref().ok()
    }

    pub(super) fn failed(&self, page: usize) -> bool {
        self.pages.get(&page).is_some_and(Result::is_err)
    }

    pub(super) fn request(
        &mut self,
        visible: Range<usize>,
        priority: Option<usize>,
        active: [Option<usize>; 2],
        generation: u64,
    ) -> Option<TextRequest> {
        // Prune failures too: browsing a long document must not accumulate an
        // unbounded failure set. Active consumers also retain their Arcs, so
        // eviction never invalidates a line identity or a copy operation.
        self.pages.retain(|page, _| {
            visible.contains(page) || priority == Some(*page) || active.contains(&Some(*page))
        });
        if self.in_flight.is_some() {
            return None;
        }
        let page = priority
            .into_iter()
            .chain(visible)
            .find(|page| !self.pages.contains_key(page))?;
        let request = TextRequest { page, generation };
        self.in_flight = Some(request);
        Some(request)
    }

    pub(super) fn finish(
        &mut self,
        request: TextRequest,
        result: Result<PageText, String>,
    ) -> bool {
        if self.in_flight != Some(request) {
            return false;
        }
        self.in_flight = None;
        self.pages.insert(request.page, result.map(Arc::new));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_and_priority_requests_stay_bounded_and_late_generations_cannot_finish() {
        let mut pages = PageTexts::default();
        let old = pages.request(0..2, Some(7), [None; 2], 1).unwrap();
        assert_eq!(old.page(), 7);
        assert!(pages.request(100..102, Some(8), [None; 2], 1).is_none());
        pages = PageTexts::default();
        let current = pages.request(0..2, None, [None; 2], 2).unwrap();
        assert!(!pages.finish(old, Err("obsolete".into())));
        assert!(pages.request(100..102, None, [None; 2], 2).is_none());
        assert!(pages.finish(current, Err("unreadable".into())));
        for page in 1..1000 {
            let request = pages.request(page..page + 1, None, [None; 2], 2).unwrap();
            assert!(pages.finish(request, Err("unreadable".into())));
            assert_eq!(pages.pages.len(), 1);
            assert!(pages.failed(page));
            assert!(pages.request(page..page + 1, None, [None; 2], 2).is_none());
        }
    }

    #[test]
    fn textless_traversal_retains_only_visible_active_and_one_priority_page() {
        let mut pages = PageTexts::default();
        let empty = || PageText {
            transform: crate::coordinates::PageTransform::from_coefficients([
                1., 0., 0., 1., 0., 0.,
            ]),
            glyphs: Vec::new(),
            lines: Vec::new(),
        };
        for page in 0..2 {
            let request = pages.request(0..2, Some(page), [Some(1), None], 1).unwrap();
            assert!(pages.finish(request, Ok(empty())));
        }
        for page in 2..1000 {
            let request = pages.request(0..1, Some(page), [Some(1), None], 1).unwrap();
            assert_eq!(request.page(), page);
            assert!(pages.finish(request, Ok(empty())));
            assert_eq!(pages.pages.len(), 3);
            assert!(pages.get(0).is_some());
            assert!(pages.get(1).is_some());
            assert!(pages.get(page).is_some());
        }
    }
}
