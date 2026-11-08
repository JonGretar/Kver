//! Visible-page link metadata: one background request and generation-safe replies.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use crate::pdf::PageLink;

#[derive(Default)]
pub(super) struct PageLinks {
    pages: HashMap<usize, Vec<PageLink>>,
    failed: HashSet<usize>,
    in_flight: Option<LinkRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LinkRequest {
    page: usize,
    generation: u64,
}

impl LinkRequest {
    pub(super) fn page(self) -> usize {
        self.page
    }
}

impl PageLinks {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn visible(&self, page: usize) -> Option<&[PageLink]> {
        self.pages.get(&page).map(Vec::as_slice)
    }

    pub(super) fn destination(&self, page: usize, index: usize) -> Option<&PageLink> {
        self.pages.get(&page)?.get(index)
    }

    pub(super) fn request(
        &mut self,
        visible: Range<usize>,
        generation: u64,
    ) -> Option<LinkRequest> {
        self.pages.retain(|page, _| visible.contains(page));
        if self.in_flight.is_some() {
            return None;
        }
        let page = visible
            .into_iter()
            .find(|page| !self.pages.contains_key(page) && !self.failed.contains(page))?;
        let request = LinkRequest { page, generation };
        self.in_flight = Some(request);
        Some(request)
    }

    pub(super) fn finish(
        &mut self,
        request: LinkRequest,
        result: Result<Vec<PageLink>, String>,
    ) -> bool {
        if self.in_flight != Some(request) {
            return false;
        }
        self.in_flight = None;
        match result {
            Ok(links) => {
                self.pages.insert(request.page, links);
            }
            Err(_) => {
                self.failed.insert(request.page);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaced_document_and_scrolled_pages_reject_stale_work() {
        let mut links = PageLinks::default();
        let old = links.request(0..2, 1).unwrap();
        links.reset();
        let current = links.request(2..4, 2).unwrap();
        assert!(!links.finish(old, Ok(Vec::new())));
        assert!(links.finish(current, Ok(Vec::new())));
        assert!(links.visible(2).is_some());
        assert!(links.request(3..4, 2).is_some());
        assert!(links.visible(2).is_none());
    }
}
