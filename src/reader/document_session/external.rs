//! External files never replace the editing source until a complete reload succeeds.
use super::*;
use std::time::Duration;

struct Reload {
    document: Arc<pdf::Document>,
    pages: Vec<(f32, f32)>,
}

impl DocumentSession {
    pub(crate) fn reloaded_pages(&self) -> Vec<(f32, f32)> {
        self.external_pages.clone()
    }

    pub(super) fn watch_external(&mut self, cx: &mut Context<Self>) {
        if self.external_watch.is_some() || self.result().and_then(Result::ok).is_none() {
            return;
        }
        self.external_watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |this, cx| this.check_external(false, false, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn external_error(&self) -> Option<&str> {
        self.external_error.as_deref()
    }

    pub(crate) fn take_external_notice(&mut self) -> bool {
        std::mem::take(&mut self.external_notice)
    }

    /// Activation verifies bytes even when an external writer preserves timestamps.
    /// Polling normally checks metadata only, avoiding repeated reads of large PDFs.
    pub(crate) fn check_external(&mut self, force: bool, discard: bool, cx: &mut Context<Self>) {
        if self.is_saving() || (!force && !discard && self.external_conflict && self.is_dirty()) {
            return;
        }
        if self.external_work.is_some() {
            if !force && !discard {
                return;
            }
            // An explicit decision supersedes a routine probe, never the reverse.
            self.external_work = None;
        }
        let Some(document) = self.result().and_then(Result::ok) else {
            return;
        };
        let epoch = self.epoch;
        let revision = self.revision;
        let path = self.path.clone();
        let baseline = document.clone();
        let work = cx.background_executor().spawn(async move {
            let metadata = std::fs::metadata(&path)
                .map_err(|error| format!("Cannot reload {}: {error}", path.display()))?;
            if !force
                && !discard
                && baseline
                    .file_metadata()
                    .is_some_and(|loaded| save_file::same_content_version(loaded, &metadata))
            {
                return Ok(None);
            }
            if !discard
                && baseline
                    .file_metadata()
                    .is_some_and(|loaded| save_file::same_content_version(loaded, &metadata))
                && std::fs::read(&path).map_err(|error| error.to_string())? == baseline.bytes()
                && std::fs::metadata(&path)
                    .is_ok_and(|after| save_file::same_content_version(&metadata, &after))
            {
                // Activation verifies contents but need not parse an unchanged PDF.
                return Ok(None);
            }
            let replacement = pdf::Document::open(&path)?;
            if !discard
                && baseline.bytes() == replacement.bytes()
                && baseline.file_metadata().is_some_and(|loaded| {
                    replacement
                        .file_metadata()
                        .is_some_and(|current| save_file::same_content_version(loaded, current))
                })
            {
                return Ok(None);
            }
            let pages = replacement.pages();
            if pages.is_empty() {
                return Err("Cannot reload a PDF with no pages.".into());
            }
            Ok(Some(Reload {
                document: Arc::new(replacement),
                pages,
            }))
        });
        self.external_work = Some(cx.spawn(async move |this, cx| {
            let result: Result<Option<Reload>, String> = work.await;
            let _ = this.update(cx, |this, cx| {
                this.external_work = None;
                // Save, Save As, and other reloads supersede this file observation.
                if this.epoch != epoch
                    || this.is_saving()
                    || this
                        .result()
                        .and_then(Result::ok)
                        .is_none_or(|current| !Arc::ptr_eq(&current, &document))
                {
                    return;
                }
                match result {
                    Ok(Some(replacement)) => {
                        if this.is_dirty() && (!discard || this.revision != revision) {
                            this.external_notice |= !this.external_conflict || discard;
                            this.external_conflict = true;
                            if discard {
                                this.external_error = Some("New edits arrived while reloading. They have been kept; choose again.".into());
                            }
                        } else if this.revision == revision {
                            this.install_external(replacement, cx);
                        }
                    }
                    Ok(None) => {
                        if this.external_error.is_none()
                            && !this.external_conflict
                            && !this.external_notice
                        {
                            return;
                        }
                        this.external_error = None;
                        this.external_conflict = false;
                        this.external_notice = false;
                    }
                    Err(error) => {
                        this.external_error = Some(error);
                        if this.is_dirty() {
                            this.external_notice |= !this.external_conflict;
                            this.external_conflict = true;
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn install_external(&mut self, reload: Reload, cx: &mut Context<Self>) {
        self.epoch = NEXT_EPOCH.fetch_add(1, Ordering::Relaxed);
        self.revision += 1;
        self.annotation_load = None;
        self.preview_work = None;
        self.preview = None;
        self.preview_error = None;
        self.annotations = None;
        self.editing_source = None;
        self.original_annotations = Arc::new(Vec::new());
        self.saved_annotations = Arc::new(Vec::new());
        self.next_serial = 1;
        self.saved_serial = 0;
        self.history = history::AnnotationHistory::default();
        self.dirty = false;
        self.external_conflict = false;
        self.external_notice = false;
        self.external_error = None;
        self.external_pages = reload.pages;
        self.result = Some(Ok(reload.document));
        crate::app::quit_guard::invalidate_approval();
        self.ensure_annotations(cx);
    }
}
