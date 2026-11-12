use super::*;
use gpui_kit::TestAppContext;

fn baseline(path: &str) -> Option<Arc<pdf::Document>> {
    Some(Arc::new(
        pdf::Document::open(std::path::Path::new(path)).unwrap(),
    ))
}

#[gpui_kit::test]
fn loading_is_lazy_and_old_results_cannot_replace_the_new_document(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(|_, cx| ContentsPanel::new(cx));
    window
        .update(cx, |contents, _, cx| {
            contents.set_document(1, baseline("examples/field-guide.pdf"), cx);
            assert!(contents.task.is_none());
            assert!(contents.outline.is_none());
            contents.ensure_loaded(cx);
            assert!(contents.task.is_some());

            contents.set_document(2, baseline("examples/readme-sample.pdf"), cx);
            assert!(contents.task.is_none());
            let old_outline =
                pdf::document_outline(std::path::Path::new("examples/field-guide.pdf")).unwrap();
            assert!(!contents.install(1, Ok(old_outline), cx));
            assert!(contents.outline.is_none());
            assert!(contents.selected_destination(cx).is_none());
            contents.ensure_loaded(cx);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |contents, _| {
            assert!(
                contents
                    .outline
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .is_empty()
            );
        })
        .unwrap();
}

#[gpui_kit::test]
fn load_failures_are_retained_until_document_replacement(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(|_, cx| ContentsPanel::new(cx));
    window
        .update(cx, |contents, _, cx| {
            contents.set_document(1, baseline("examples/readme-sample.pdf"), cx);
            contents.install(1, Err("Cannot read outline".into()), cx);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |contents, frame, cx| {
            assert!(contents.outline.as_ref().unwrap().is_err());
            contents.focus(frame, cx);
            assert!(contents.outline.as_ref().unwrap().is_err());
            contents.set_document(2, baseline("examples/field-guide.pdf"), cx);
            assert!(contents.outline.is_none());
            contents.ensure_loaded(cx);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .read_with(cx, |contents, _| {
            assert!(contents.has_navigable_entries());
            assert_eq!(contents.destination(0).unwrap().page(), 2);
        })
        .unwrap();
}

#[gpui_kit::test]
fn pending_extraction_does_not_keep_a_closed_contents_view_alive(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let window = cx.add_window(|_, cx| ContentsPanel::new(cx));
    let weak = window
        .update(cx, |contents, _, cx| {
            contents.set_document(1, baseline("examples/field-guide.pdf"), cx);
            contents.ensure_loaded(cx);
            cx.weak_entity()
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}
