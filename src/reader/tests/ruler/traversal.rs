use super::*;

// One line at each end, with a long textless run between. This goes through
// the real PDF decoder and production background request loop.
fn sparse_pdf(count: usize) -> Vec<u8> {
    let content = "BT /F1 14 Tf 40 740 Td (Readable line) Tj ET";
    let font = count + 3;
    let stream = count + 4;
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!(
            "<< /Type /Pages /Count {count} /Kids [{}] >>",
            (3..count + 3)
                .map(|ix| format!("{ix} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    ];
    for page in 0..count {
        let contents = if page == 0 || page == count - 1 {
            format!("/Resources << /Font << /F1 {font} 0 R >> >> /Contents {stream} 0 R")
        } else {
            String::new()
        };
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] {contents} >>"
        ));
    }
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    objects.push(format!(
        "<< /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    ));
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (ix, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", ix + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

#[gpui_kit::test]
fn ruler_explicit_traversal_passes_long_textless_runs_but_browsing_does_not(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("sparse.pdf");
    std::fs::write(&path, sparse_pdf(130)).unwrap();
    let (window, reader) = open(cx, path.to_str().unwrap(), 0);
    for mode in [ReadingMode::SinglePage, ReadingMode::Continuous] {
        reader.update(cx, |reader, cx| {
            reader.set_reading_mode(mode, cx);
            reader.select_page(0, cx);
        });
        settle(cx, window);
        if !reader.read_with(cx, |reader, _| reader.ruler.enabled) {
            cx.simulate_keystrokes(window, "r");
        }
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (129, 0));
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (129, 0));
        reader.read_with(cx, |reader, _| {
            assert!(reader.status().contains("end of document"))
        });
        cx.simulate_keystrokes(window, "k");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (0, 0));
        cx.simulate_keystrokes(window, "k");
        reader.read_with(cx, |reader, _| {
            assert!(reader.status().contains("start of document"))
        });

        // Deliberate page navigation is not implicit textless-page traversal.
        reader.update(cx, |reader, cx| reader.select_page(60, cx));
        settle(cx, window);
        assert_eq!(reader.read_with(cx, |reader, _| reader.page_index), 60);
        // Browsing feedback/retarget uses the same settle path as a wheel gesture.
        reader.update(cx, |reader, cx| reader.browse_ruler(cx));
        cx.background_executor
            .advance_clock(Duration::from_millis(200));
        settle(cx, window);
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.page_index, 60);
            assert!(reader.ruler.enabled);
            assert!(reader.ruler.target.is_none());
            assert!(reader.status().contains("no usable text"));
        });
        cx.simulate_keystrokes(window, "j");
        settle(cx, window);
        assert_eq!(target(&reader, cx), (129, 0));
        cx.simulate_keystrokes(window, "escape");
    }
}

fn hold_next(reader: &Entity<ReaderView>, cx: &mut TestAppContext) -> page_text::TextRequest {
    reader.update(cx, |reader, _| {
        reader.page_text = page_text::PageTexts::default();
        reader
            .page_text
            .request(0..0, Some(3), [None; 2], reader.document_generation)
            .unwrap()
    })
}

fn release_next(
    reader: &Entity<ReaderView>,
    cx: &mut TestAppContext,
    request: page_text::TextRequest,
) {
    let text = pdf::page_text(Path::new("examples/reading-order.pdf"), 3).unwrap();
    reader.update(cx, |reader, cx| {
        assert!(reader.page_text.finish(request, Ok(text)));
        reader.continue_ruler(cx);
        reader.schedule_text(cx);
        cx.notify();
    });
}

#[gpui_kit::test]
fn ruler_delayed_traversal_reversal_navigation_and_failure_do_not_apply_obsolete_intent(
    cx: &mut TestAppContext,
) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    boundary(&reader, cx, 2, true);
    let initial = target(&reader, cx);
    let request = hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "j j j");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.ruler.text_priority(), Some(3));
        assert!(reader.status().contains("looking for text"));
    });
    cx.simulate_keystrokes(window, "k");
    assert_eq!(target(&reader, cx), (2, initial.1 - 1));
    let offset = reader.read_with(cx, |reader, _| reader.scroll.offset());
    release_next(&reader, cx, request);
    settle(cx, window);
    assert_eq!(target(&reader, cx), (2, initial.1 - 1));
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.scroll.offset()),
        offset
    );

    boundary(&reader, cx, 2, true);
    let request = hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "j");
    reader.update(cx, |reader, cx| reader.select_page(1, cx));
    release_next(&reader, cx, request);
    settle(cx, window);
    assert_eq!(reader.read_with(cx, |reader, _| reader.page_index), 1);

    boundary(&reader, cx, 2, true);
    let request = hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "j");
    reader.update(cx, |reader, cx| {
        assert!(
            reader
                .page_text
                .finish(request, Err("fixture failure".into()))
        );
        reader.continue_ruler(cx);
        cx.notify();
    });
    assert_eq!(target(&reader, cx), initial);
    reader.read_with(cx, |reader, _| {
        assert!(reader.ruler.traversal.is_none());
        assert!(reader.status().contains("couldn’t read page text"));
    });

    boundary(&reader, cx, 2, true);
    let request = hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "j");
    reader.update(cx, |reader, cx| {
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx)
    });
    reader.update(cx, |reader, _| {
        assert!(!reader.page_text.finish(request, Err("obsolete".into())));
    });
    settle(cx, window);
    reader.read_with(cx, |reader, _| {
        assert!(!reader.ruler.enabled);
        assert!(reader.ruler.target.is_none());
        assert!(reader.ruler.traversal.is_none());
    });
}

#[gpui_kit::test]
fn ruler_rapid_repeats_have_bounded_debt_and_closure_drops_pending_intent(cx: &mut TestAppContext) {
    let (window, reader) = open(cx, "examples/reading-order.pdf", 2);
    boundary(&reader, cx, 2, true);
    let request = hold_next(&reader, cx);
    for _ in 0..100 {
        cx.simulate_keystrokes(window, "j");
    }
    release_next(&reader, cx, request);
    settle(cx, window);
    assert_eq!(target(&reader, cx), (3, 31));
    boundary(&reader, cx, 2, true);
    let _request = hold_next(&reader, cx);
    cx.simulate_keystrokes(window, "j");
    let weak = reader.downgrade();
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    drop(reader);
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
}
