//! Main-thread Cargo test: drive the actual reader, assert state, and inspect PNGs.
//!
//! Set `KVER_BENCH=1` for the opt-in page-turn timing diagnostic.
//! Rust's default #[test] runner uses worker threads; Metal requires the
//! process main thread.

use gpui::{
    AnyWindowHandle, AppContext as _, HeadlessAppContext, InputEvent as _, KeyDownEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent,
    WindowHandle, point, px, size,
};
use kver::reader::Preview;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn wait_for_page(
    cx: &mut HeadlessAppContext,
    window: &WindowHandle<Preview>,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    cx.allow_parking();
    loop {
        cx.run_until_parked();
        let status = cx.update(|app| window.read(app).map(|reader| reader.status().to_owned()))?;
        if status == expected {
            cx.forbid_parking();
            return Ok(());
        }
        if !(status.starts_with("Rendering page ") || status == "Loading document…")
            || Instant::now() >= deadline
        {
            return Err(format!("Expected {expected:?}, got {status:?}").into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Click at window-local logical pixels, not screenshot (device) pixels.
/// Capture a frame first so GPUI has laid out the elements for hit-testing.
pub fn click(
    cx: &mut HeadlessAppContext,
    window: AnyWindowHandle,
    position: Point<Pixels>,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.update_window(window, |_, window, app| {
        window.dispatch_event(
            MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
        window.dispatch_event(
            MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
    })?;
    cx.run_until_parked();
    Ok(())
}

fn press_key(
    cx: &mut HeadlessAppContext,
    window: WindowHandle<Preview>,
    key: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_event(
            KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            app,
        );
    })?;
    Ok(())
}

fn opening_a_pdf_and_zooming_changes_the_rendered_page() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("KVER_TEST_PDF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/readme-sample.pdf"));
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(path, window.scale_factor(), cx))
    })?;

    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(30);
    let page_count = loop {
        cx.run_until_parked();
        if let Some(count) = cx.update(|app| window.read(app).unwrap().page_count()) {
            break count;
        }
        if Instant::now() >= deadline {
            return Err("Document did not render its first page in 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    cx.forbid_parking();
    if page_count < 3 {
        return Err("Visual navigation check needs at least three pages".into());
    }
    let middle_index = page_count / 2 - 1;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().status().to_owned()),
        format!("Page 1 of {page_count} · 100%")
    );
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().displayed_page_index()),
        Some(0)
    );
    let before = cx.capture_screenshot(window.into())?;
    let output = std::env::var_os("KVER_CAPTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/captures"));
    std::fs::create_dir_all(&output)?;
    before.save(output.join("before.png"))?;

    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_event(
            KeyDownEvent {
                keystroke: Keystroke::parse("right").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            app,
        );
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        1,
        "navigation updates before the next bitmap is ready"
    );
    wait_for_page(&mut cx, &window, &format!("Page 2 of {page_count} · 100%"))?;
    window.update(&mut cx, |reader, _, cx| reader.previous_page(cx))?;
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 100%"))?;

    window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 125%"))?;
    let after = cx.capture_screenshot(window.into())?;
    after.save(output.join("after.png"))?;

    assert_eq!(before.dimensions(), after.dimensions());
    let width = before.width() as usize;
    let page_region = (before.height() as usize / 4) * width;
    assert!(
        before
            .pixels()
            .zip(after.pixels())
            .skip(page_region)
            .any(|(a, b)| a != b),
        "zoom should change the page, not just the toolbar"
    );
    // Input-level zoom shares the toolbar command and retains the PDF-point
    // position at the top of a scrolled page (not the same screen pixel).
    window.update(&mut cx, |reader, _, cx| {
        reader.select_page(1, cx);
        reader.scroll_by(100.0, cx);
    })?;
    let position = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(position.0, 1);
    assert!(position.1 > 0.0);
    press_key(&mut cx, window, "=")?;
    assert_eq!(cx.update(|app| window.read(app).unwrap().page_index()), 1);
    let zoomed_position = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(zoomed_position.0, position.0);
    assert!((zoomed_position.1 - position.1).abs() < 0.01);
    wait_for_page(&mut cx, &window, &format!("Page 2 of {page_count} · 156%"))?;
    press_key(&mut cx, window, "-")?;
    wait_for_page(&mut cx, &window, &format!("Page 2 of {page_count} · 125%"))?;
    let restored = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(restored.0, position.0);
    assert!((restored.1 - position.1).abs() < 0.01);
    press_key(&mut cx, window, "shift-=")?;
    wait_for_page(&mut cx, &window, &format!("Page 2 of {page_count} · 156%"))?;
    press_key(&mut cx, window, "-")?;
    wait_for_page(&mut cx, &window, &format!("Page 2 of {page_count} · 125%"))?;
    // Queue pages quickly, without waiting for any intermediate render.
    // Only the newest request may be installed by the reader.
    window.update(&mut cx, |reader, _, cx| {
        reader.next_page(cx);
        reader.select_page(middle_index + 1, cx);
        reader.previous_page(cx);
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        middle_index,
        "navigation must accept the latest request before the page finishes rendering"
    );
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {} of {page_count} · 125%", middle_index + 1),
    )?;
    let middle = cx.capture_screenshot(window.into())?;
    assert!(
        middle
            .pixels()
            .zip(after.pixels())
            .skip(page_region)
            .any(|(a, b)| a != b),
        "middle page should differ from first page"
    );
    middle.save(output.join("middle.png"))?;

    window.update(&mut cx, |reader, _, cx| {
        reader.select_page(page_count - 1, cx)
    })?;
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {page_count} of {page_count} · 125%"),
    )?;
    let last = cx.capture_screenshot(window.into())?;
    assert!(
        last.pixels()
            .zip(middle.pixels())
            .skip(page_region)
            .any(|(a, b)| a != b),
        "last page should differ from middle page"
    );
    last.save(output.join("last.png"))?;
    // Traverse the long sample with both rapid jumps and sustained scrolling.
    window.update(&mut cx, |reader, _, cx| reader.select_page(0, cx))?;
    for index in 1..page_count {
        window.update(&mut cx, |reader, _, cx| reader.select_page(index, cx))?;
    }
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {page_count} of {page_count} · 125%"),
    )?;
    let bytes = cx.update(|app| window.read(app).unwrap().retained_bitmap_bytes());
    assert!(
        bytes > 0 && bytes <= 128 * 1024 * 1024,
        "retained {bytes} bytes"
    );
    window.update(&mut cx, |reader, _, cx| reader.select_page(0, cx))?;
    for _ in 0..page_count * 2 {
        window.update(&mut cx, |reader, _, cx| reader.scroll_by(800.0, cx))?;
    }
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        page_count - 1
    );
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {page_count} of {page_count} · 125%"),
    )?;
    window.update(&mut cx, |reader, _, cx| reader.scroll_by(-800.0, cx))?;
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {} of {page_count} · 125%", page_count - 1),
    )?;
    cx.capture_screenshot(window.into())?
        .save(output.join("scrolled.png"))?;
    assert!(
        cx.update(|app| window.read(app).unwrap().page_index()) < page_count - 1,
        "scrolling must move into the preceding page"
    );
    // Dispatch a real wheel event through GPUI, not just the reader command.
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_event(
            ScrollEvent {
                position: point(px(500.0), px(500.0)),
                delta: ScrollDelta::Pixels(point(px(0.0), px(-800.0))),
                ..Default::default()
            }
            .to_platform_input(),
            app,
        );
    })?;
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {page_count} of {page_count} · 125%"),
    )?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        page_count - 1
    );
    window.update(&mut cx, |reader, _, cx| {
        reader.select_page(page_count - 1, cx)
    })?;
    window.update(&mut cx, |reader, _, cx| reader.next_page(cx))?;
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {page_count} of {page_count} · 125%"),
    )?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        page_count - 1
    );
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().status().to_owned()),
        format!("Page {page_count} of {page_count} · 125%")
    );
    println!("Reader zoom passed; captures in {}", output.display());
    Ok(())
}

fn loading_error_is_visible(
    path: PathBuf,
    expected_prefix: &str,
    capture: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(path, window.scale_factor(), cx))
    })?;
    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.run_until_parked();
        let status = cx.update(|app| window.read(app).unwrap().status().to_owned());
        if status.starts_with(expected_prefix) {
            println!("Loading error: {status}");
            break;
        }
        if !(status.starts_with("Rendering page ") || status == "Loading document…")
            || Instant::now() >= deadline
        {
            return Err(format!("Loading error not shown: {status}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cx.forbid_parking();
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_count()),
        None
    );
    if let Some(name) = capture {
        cx.capture_screenshot(window.into())?
            .save(PathBuf::from("target/captures").join(name))?;
    }
    Ok(())
}

fn loading_errors_are_visible() -> Result<(), Box<dyn std::error::Error>> {
    loading_error_is_visible(PathBuf::from("examples/missing.pdf"), "Cannot read ", None)?;
    std::fs::create_dir_all("target/captures")?;
    let invalid_path = PathBuf::from("target/captures/not-a-pdf.pdf");
    std::fs::write(&invalid_path, b"This is not a PDF")?;
    loading_error_is_visible(
        invalid_path,
        "Cannot open PDF: invalid or unsupported PDF file",
        Some("invalid-pdf.png"),
    )?;
    Ok(())
}

/// Opt-in diagnostic, not a CI performance gate: Metal screenshot readback is
/// not equivalent to an onscreen frame present.
fn measure_page_turns() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("KVER_BENCH_PDF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/readme-sample.pdf"));
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(path, window.scale_factor(), cx))
    })?;
    let density = cx.update_window(window.into(), |_, window, _| window.scale_factor())?;
    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(30);
    let count = loop {
        cx.run_until_parked();
        if let Some(count) = cx.update(|app| window.read(app).unwrap().page_count()) {
            break count;
        }
        if Instant::now() >= deadline {
            return Err("Benchmark document did not load in 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    cx.forbid_parking();
    if count < 3 {
        return Err("Benchmark PDF needs at least three pages".into());
    }
    let base = count / 2;
    window.update(&mut cx, |reader, _, cx| reader.select_page(base - 1, cx))?;
    wait_for_page(&mut cx, &window, &format!("Page {base} of {count} · 100%"))?;
    let zoom = if std::env::var_os("KVER_BENCH_ZOOM_IN").is_some() {
        window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
        wait_for_page(&mut cx, &window, &format!("Page {base} of {count} · 125%"))?;
        125
    } else {
        100
    };
    let mut previous = cx.capture_screenshot(window.into())?;
    println!("density={density:.2} zoom={zoom}% pages={count} base_page={base}");
    for turn in 0..8 {
        let next = turn % 2 == 0;
        let expected = format!(
            "Page {} of {count} · {zoom}%",
            if next { base + 1 } else { base }
        );
        let start = Instant::now();
        window.update(&mut cx, |reader, _, cx| {
            if next {
                reader.next_page(cx);
            } else {
                reader.previous_page(cx);
            }
        })?;
        wait_for_page(&mut cx, &window, &expected)?;
        let accepted_ms = start.elapsed().as_secs_f64() * 1000.0;
        let frame = cx.capture_screenshot(window.into())?;
        let captured_ms = start.elapsed().as_secs_f64() * 1000.0;
        let page_start = previous.width() as usize * previous.height() as usize / 4;
        assert!(
            previous
                .pixels()
                .zip(frame.pixels())
                .skip(page_start)
                .any(|(a, b)| a != b),
            "the captured frame must show the newly requested page"
        );
        println!(
            "turn={} accepted_ms={accepted_ms:.1} captured_ms={captured_ms:.1}",
            turn + 1
        );
        previous = frame;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("KVER_BENCH").is_some() {
        return measure_page_turns();
    }
    opening_a_pdf_and_zooming_changes_the_rendered_page()?;
    loading_errors_are_visible()
}
