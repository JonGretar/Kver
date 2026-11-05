//! Main-thread Cargo test: drive the actual reader, assert state, and inspect PNGs.
//!
//! Set `KVER_BENCH=1` for the opt-in page-turn timing diagnostic.
//! Rust's default #[test] runner uses worker threads; Metal requires the
//! process main thread.

use gpui_kit as gpui;
use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::platform as gpui_platform;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, AsKeystroke as _, HeadlessAppContext, InputEvent as _,
    KeyDownEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    OwnedMenuItem, Pixels, Point, ScrollDelta, ScrollEvent, WindowHandle, point, px, size,
};
use kver::app_menu::{self, Quit};
use kver::assets::ReaderAssets;
use kver::reader::{Copy, OpenPdf, Preview, ZoomIn, ZoomMode, bind_reader_keys};
use kver::reader_sidebar::SidebarPanel;
use kver::reader_windows::{NewWindow, NextWindow, PreviousWindow};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[track_caller]
fn wait_for_page(
    cx: &mut HeadlessAppContext,
    window: &WindowHandle<Preview>,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let view = window.root(cx)?;
    wait_for_view(cx, &view, expected)
}

#[track_caller]
fn wait_for_view(
    cx: &mut HeadlessAppContext,
    reader: &gpui::Entity<Preview>,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    cx.allow_parking();
    loop {
        cx.run_until_parked();
        let status = cx.update(|app| reader.read(app).status().to_owned());
        if status == expected {
            cx.forbid_parking();
            return Ok(());
        }
        if !(status.starts_with("Rendering page ") || status == "Loading document…")
            || Instant::now() >= deadline
        {
            let (index, displayed) = cx.update(|app| {
                let reader = reader.read(app);
                (reader.page_index(), reader.displayed_page_index())
            });
            return Err(format!(
                "{}: Expected {expected:?}, got {status:?} (page_index={index}, displayed={displayed:?})",
                std::panic::Location::caller()
            )
            .into());
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

fn click_control(
    cx: &mut HeadlessAppContext,
    window: AnyWindowHandle,
    id: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    cx.update_window(window, |_, frame, app| {
        frame.render_frame(app);
        frame.click(id, app);
    })?;
    cx.run_until_parked();
    Ok(())
}

fn click_panel_tab(
    cx: &mut HeadlessAppContext,
    window: AnyWindowHandle,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let position = cx.update_window(window, |_, frame, app| {
        frame.render_frame(app);
        frame.find_by_label(label).bounds().center()
    })?;
    click(cx, window, position)
}

fn press_key(
    cx: &mut HeadlessAppContext,
    window: impl Into<AnyWindowHandle>,
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

fn type_query(
    cx: &mut HeadlessAppContext,
    window: impl Into<AnyWindowHandle>,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let window = window.into();
    for character in text.chars() {
        let key = character.to_string();
        cx.update_window(window, |_, window, app| {
            window.dispatch_keystroke(
                Keystroke {
                    key: key.clone(),
                    key_char: Some(key.clone()),
                    modifiers: Modifiers::default(),
                },
                app,
            );
        })?;
    }
    cx.run_until_parked();
    Ok(())
}

// Exercise mixed page sizes through the reader, not only the fit calculation.
fn pdf_with_page_sizes(sizes: [(u32, u32); 3]) -> Vec<u8> {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << >> >>",
            sizes[0].0, sizes[0].1
        ),
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << >> >>",
            sizes[1].0, sizes[1].1
        ),
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << >> >>",
            sizes[2].0, sizes[2].1
        ),
    ];
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}

fn contents_pdf() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 384 504] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [10 20 210 320] /CropBox [30 50 130 250] /Rotate 90 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 384 504] >>",
        "<< /Type /Outlines /First 7 0 R /Last 9 0 R >>",
        "<< /Title (Beginning) /Dest [3 0 R /Fit] /Next 8 0 R /First 10 0 R >>",
        "<< /Title (Rotated target) /Dest [4 0 R /XYZ 100 80 0] /Next 9 0 R >>",
        "<< /Title (Broken target) /Dest [99 0 R /Fit] >>",
        "<< /Title (Nested end) /Dest [5 0 R /Fit] >>",
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

// A real PDF page stream with a nonzero crop origin, 90° rotation, text
// crossing the crop edge, and entirely hidden text.
fn rotated_selection_pdf() -> Vec<u8> {
    let stream = b"BT /F1 16 Tf 40 100 Td (VISIBLE) Tj ET BT /F1 16 Tf 126 100 Td (CROSS) Tj ET BT /F1 16 Tf 40 280 Td (HIDDEN) Tj ET";
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [10 20 210 320] /CropBox [30 50 130 250] /Rotate 90 /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes().into_iter().chain(stream.iter().copied()).chain(b"\nendstream".iter().copied()).collect(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}

fn image_only_pdf() -> Vec<u8> {
    let image = [0xaa_u8; 64];
    let stream = b"q 300 0 0 400 40 40 cm /Im1 Do Q";
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 384 504] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes().into_iter().chain(stream.iter().copied()).chain(b"\nendstream".iter().copied()).collect(),
        format!("<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length {} >>\nstream\n", image.len()).into_bytes().into_iter().chain(image).chain(b"\nendstream".iter().copied()).collect(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}

fn mixed_page_pdf() -> Vec<u8> {
    pdf_with_page_sizes([(612, 792), (300, 400), (600, 1200)])
}

fn mixed_page_pdf_with_last_height(height: u32) -> Vec<u8> {
    pdf_with_page_sizes([(612, 792), (300, 400), (600, height)])
}

fn fit_page_transition_is_stable() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/captures")?;
    let path = PathBuf::from("target/captures/short-rotated-sizes.pdf");
    std::fs::write(
        &path,
        pdf_with_page_sizes([(384, 504), (378, 288), (352, 294)]),
    )?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
    press_key(&mut cx, window, "f")?;
    let mut highest_page = 0;
    for _ in 0..20 {
        window.update(&mut cx, |reader, _, cx| reader.scroll_by(80.0, cx))?;
        cx.capture_screenshot(window.into())?;
        let (page, zoom) = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (reader.page_index(), reader.scale())
        });
        assert!(
            page >= highest_page,
            "fit-page went backwards while scrolling forward"
        );
        highest_page = page;
        cx.capture_screenshot(window.into())?;
        let after = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (reader.page_index(), reader.scale())
        });
        assert_eq!(after, (page, zoom), "fit-page changed without scrolling");
    }
    assert!(highest_page >= 1, "fit-page scrolling must reach page two");
    let mut lowest_page = highest_page;
    for _ in 0..20 {
        window.update(&mut cx, |reader, _, cx| reader.scroll_by(-80.0, cx))?;
        cx.capture_screenshot(window.into())?;
        let page = cx.update(|app| window.read(app).unwrap().page_index());
        assert!(
            page <= lowest_page,
            "fit-page went forward while scrolling backward"
        );
        lowest_page = page;
    }
    assert_eq!(lowest_page, 0, "fit-page scrolling must return to page one");
    Ok(())
}

fn short_final_page_fits_without_feedback() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/captures")?;
    let path = PathBuf::from("target/captures/short-last-page.pdf");
    std::fs::write(&path, mixed_page_pdf_with_last_height(180))?;
    for mode in [ZoomMode::FitWidth, ZoomMode::FitPage] {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_platform::current_platform(true).text_system(),
            Arc::new(ReaderAssets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(gpui_kit::init);
        let path = path.clone();
        let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
            cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
        })?;
        wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
        window.update(&mut cx, |reader, _, cx| {
            if mode == ZoomMode::FitWidth {
                reader.fit_width(cx);
            } else {
                reader.fit_page(cx);
            }
            reader.select_page(1, cx);
        })?;
        // A thin strip of page two remains visible above the short third page.
        for _ in 0..5 {
            window.update(&mut cx, |reader, _, cx| reader.scroll_by(800.0, cx))?;
            cx.run_until_parked();
        }
        let (page, scale) = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (reader.page_index(), reader.scale())
        });
        assert_eq!(
            page, 2,
            "{mode:?} must identify the mostly visible final page"
        );
        for _ in 0..2 {
            cx.capture_screenshot(window.into())?;
            assert_eq!(
                cx.update(|app| window.read(app).unwrap().scale()),
                scale,
                "{mode:?} must not refit again without input"
            );
        }
        window.update(&mut cx, |reader, _, cx| reader.previous_page(cx))?;
        window.update(&mut cx, |reader, _, cx| reader.next_page(cx))?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().page_index()),
            2,
            "{mode:?} must navigate to the short final page"
        );
        // Walk back across the differently sized page boundary and return.
        // A fit change must not itself trigger repeated fit changes.
        let mut previous_stable = None;
        for amount in [-200.0; 12].into_iter().chain([200.0; 12]) {
            window.update(&mut cx, |reader, _, cx| reader.scroll_by(amount, cx))?;
            cx.capture_screenshot(window.into())?;
            let stable = cx.update(|app| {
                let reader = window.read(app).unwrap();
                (reader.page_index(), reader.scale())
            });
            // Verify another frame at each page/fit transition, where
            // feedback loops would change the selected page without input.
            if previous_stable != Some(stable) {
                cx.capture_screenshot(window.into())?;
                let after = cx.update(|app| {
                    let reader = window.read(app).unwrap();
                    (reader.page_index(), reader.scale())
                });
                assert_eq!(after, stable, "{mode:?} refit without scrolling");
            }
            previous_stable = Some(stable);
        }
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().page_index()),
            2,
            "{mode:?} must reach the final page again"
        );
    }
    Ok(())
}

fn fit_modes_follow_page_and_window() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/captures")?;
    let path = PathBuf::from("target/captures/mixed-page.pdf");
    std::fs::write(&path, mixed_page_pdf())?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
    window.update(&mut cx, |reader, _, cx| reader.fit_width(cx))?;
    let first = cx.update(|app| window.read(app).unwrap().scale());
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().zoom_mode()),
        ZoomMode::FitWidth
    );
    assert!(first > 1.0, "first page should fit the available width");
    window.update(&mut cx, |reader, _, cx| reader.select_page(1, cx))?;
    let second = cx.update(|app| window.read(app).unwrap().scale());
    assert!((second - first * 612.0 / 300.0).abs() < 0.01);
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().reading_position()),
        (1, 0.0)
    );
    window.update(&mut cx, |reader, _, cx| reader.select_page(2, cx))?;
    let third = cx.update(|app| window.read(app).unwrap().scale());
    assert!((third - first * 612.0 / 600.0).abs() < 0.01);
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().reading_position()),
        (2, 0.0)
    );

    window.update(&mut cx, |reader, _, cx| {
        reader.select_page(0, cx);
        reader.scroll_by(120.0, cx);
    })?;
    let position = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(position.0, 0);
    window.update(&mut cx, |reader, _, cx| reader.fit_page(cx))?;
    let fitted = cx.update(|app| window.read(app).unwrap().scale());
    assert!(fitted < first);
    let after_fit = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(after_fit.0, position.0);
    assert!((after_fit.1 - position.1).abs() < 0.01);

    // The headless platform's resize does not emit its native callback;
    // explicitly deliver it as a real window does.
    cx.update_window(window.into(), |_, window, app| {
        window.resize(size(px(600.0), px(700.0)));
        window.bounds_changed(app);
    })?;
    cx.run_until_parked();
    let resized = cx.update(|app| window.read(app).unwrap().scale());
    assert!(resized < fitted, "fit page must respond to window resize");
    let after_resize = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(after_resize.0, position.0);
    assert!((after_resize.1 - position.1).abs() < 0.01);
    press_key(&mut cx, window, "=")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().zoom_mode()),
        ZoomMode::Fixed
    );
    let manual = cx.update(|app| window.read(app).unwrap().scale());
    assert!((manual - resized * 1.25).abs() < 0.01);
    cx.update_window(window.into(), |_, window, app| {
        window.resize(size(px(920.0), px(800.0)));
        window.bounds_changed(app);
    })?;
    cx.run_until_parked();
    assert_eq!(cx.update(|app| window.read(app).unwrap().scale()), manual);
    window.update(&mut cx, |reader, _, cx| reader.select_page(1, cx))?;
    assert_eq!(cx.update(|app| window.read(app).unwrap().scale()), manual);
    Ok(())
}

fn opening_a_pdf_and_zooming_changes_the_rendered_page() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("KVER_TEST_PDF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/readme-sample.pdf"));
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
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
    let light_background = cx.update(|app| Theme::global(app).background);
    cx.update(|app| Theme::change(ThemeMode::Dark, None, app));
    assert!(cx.update(|app| Theme::global(app).is_dark()));
    assert_ne!(
        light_background,
        cx.update(|app| Theme::global(app).background)
    );
    window.update(&mut cx, |_, _, cx| cx.notify())?;
    cx.run_until_parked();
    let dark = cx.capture_screenshot(window.into())?;
    dark.save(output.join("reader-loaded-dark.png"))?;
    assert_ne!(before.get_pixel(0, 0), dark.get_pixel(0, 0));
    cx.update(|app| Theme::change(ThemeMode::Light, None, app));

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
        reader.select_page(0, cx);
        reader.scroll_by(100.0, cx);
    })?;
    let position = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(position.0, 0);
    assert!(position.1 > 0.0);
    press_key(&mut cx, window, "=")?;
    assert_eq!(cx.update(|app| window.read(app).unwrap().page_index()), 0);
    let zoomed_position = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(zoomed_position.0, position.0);
    assert!((zoomed_position.1 - position.1).abs() < 0.01);
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 156%"))?;
    press_key(&mut cx, window, "-")?;
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 125%"))?;
    let restored = cx.update(|app| window.read(app).unwrap().reading_position());
    assert_eq!(restored.0, position.0);
    assert!((restored.1 - position.1).abs() < 0.01);
    press_key(&mut cx, window, "shift-=")?;
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 156%"))?;
    press_key(&mut cx, window, "-")?;
    wait_for_page(&mut cx, &window, &format!("Page 1 of {page_count} · 125%"))?;
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
    let mut peak_bitmap_bytes = cx.update(|app| window.read(app).unwrap().retained_bitmap_bytes());
    window.update(&mut cx, |reader, _, cx| reader.select_page(0, cx))?;
    for index in 1..page_count {
        window.update(&mut cx, |reader, _, cx| reader.select_page(index, cx))?;
        let bytes = cx.update(|app| window.read(app).unwrap().retained_bitmap_bytes());
        peak_bitmap_bytes = peak_bitmap_bytes.max(bytes);
        assert!(
            bytes <= 128 * 1024 * 1024,
            "page {index} retained {bytes} bytes"
        );
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
        let bytes = cx.update(|app| window.read(app).unwrap().retained_bitmap_bytes());
        peak_bitmap_bytes = peak_bitmap_bytes.max(bytes);
        assert!(bytes <= 128 * 1024 * 1024, "scroll retained {bytes} bytes");
    }
    let end_page = cx.update(|app| window.read(app).unwrap().page_index());
    assert!(
        end_page >= page_count - 2,
        "scrolling must reach the end of the document (top visible page {})",
        end_page + 1
    );
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {} of {page_count} · 125%", end_page + 1),
    )?;
    window.update(&mut cx, |reader, _, cx| reader.scroll_by(-800.0, cx))?;
    let back_page = cx.update(|app| window.read(app).unwrap().page_index());
    assert!(
        back_page < end_page,
        "backward scroll must change current page"
    );
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {} of {page_count} · 125%", back_page + 1),
    )?;
    cx.capture_screenshot(window.into())?
        .save(output.join("scrolled.png"))?;
    assert!(
        cx.update(|app| window.read(app).unwrap().page_index()) < page_count - 1,
        "scrolling must move into the preceding page"
    );
    // Dispatch a real wheel event through GPUI, not just the reader command.
    // On a short last page the viewport cannot scroll far enough to put
    // that page at its top; the reading position can stay on the prior page.
    let before_wheel = cx.update(|app| window.read(app).unwrap().reading_position());
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
    cx.run_until_parked();
    let after_wheel = cx.update(|app| window.read(app).unwrap().reading_position());
    let wheel_page = cx.update(|app| window.read(app).unwrap().page_index()) + 1;
    assert!(
        after_wheel.0 > before_wheel.0
            || (after_wheel.0 == before_wheel.0 && after_wheel.1 >= before_wheel.1),
        "wheel must not scroll backwards: {before_wheel:?} -> {after_wheel:?}"
    );
    wait_for_page(
        &mut cx,
        &window,
        &format!("Page {wheel_page} of {page_count} · 125%"),
    )?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_index()),
        wheel_page - 1
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
    press_key(&mut cx, window, "w")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().zoom_mode()),
        ZoomMode::FitWidth
    );
    let width_scale = cx.update(|app| window.read(app).unwrap().scale());
    assert!(width_scale.is_finite() && (0.25..=4.0).contains(&width_scale));
    press_key(&mut cx, window, "f")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().zoom_mode()),
        ZoomMode::FitPage
    );
    let page_scale = cx.update(|app| window.read(app).unwrap().scale());
    assert!(page_scale <= width_scale);
    println!(
        "Reader case: pages={page_count} peak_retained_bitmap_bytes={peak_bitmap_bytes} \
         final_retained_bitmap_bytes={bytes} fit_width={width_scale:.3} fit_page={page_scale:.3}; \
         captures in {}",
        output.display()
    );
    Ok(())
}

fn loading_error_is_visible(
    path: PathBuf,
    expected_prefix: &str,
    capture: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
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

fn opening_from_empty_window_and_replacing_documents() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_count()),
        None
    );
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().status().to_owned()),
        "Open a PDF to start reading"
    );
    std::fs::create_dir_all("target/captures")?;
    let light = cx.capture_screenshot(window.into())?;
    light.save("target/captures/reader-empty-light.png")?;
    cx.update(|app| Theme::change(ThemeMode::Dark, None, app));
    assert!(cx.update(|app| Theme::global(app).is_dark()));
    window.update(&mut cx, |_, _, cx| cx.notify())?;
    cx.run_until_parked();
    let dark = cx.capture_screenshot(window.into())?;
    dark.save("target/captures/reader-empty-dark.png")?;
    assert_ne!(light.get_pixel(0, 0), dark.get_pixel(0, 0));
    cx.update(|app| Theme::change(ThemeMode::Light, None, app));
    window.update(&mut cx, |_, _, cx| cx.notify())?;
    cx.update_window(window.into(), |_, frame, app| {
        frame.render_frame(app);
        let center = frame.find("open-pdf-center");
        assert!(center.visible());
        assert!(center.bounds().center().y > px(300.0));
        assert!(center.bounds().center().y < px(600.0));
        assert!(frame.find("title-bar").visible());
        assert!(frame.find("open-pdf").visible());
    })?;
    // A native file picker can appear even in a headless window. Check
    // discoverability here; exercise the actual picker in a visible Mac window.
    let sample = PathBuf::from("examples/readme-sample.pdf");
    let invalid = PathBuf::from("target/captures/not-a-pdf.pdf");
    std::fs::create_dir_all("target/captures")?;
    std::fs::write(&invalid, b"This is not a PDF")?;
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(sample.clone(), cx)
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().displayed_page_index()),
        Some(0)
    );
    window.update(&mut cx, |reader, _, cx| {
        reader.select_page(50, cx);
        reader.open_path(invalid.clone(), cx);
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_count()),
        None
    );
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().displayed_page_index()),
        None
    );
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().retained_bitmap_bytes()),
        0
    );
    wait_for_page(
        &mut cx,
        &window,
        "Cannot open PDF: invalid or unsupported PDF file",
    )?;
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(PathBuf::from("examples/missing.pdf"), cx)
    })?;
    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.run_until_parked();
        let status = cx.update(|app| window.read(app).unwrap().status().to_owned());
        if status.starts_with("Cannot read examples/missing.pdf") {
            break;
        }
        if status != "Loading document…" || Instant::now() >= deadline {
            return Err(format!("Missing file error not shown: {status}").into());
        }
    }
    cx.forbid_parking();
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(sample.clone(), cx)
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")?;
    assert_eq!(cx.update(|app| window.read(app).unwrap().page_index()), 0);
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().path().map(|p| p.to_path_buf())),
        Some(sample)
    );
    // Both loads can be outstanding; only the newest document may win.
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(invalid, cx);
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().page_count()),
        Some(100)
    );
    Ok(())
}

fn long_path_keeps_toolbar_clickable() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from("target/captures")
        .join("long-directory-name-for-testing-toolbar-layout".repeat(5));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.pdf", "long-reader-filename-".repeat(7)));
    std::fs::copy("examples/readme-sample.pdf", &path)?;
    for width in [920.0, 440.0, 360.0] {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_platform::current_platform(true).text_system(),
            Arc::new(ReaderAssets),
            gpui_platform::current_headless_renderer,
        );
        cx.update(|app| {
            gpui_kit::init(app);
            bind_reader_keys(app);
        });
        let path = path.clone();
        let window = cx.open_window(size(px(width), px(800.0)), |window, cx| {
            cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
        })?;
        wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")?;
        let capture = cx.capture_screenshot(window.into())?;
        capture.save(format!("target/captures/long-filename-{width}.png"))?;
        // Resolve Kit controls by identity and hit-test their actual bounds.
        cx.update_window(window.into(), |_, frame, app| {
            frame.render_frame(app);
            for id in [
                "open-pdf",
                "previous-page",
                "next-page",
                "zoom-out",
                "zoom-in",
                "fit-width",
                "fit-page",
                "copy-selection",
            ] {
                let target = frame.find(id);
                assert!(
                    target.visible(),
                    "{id} is clipped at width {width}: {target:?}"
                );
                assert!(
                    target.bounds().right() <= px(width),
                    "{id} extends past width {width}"
                );
            }
            for (id, label) in [("fit-width", "Fit width"), ("fit-page", "Fit page")] {
                let button = frame.find(id);
                assert_eq!(button.label(), Some(label));
                if width < 560.0 {
                    assert!(
                        button.bounds().size.width <= px(32.0),
                        "{id} should use its icon at width {width}"
                    );
                } else {
                    assert!(
                        button.bounds().size.width > px(32.0),
                        "{id} should retain its visible label at width {width}"
                    );
                }
            }
        })?;
        click_control(&mut cx, window.into(), "previous-page")?;
        assert_eq!(cx.update(|app| window.read(app).unwrap().page_index()), 0);
        click_control(&mut cx, window.into(), "next-page")?;
        wait_for_page(&mut cx, &window, "Page 2 of 100 · 100%").map_err(|error| {
            format!("next-page must remain reachable at width {width}: {error}")
        })?;
        click_control(&mut cx, window.into(), "previous-page")?;
        wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%").map_err(|error| {
            format!("previous-page must remain reachable at width {width}: {error}")
        })?;
        click_control(&mut cx, window.into(), "zoom-in")?;
        wait_for_page(&mut cx, &window, "Page 1 of 100 · 125%")
            .map_err(|error| format!("zoom-in must remain reachable at width {width}: {error}"))?;
        click_control(&mut cx, window.into(), "zoom-out")?;
        wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")
            .map_err(|error| format!("zoom-out must remain reachable at width {width}: {error}"))?;
        click_control(&mut cx, window.into(), "fit-width")?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().zoom_mode()),
            ZoomMode::FitWidth,
            "fit-width button must remain reachable at width {width}"
        );
        click_control(&mut cx, window.into(), "fit-page")?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().zoom_mode()),
            ZoomMode::FitPage,
            "fit-page button must remain reachable at width {width}"
        );
        click_control(&mut cx, window.into(), "zoom-in")?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().zoom_mode()),
            ZoomMode::Fixed,
            "manual zoom must leave fit mode at width {width}"
        );
        press_key(&mut cx, window, "cmd-shift-p")?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, frame, app| {
            frame.render_frame(app);
            assert_eq!(
                frame.find("reader-command-palette").role(),
                Some(gpui::Role::Dialog)
            );
            let palette = frame.find("command");
            assert!(palette.visible(), "palette is clipped at width {width}");
            assert!(
                palette.bounds().right() <= px(width),
                "palette extends past width {width}"
            );
            assert!(
                palette.bounds().left() >= px(0.0),
                "palette extends before width {width}"
            );
        })?;
        if width == 360.0 {
            cx.capture_screenshot(window.into())?
                .save("target/captures/palette-narrow.png")?;
        }
        press_key(&mut cx, window, "escape")?;
        cx.run_until_parked();
        assert!(!cx.update(|app| window.read(app).unwrap().palette_open()));
    }
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
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
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

fn open_pdf_palette_in_empty_window() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
    })?;
    cx.capture_screenshot(window.into())?;
    press_key(&mut cx, window, "cmd-shift-p")?;
    cx.run_until_parked();
    assert!(cx.update(|app| window.read(app).unwrap().palette_open()));
    cx.capture_screenshot(window.into())?
        .save("target/captures/open-pdf-palette.png")?;
    // Escape clears a query before dismissing the palette; focus returns
    // to the reader so the shortcut can reopen it.
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_keystroke(
            Keystroke {
                key: "x".into(),
                key_char: Some("x".into()),
                modifiers: Modifiers::default(),
            },
            app,
        );
    })?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().palette_query(app)),
        Some("x".into())
    );
    press_key(&mut cx, window, "escape")?;
    cx.run_until_parked();
    assert!(cx.update(|app| window.read(app).unwrap().palette_open()));
    press_key(&mut cx, window, "escape")?;
    cx.run_until_parked();
    assert!(!cx.update(|app| window.read(app).unwrap().palette_open()));
    press_key(&mut cx, window, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, window, "Open PDF")?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, window, "escape")?;
    cx.run_until_parked();
    press_key(&mut cx, window, "escape")?;
    cx.run_until_parked();
    assert!(!cx.update(|app| window.read(app).unwrap().palette_open()));
    press_key(&mut cx, window, "cmd-shift-p")?;
    cx.run_until_parked();
    cx.capture_screenshot(window.into())?;
    click(&mut cx, window.into(), point(px(460.0), px(420.0)))?;
    assert!(!cx.update(|app| window.read(app).unwrap().palette_open()));
    Ok(())
}

fn sidebar_host() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let first = cx.open_window(size(px(920.), px(800.)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some("examples/readme-sample.pdf".into()),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    let second = cx.open_window(size(px(920.), px(800.)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some("examples/readme-sample.pdf".into()),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &first, "Page 1 of 100 · 100%")?;
    wait_for_page(&mut cx, &second, "Page 1 of 100 · 100%")?;
    let first_reader = first.root(&mut cx)?;
    let second_reader = second.root(&mut cx)?;
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find("toggle-sidebar").visible());
        assert!(
            window.find("toggle-sidebar").bounds().left()
                < window.find("previous-page").bounds().left()
        );
        window.click("toggle-sidebar", app);
    })?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    click_control(&mut cx, first.into(), "toggle-sidebar")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    press_key(&mut cx, first, "cmd-f")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    press_key(&mut cx, first, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, first, "toggle sidebar")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, first, "enter")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    press_key(&mut cx, first, "cmd-f")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| second_reader.read(app).sidebar_panel()),
        None
    );
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find("reader-sidebar").visible());
        assert!(window.find("find-query").visible());
        assert_eq!(window.find("find-query").focused(), Some(true));
        let sidebar = window.find("reader-sidebar").bounds();
        let page = window.find("page").bounds();
        assert!(sidebar.right() <= page.left());
        assert!(sidebar.top() < page.top());
        assert!(sidebar.bottom() > page.bottom());
        assert!(page.size.width > px(500.));
    })?;
    type_query(&mut cx, first, "nfpw=-")?;
    assert_eq!(cx.update(|app| first_reader.read(app).page_index()), 0);
    assert_eq!(cx.update(|app| first_reader.read(app).scale()), 1.);
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert_eq!(window.find("find-query").value(), Some("nfpw=-"));
        window.click("zoom-in", app);
    })?;
    assert_eq!(cx.update(|app| first_reader.read(app).scale()), 1.25);
    click_panel_tab(&mut cx, first.into(), "Contents")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Contents)
    );
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.try_find("find-query").is_none());
        assert!(window.find("contents-panel").visible());
    })?;
    click_control(&mut cx, first.into(), "toggle-sidebar")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    click_control(&mut cx, first.into(), "toggle-sidebar")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Contents)
    );
    press_key(&mut cx, first, "escape")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    press_key(&mut cx, first, "cmd-f")?;
    cx.run_until_parked();
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert_eq!(window.find("find-query").value(), Some("nfpw=-"));
        assert_eq!(window.find("find-query").focused(), Some(true));
    })?;
    click_panel_tab(&mut cx, first.into(), "Contents")?;
    click_panel_tab(&mut cx, first.into(), "Find")?;
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert_eq!(window.find("find-query").value(), Some("nfpw=-"));
    })?;
    click_control(&mut cx, first.into(), "toggle-sidebar")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    let focus = cx.update(|app| first_reader.read(app).reader_focus());
    assert!(cx.update_window(first.into(), |_, window, _| focus.is_focused(window))?);
    press_key(&mut cx, first, "n")?;
    assert_eq!(cx.update(|app| first_reader.read(app).page_index()), 1);
    press_key(&mut cx, first, "cmd-f")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    press_key(&mut cx, first, "escape")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    press_key(&mut cx, second, "cmd-f")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| second_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    first_reader.update(&mut cx, |reader, app| {
        reader.open_path("examples/readme-sample.pdf".into(), app);
    });
    wait_for_view(&mut cx, &first_reader, "Page 1 of 100 · 100%")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        None
    );
    press_key(&mut cx, first, "cmd-f")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    assert_eq!(
        cx.update(|app| second_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    std::fs::create_dir_all("target/captures")?;
    let replacement = PathBuf::from("target/captures/sidebar-replacement.pdf");
    std::fs::write(&replacement, mixed_page_pdf())?;
    first_reader.update(&mut cx, |reader, app| {
        reader.open_path(replacement.clone(), app)
    });
    wait_for_view(&mut cx, &first_reader, "Page 1 of 3 · 100%")?;
    assert_eq!(
        cx.update(|app| first_reader.read(app).sidebar_panel()),
        Some(SidebarPanel::Find)
    );
    cx.update_window(first.into(), |_, window, app| {
        window.render_frame(app);
        assert_eq!(window.find_all("reader-sidebar").len(), 1);
    })?;
    assert_eq!(
        cx.update(|app| second_reader.read(app).page_count()),
        Some(100)
    );
    std::fs::remove_file(replacement)?;
    let narrow = cx.open_window(size(px(360.), px(500.)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some("examples/readme-sample.pdf".into()),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &narrow, "Page 1 of 100 · 100%")?;
    press_key(&mut cx, narrow, "cmd-f")?;
    cx.run_until_parked();
    cx.update_window(narrow.into(), |_, window, app| {
        window.render_frame(app);
        let sidebar = window.find("reader-sidebar").bounds();
        let page = window.find("page").bounds();
        assert!(page.size.width > sidebar.size.width);
        let tabs = window.find_by_label("Contents").bounds();
        assert!(tabs.top() > page.top());
        assert!(tabs.bottom() <= sidebar.bottom());
        assert!(tabs.size.width > sidebar.size.width / 3.);
    })?;
    std::fs::create_dir_all("target/captures")?;
    cx.capture_screenshot(narrow.into())?
        .save("target/captures/sidebar-narrow-light.png")?;
    cx.update(|app| Theme::change(ThemeMode::Dark, None, app));
    narrow.update(&mut cx, |_, _, app| app.notify())?;
    cx.run_until_parked();
    cx.capture_screenshot(narrow.into())?
        .save("target/captures/sidebar-narrow-dark.png")?;
    cx.update_window(narrow.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find("reader-sidebar").visible());
        assert!(window.find("find-query").visible());
        window.click("toggle-sidebar", app);
    })?;
    Ok(())
}

fn shared_reader_actions() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let document = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    let empty = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
    })?;
    let error = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some(PathBuf::from("examples/missing.pdf")),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &document, "Page 1 of 100 · 100%")?;
    cx.capture_screenshot(document.into())?;
    cx.capture_screenshot(empty.into())?;

    press_key(&mut cx, document, "right")?;
    assert_eq!(cx.update(|app| document.read(app).unwrap().page_index()), 1);
    press_key(&mut cx, document, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, document, "next page")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, document, "enter")?;
    cx.run_until_parked();
    assert_eq!(cx.update(|app| document.read(app).unwrap().page_index()), 2);
    let focus = cx.update(|app| document.read(app).unwrap().reader_focus());
    assert!(cx.update_window(document.into(), |_, window, _| focus.is_focused(window))?);

    press_key(&mut cx, document, "=")?;
    assert_eq!(cx.update(|app| document.read(app).unwrap().scale()), 1.25);
    press_key(&mut cx, document, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, document, "zoom in")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    // Kit reads the item's hint from this active window binding, not a
    // separately maintained label. The bare "=" binding must not steal text.
    assert!(cx.update_window(document.into(), |_, window, _| {
        window
            .highest_precedence_binding_for_action(&ZoomIn)
            .is_some_and(|binding| {
                binding.keystrokes().first().map(|key| key.as_keystroke())
                    == Some(&Keystroke::parse("cmd-alt-=").unwrap())
            })
    })?);
    press_key(&mut cx, document, "enter")?;
    cx.run_until_parked();
    assert_eq!(cx.update(|app| document.read(app).unwrap().scale()), 1.5625);
    assert!(cx.update_window(document.into(), |_, window, _| focus.is_focused(window))?);

    press_key(&mut cx, document, "w")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().zoom_mode()),
        ZoomMode::FitWidth
    );
    let width_scale = cx.update(|app| document.read(app).unwrap().scale());
    press_key(&mut cx, document, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, document, "fit width")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, document, "enter")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().scale()),
        width_scale
    );
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().zoom_mode()),
        ZoomMode::FitWidth
    );
    press_key(&mut cx, document, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, document, "fit page")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, document, "enter")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().zoom_mode()),
        ZoomMode::FitPage
    );
    assert!(cx.update_window(document.into(), |_, window, _| focus.is_focused(window))?);

    let before = cx.update(|app| {
        let reader = document.read(app).unwrap();
        (reader.page_index(), reader.scale(), reader.zoom_mode())
    });
    press_key(&mut cx, empty, "right")?;
    press_key(&mut cx, empty, "=")?;
    press_key(&mut cx, empty, "w")?;
    press_key(&mut cx, empty, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, empty, "zoom in")?;
    assert_eq!(
        cx.update(|app| empty.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, empty, "enter")?;
    cx.run_until_parked();
    assert!(cx.update(|app| empty.read(app).unwrap().palette_open()));
    assert_eq!(cx.update(|app| empty.read(app).unwrap().scale()), 1.0);
    assert_eq!(cx.update(|app| empty.read(app).unwrap().page_count()), None);
    assert_eq!(
        cx.update(|app| {
            let reader = document.read(app).unwrap();
            (reader.page_index(), reader.scale(), reader.zoom_mode())
        }),
        before
    );
    cx.run_until_parked();
    assert!(cx.update(|app| error.read(app).unwrap().status().starts_with("Cannot read")));
    press_key(&mut cx, error, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, error, "fit page")?;
    assert_eq!(
        cx.update(|app| error.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, error, "enter")?;
    cx.run_until_parked();
    assert!(cx.update(|app| error.read(app).unwrap().palette_open()));
    assert_eq!(cx.update(|app| error.read(app).unwrap().page_count()), None);
    assert_eq!(
        cx.update(|app| {
            let reader = document.read(app).unwrap();
            (reader.page_index(), reader.scale(), reader.zoom_mode())
        }),
        before
    );
    document.update(&mut cx, |reader, _, app| {
        reader.open_path(PathBuf::from("examples/missing.pdf"), app);
    })?;
    cx.run_until_parked();
    assert!(cx.update(|app| {
        document
            .read(app)
            .unwrap()
            .status()
            .starts_with("Cannot read")
    }));
    press_key(&mut cx, document, "=")?;
    press_key(&mut cx, document, "f")?;
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().page_count()),
        None
    );
    assert_eq!(cx.update(|app| document.read(app).unwrap().scale()), 1.0);
    assert_eq!(
        cx.update(|app| document.read(app).unwrap().zoom_mode()),
        ZoomMode::Fixed
    );
    Ok(())
}

/// Inspect native menu Actions without opening a real picker during a headless test.
fn menu_actions_and_shortcuts() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
        app_menu::install(app);
    });
    let quit = cx.update(|app| {
        let menus = app.get_menus().unwrap();
        assert_eq!(menus.len(), 5);
        assert_eq!(menus[0].name.as_ref(), "Kver");
        assert_eq!(menus[1].name.as_ref(), "File");
        assert_eq!(menus[2].name.as_ref(), "Edit");
        assert!(matches!(&menus[2].items[0], OwnedMenuItem::Action { action, .. } if action.as_any().is::<Copy>()));
        assert_eq!(menus[3].name.as_ref(), "Window");
        assert_eq!(menus[4].name.as_ref(), "View");
        assert!(matches!(&menus[4].items[1], OwnedMenuItem::Action { action, .. } if action.as_any().is::<kver::reader::ShowContents>()));
        assert!(matches!(&menus[4].items[2], OwnedMenuItem::Action { action, .. } if action.as_any().is::<kver::reader::ToggleSidebar>()));
        assert!(matches!(&menus[1].items[0], OwnedMenuItem::Action { action, .. } if action.as_any().is::<NewWindow>()));
        assert!(matches!(&menus[3].items[0], OwnedMenuItem::Action { action, .. } if action.as_any().is::<NextWindow>()));
        assert!(matches!(&menus[3].items[1], OwnedMenuItem::Action { action, .. } if action.as_any().is::<PreviousWindow>()));
        let OwnedMenuItem::Action {
            name, action: quit, ..
        } = &menus[0].items[0]
        else {
            panic!("Quit menu item is not an Action")
        };
        assert_eq!(name, "Quit Kver");
        assert!(quit.as_any().is::<Quit>());
        let OwnedMenuItem::Action {
            name, action: open, ..
        } = &menus[1].items[1]
        else {
            panic!("Open PDF menu item is not an Action")
        };
        assert_eq!(name, "Open PDF…");
        assert!(open.as_any().is::<OpenPdf>());
        quit.boxed_clone()
    });
    let first = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
    })?;
    let second = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(None, window.scale_factor(), cx))
    })?;
    cx.capture_screenshot(first.into())?;
    cx.capture_screenshot(second.into())?;
    cx.update_window(first.into(), |_, window, _| window.activate_window())?;
    assert_eq!(cx.update(|app| app.active_window()), Some(first.into()));
    cx.update_window(second.into(), |_, window, _| window.activate_window())?;
    assert_eq!(cx.update(|app| app.active_window()), Some(second.into()));
    assert!(cx.update_window(first.into(), |_, window, _| {
        window
            .highest_precedence_binding_for_action(&OpenPdf)
            .is_some()
    })?);
    assert!(cx.update_window(second.into(), |_, window, _| {
        window
            .highest_precedence_binding_for_action(&Quit)
            .is_some()
    })?);
    cx.update(|app| app.dispatch_action(quit.as_ref()));
    cx.run_until_parked();
    Ok(())
}

fn select_and_copy_ordinary_text() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some(PathBuf::from("examples/readme-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 100 · 100%")?;
    cx.capture_screenshot(window.into())?;
    cx.update(|app| app.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into())));
    press_key(&mut cx, window, "cmd-c")?;
    assert_eq!(
        cx.update(|app| app.read_from_clipboard().unwrap().text()),
        Some("unchanged".into())
    );
    press_key(&mut cx, window, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, window, "copy")?;
    press_key(&mut cx, window, "enter")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| app.read_from_clipboard().unwrap().text()),
        Some("unchanged".into()),
        "disabled Copy in the palette must leave the clipboard alone"
    );
    if cx.update(|app| window.read(app).unwrap().palette_open()) {
        press_key(&mut cx, window, "escape")?;
        cx.run_until_parked();
    }

    for (zoom, scroll) in [(1.0, 0.0), (1.25, 0.0), (1.25, 40.0)] {
        if zoom > cx.update(|app| window.read(app).unwrap().scale()) {
            window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
            wait_for_page(&mut cx, &window, "Page 1 of 100 · 125%")?;
        }
        if scroll > 0.0 {
            window.update(&mut cx, |reader, _, cx| reader.scroll_by(scroll, cx))?;
        }
        click(&mut cx, window.into(), point(px(160.0), px(200.0)))?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            None
        );
        let before = cx.capture_screenshot(window.into())?;
        let (start, end) = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (
                reader.text_screen_point(0, 0, 0.1),
                reader.text_screen_point(0, 4, 0.8),
            )
        });
        let (start, end) = (
            start.ok_or("text was not extracted")?,
            end.ok_or("text end missing")?,
        );
        cx.update_window(window.into(), |_, view, app| {
            view.dispatch_event(
                MouseDownEvent {
                    position: start,
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                app,
            );
            view.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                app,
            );
            view.dispatch_event(
                MouseUpEvent {
                    position: end,
                    button: MouseButton::Left,
                    click_count: 1,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                app,
            );
        })?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            Some("KVER ".into())
        );
        let after = cx.capture_screenshot(window.into())?;
        let density = after.width() as f32 / 920.0;
        let y = (f32::from(start.y) * density) as u32;
        let left = ((f32::from(start.x) + 2.0) * density) as u32;
        let right = ((f32::from(end.x) - 2.0) * density) as u32;
        assert!(right > left);
        for x in left..right {
            assert_ne!(
                before.get_pixel(x, y),
                after.get_pixel(x, y),
                "selection highlight has a gap at x={x}"
            );
        }
        press_key(&mut cx, window, "cmd-c")?;
        assert_eq!(
            cx.update(|app| app.read_from_clipboard().unwrap().text()),
            Some("KVER ".into())
        );
        if zoom == 1.0 {
            cx.update(|app| {
                app.write_to_clipboard(gpui::ClipboardItem::new_string("reset".into()))
            });
            press_key(&mut cx, window, "cmd-shift-p")?;
            cx.run_until_parked();
            type_query(&mut cx, window, "copy")?;
            press_key(&mut cx, window, "enter")?;
            cx.run_until_parked();
            assert_eq!(
                cx.update(|app| app.read_from_clipboard().unwrap().text()),
                Some("KVER ".into()),
                "palette Copy must dispatch the same reader action"
            );
        }
    }
    std::fs::create_dir_all("target/captures")?;
    let blank = PathBuf::from("target/captures/blank-selection.pdf");
    std::fs::write(
        &blank,
        pdf_with_page_sizes([(384, 504), (378, 288), (352, 294)]),
    )?;
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(blank, cx);
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().selected_text()),
        None
    );
    wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
    // An obsolete extraction completing after a second open must not restore
    // either old selection or old text on the new blank document.
    window.update(&mut cx, |reader, _, cx| {
        reader.open_path(PathBuf::from("examples/readme-sample.pdf"), cx);
        reader.open_path(PathBuf::from("target/captures/blank-selection.pdf"), cx);
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
    cx.capture_screenshot(window.into())?;
    cx.update_window(window.into(), |_, view, app| {
        view.dispatch_event(
            MouseDownEvent {
                position: point(px(370.0), px(200.0)),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
        view.dispatch_event(
            MouseMoveEvent {
                position: point(px(450.0), px(200.0)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().selected_text()),
        None
    );
    press_key(&mut cx, window, "cmd-c")?;
    assert_eq!(
        cx.update(|app| app.read_from_clipboard().unwrap().text()),
        Some("KVER ".into())
    );
    Ok(())
}

fn multi_line_selection_survives_zoom_and_scroll() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(920.0), px(720.0)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some(PathBuf::from("examples/selection-sample.pdf")),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 1 · 100%")?;
    for (zoom, scroll) in [(1.0, 0.0), (1.25, 0.0), (1.25, 55.0)] {
        if zoom > cx.update(|app| window.read(app).unwrap().scale()) {
            window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
            wait_for_page(&mut cx, &window, "Page 1 of 1 · 125%")?;
        }
        if scroll > 0.0 {
            window.update(&mut cx, |reader, _, cx| reader.scroll_by(scroll, cx))?;
        }
        if zoom > 1.0 {
            assert_eq!(
                cx.update(|app| window.read(app).unwrap().selected_text()),
                Some("café world\nSecond naïve".into()),
                "zooming or scrolling must not change the document-space selection"
            );
        }
        click(&mut cx, window.into(), point(px(160.0), px(300.0)))?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            None
        );
        let before = cx.capture_screenshot(window.into())?;
        let (start, end, first_line, second_line, third_line) = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (
                reader.text_screen_point(0, 6, 0.1),
                reader.text_screen_point(0, 27, 0.9),
                reader.text_screen_point(0, 8, 0.5),
                reader.text_screen_point(0, 20, 0.5),
                reader.text_screen_point(0, 35, 0.5),
            )
        });
        let (start, end) = (start.ok_or("start missing")?, end.ok_or("end missing")?);
        cx.update_window(window.into(), |_, view, app| {
            for input in [
                MouseDownEvent {
                    position: start,
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                MouseUpEvent {
                    position: end,
                    button: MouseButton::Left,
                    click_count: 1,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
            ] {
                view.dispatch_event(input, app);
            }
        })?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            Some("café world\nSecond naïve".into())
        );
        let after = cx.capture_screenshot(window.into())?;
        let density = after.width() as f32 / 920.0;
        for (point, highlighted) in [
            (first_line.unwrap(), true),
            (second_line.unwrap(), true),
            (third_line.unwrap(), false),
        ] {
            let x = (f32::from(point.x) * density) as u32;
            let y = (f32::from(point.y) * density) as u32;
            assert_eq!(
                before.get_pixel(x, y) != after.get_pixel(x, y),
                highlighted,
                "overlay at {point:?} (zoom {zoom}, scroll {scroll})"
            );
        }
        press_key(&mut cx, window, "cmd-c")?;
        assert_eq!(
            cx.update(|app| app.read_from_clipboard().unwrap().text()),
            Some("café world\nSecond naïve".into())
        );
    }
    // Reverse drag through a paragraph gap still copies in visible reading order.
    let (start, end) = cx.update(|app| {
        let reader = window.read(app).unwrap();
        (
            reader.text_screen_point(0, 39, 0.9).unwrap(),
            reader.text_screen_point(0, 9, 0.9).unwrap(),
        )
    });
    cx.capture_screenshot(window.into())?;
    cx.update_window(window.into(), |_, view, app| {
        view.dispatch_event(
            MouseDownEvent {
                position: start,
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
        view.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
        view.dispatch_event(
            MouseUpEvent {
                position: end,
                button: MouseButton::Left,
                click_count: 1,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            app,
        );
    })?;
    press_key(&mut cx, window, "cmd-c")?;
    assert_eq!(
        cx.update(|app| app.read_from_clipboard().unwrap().text()),
        Some(" world\nSecond naïve line\n\nThird Þ".into())
    );
    Ok(())
}

fn rotated_crop_and_scan_selection() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/captures")?;
    let path = PathBuf::from("target/captures/rotated-selection.pdf");
    std::fs::write(&path, rotated_selection_pdf())?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(420.0), px(220.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 1 · 100%")?;
    for zoom in [1.0, 1.25] {
        if zoom > 1.0 {
            window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
            wait_for_page(&mut cx, &window, "Page 1 of 1 · 125%")?;
            cx.capture_screenshot(window.into())?;
            let before_scroll = cx
                .update(|app| window.read(app).unwrap().text_screen_point(0, 0, 0.1))
                .ok_or("text before scroll missing")?;
            window.update(&mut cx, |reader, _, cx| reader.scroll_by(25.0, cx))?;
            let after_scroll = cx
                .update(|app| window.read(app).unwrap().text_screen_point(0, 0, 0.1))
                .ok_or("text after scroll missing")?;
            assert!(
                f32::from(after_scroll.y) < f32::from(before_scroll.y) - 5.0,
                "fixture must really scroll"
            );
        }
        click(&mut cx, window.into(), point(px(200.0), px(120.0)))?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            None
        );
        let before = cx.capture_screenshot(window.into())?;
        let (start, end) = cx.update(|app| {
            let reader = window.read(app).unwrap();
            (
                reader.text_screen_point(0, 0, 0.1),
                reader.text_screen_point(0, 6, 0.9),
            )
        });
        let (start, end) = (
            start.ok_or("rotated text unavailable")?,
            end.ok_or("rotated end unavailable")?,
        );
        assert!(f32::from(end.y) > f32::from(start.y));
        cx.update_window(window.into(), |_, view, app| {
            for input in [
                MouseDownEvent {
                    position: start,
                    button: MouseButton::Left,
                    click_count: 1,
                    first_mouse: false,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                MouseUpEvent {
                    position: end,
                    button: MouseButton::Left,
                    click_count: 1,
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
            ] {
                view.dispatch_event(input, app);
            }
        })?;
        assert_eq!(
            cx.update(|app| window.read(app).unwrap().selected_text()),
            Some("VISIBLE".into())
        );
        let after = cx.capture_screenshot(window.into())?;
        let density = after.width() as f32 / 420.0;
        let x = (f32::from(start.x) * density) as u32;
        let y = ((f32::from(start.y) + f32::from(end.y)) * 0.5 * density) as u32;
        assert_ne!(
            before.get_pixel(x, y),
            after.get_pixel(x, y),
            "rotated selection overlay missing"
        );
        press_key(&mut cx, window, "cmd-c")?;
        assert_eq!(
            cx.update(|app| app.read_from_clipboard().unwrap().text()),
            Some("VISIBLE".into())
        );
    }
    let scan = PathBuf::from("target/captures/image-only-selection.pdf");
    std::fs::write(&scan, image_only_pdf())?;
    assert!(kver::pdf::page_text(&scan, 0)?.glyphs.is_empty());
    window.update(&mut cx, |reader, _, cx| reader.open_path(scan, cx))?;
    wait_for_page(&mut cx, &window, "Page 1 of 1 · 100%")?;
    cx.capture_screenshot(window.into())?;
    assert!(
        cx.update(|app| window.read(app).unwrap().text_screen_point(0, 0, 0.5))
            .is_none()
    );
    cx.update_window(window.into(), |_, view, app| {
        for input in [
            MouseDownEvent {
                position: point(px(200.0), px(135.0)),
                button: MouseButton::Left,
                click_count: 1,
                first_mouse: false,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            MouseMoveEvent {
                position: point(px(240.0), px(155.0)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            MouseUpEvent {
                position: point(px(240.0), px(155.0)),
                button: MouseButton::Left,
                click_count: 1,
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
        ] {
            view.dispatch_event(input, app);
        }
    })?;
    assert_eq!(
        cx.update(|app| window.read(app).unwrap().selected_text()),
        None
    );
    press_key(&mut cx, window, "cmd-c")?;
    assert_eq!(
        cx.update(|app| app.read_from_clipboard().unwrap().text()),
        Some("VISIBLE".into())
    );
    Ok(())
}

fn independent_reader_windows() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
        app_menu::install(app);
    });
    let first = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| {
            Preview::new(
                Some("examples/readme-sample.pdf".into()),
                window.scale_factor(),
                cx,
            )
        })
    })?;
    wait_for_page(&mut cx, &first, "Page 1 of 100 · 100%")?;
    press_key(&mut cx, first, "right")?;
    press_key(&mut cx, first, "=")?;
    let first_state = cx.update(|app| {
        let reader = first.read(app).unwrap();
        (
            reader.path().unwrap().to_owned(),
            reader.page_index(),
            reader.scale(),
        )
    });
    // No next window: cycling cannot affect the current document.
    press_key(&mut cx, first, "cmd-`")?;
    press_key(&mut cx, first, "cmd-shift-`")?;
    assert_eq!(cx.update(|app| app.windows().len()), 1);
    press_key(&mut cx, first, "cmd-n")?;
    cx.run_until_parked();
    let second_window: WindowHandle<gpui_kit::base::Root> = cx
        .update(|app| app.windows())
        .into_iter()
        .find(|handle| *handle != first.into())
        .unwrap()
        .downcast()
        .expect("new window has a Kit Root");
    let root = second_window.root(&mut cx)?;
    let second = cx.update(|app| {
        root.read(app)
            .view()
            .clone()
            .downcast::<Preview>()
            .expect("reader view")
    });
    cx.capture_screenshot(second_window.into())?;
    assert_eq!(cx.update(|app| second.read(app).page_count()), None);
    cx.update_window(second_window.into(), |_, frame, app| {
        frame.render_frame(app);
        assert!(frame.find("title-bar").visible());
        assert!(frame.find("open-pdf").visible());
    })?;
    std::fs::create_dir_all("target/captures")?;
    let other_pdf = PathBuf::from("target/captures/kver-windows.pdf");
    std::fs::write(&other_pdf, mixed_page_pdf())?;
    cx.update(|app| second.update(app, |reader, app| reader.open_path(other_pdf.clone(), app)));
    wait_for_view(&mut cx, &second, "Page 1 of 3 · 100%")?;
    press_key(&mut cx, second_window, "right")?;
    press_key(&mut cx, second_window, "cmd-alt-=")?;
    assert_eq!(cx.update(|app| second.read(app).page_index()), 1);
    assert_eq!(cx.update(|app| second.read(app).scale()), 1.25);
    assert_eq!(
        cx.update(|app| {
            let reader = first.read(app).unwrap();
            (
                reader.path().unwrap().to_owned(),
                reader.page_index(),
                reader.scale(),
            )
        }),
        first_state
    );

    // The native menu and shortcut carry the same Action; dispatch with the
    // second window active, and verify focus changes rather than document state.
    let menu_next = cx.update(|app| {
        let menus = app.get_menus().unwrap();
        let OwnedMenuItem::Action { action, .. } = &menus[3].items[0] else {
            panic!("missing Next Window")
        };
        action.boxed_clone()
    });
    cx.update_window(second_window.into(), |_, window, _| {
        window.activate_window()
    })?;
    cx.update(|app| app.dispatch_action(menu_next.as_ref()));
    assert_eq!(cx.update(|app| app.active_window()), Some(first.into()));
    press_key(&mut cx, first, "cmd-`")?;
    assert_eq!(
        cx.update(|app| app.active_window()),
        Some(second_window.into())
    );
    press_key(&mut cx, second_window, "cmd-shift-`")?;
    assert_eq!(cx.update(|app| app.active_window()), Some(first.into()));
    let menu_previous = cx.update(|app| {
        let menus = app.get_menus().unwrap();
        let OwnedMenuItem::Action { action, .. } = &menus[3].items[1] else {
            panic!("missing Previous Window")
        };
        action.boxed_clone()
    });
    cx.update(|app| app.dispatch_action(menu_previous.as_ref()));
    assert_eq!(
        cx.update(|app| app.active_window()),
        Some(second_window.into())
    );
    press_key(&mut cx, second_window, "cmd-shift-p")?;
    cx.run_until_parked();
    press_key(&mut cx, second_window, "tab")?;
    cx.run_until_parked();
    type_query(&mut cx, second_window, "next window")?;
    assert_eq!(
        cx.update(|app| second.read(app).palette_matches(app)),
        Some(1)
    );
    assert!(cx.update_window(second_window.into(), |_, window, _| {
        window
            .highest_precedence_binding_for_action(&NextWindow)
            .is_some()
    })?);
    press_key(&mut cx, second_window, "enter")?;
    cx.run_until_parked();
    assert_eq!(cx.update(|app| app.active_window()), Some(first.into()));
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().page_index()),
        first_state.1
    );
    assert_eq!(cx.update(|app| second.read(app).page_index()), 1);
    press_key(&mut cx, first, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, first, "previous window")?;
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    assert!(cx.update_window(first.into(), |_, window, _| {
        window
            .highest_precedence_binding_for_action(&PreviousWindow)
            .is_some()
    })?);
    press_key(&mut cx, first, "enter")?;
    cx.run_until_parked();
    assert_eq!(
        cx.update(|app| app.active_window()),
        Some(second_window.into())
    );
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().page_index()),
        first_state.1
    );

    cx.update_window(second_window.into(), |_, window, _| window.remove_window())?;
    cx.run_until_parked();
    press_key(&mut cx, first, "right")?;
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().page_index()),
        first_state.1 + 1
    );
    // Palette and native menu New Window also create empty, independent readers.
    press_key(&mut cx, first, "cmd-shift-p")?;
    cx.run_until_parked();
    type_query(&mut cx, first, "new window")?;
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().palette_matches(app)),
        Some(1)
    );
    press_key(&mut cx, first, "enter")?;
    cx.run_until_parked();
    let palette_window = cx
        .update(|app| app.windows())
        .into_iter()
        .find(|handle| *handle != first.into())
        .unwrap();
    assert_eq!(
        cx.update(|app| palette_window
            .downcast::<gpui_kit::base::Root>()
            .unwrap()
            .read(app)
            .unwrap()
            .view()
            .clone()
            .downcast::<Preview>()
            .unwrap()
            .read(app)
            .path()
            .map(PathBuf::from)),
        None
    );
    let menu_new = cx.update(|app| {
        let menus = app.get_menus().unwrap();
        let OwnedMenuItem::Action { action, .. } = &menus[1].items[0] else {
            panic!("missing New Window")
        };
        action.boxed_clone()
    });
    cx.update(|app| app.dispatch_action(menu_new.as_ref()));
    cx.run_until_parked();
    assert_eq!(cx.update(|app| app.windows().len()), 3);
    let third = cx.update(|app| app.active_window().unwrap());
    assert_ne!(third, palette_window);
    cx.update_window(first.into(), |_, window, _| window.activate_window())?;
    press_key(&mut cx, first, "cmd-shift-`")?;
    assert_eq!(cx.update(|app| app.active_window()), Some(third));
    cx.update_window(first.into(), |_, window, _| window.activate_window())?;
    press_key(&mut cx, first, "cmd-`")?;
    assert_eq!(cx.update(|app| app.active_window()), Some(palette_window));
    assert_eq!(
        cx.update(|app| first.read(app).unwrap().page_index()),
        first_state.1 + 1
    );
    std::fs::remove_file(other_pdf)?;
    Ok(())
}

fn contents_navigation() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("target/captures")?;
    let path = PathBuf::from("target/captures/contents.pdf");
    std::fs::write(&path, contents_pdf())?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|app| {
        gpui_kit::init(app);
        bind_reader_keys(app);
    });
    let window = cx.open_window(size(px(700.), px(650.)), |window, cx| {
        cx.new(|cx| Preview::new(Some(path), window.scale_factor(), cx))
    })?;
    wait_for_page(&mut cx, &window, "Page 1 of 3 · 100%")?;
    let reader = window.root(&mut cx)?;
    window.update(&mut cx, |_, window, app| {
        window.dispatch_action(Box::new(kver::reader::ShowContents), app);
    })?;
    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let ready = cx.update_window(window.into(), |_, window, app| {
            window.render_frame(app);
            window.try_find(("contents-entry", 1usize)).is_some()
        })?;
        if ready {
            break;
        }
        if Instant::now() > deadline {
            return Err("outline did not load".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    cx.forbid_parking();
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        window.click(("contents-entry", 2usize), app);
    })?;
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 1);
    assert!((cx.update(|app| reader.read(app).reading_position().1) - 70.).abs() < 1.);
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        window.click(("contents-entry", 3usize), app);
    })?;
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 1);
    press_key(&mut cx, window, "up")?;
    press_key(&mut cx, window, "enter")?;
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 2);
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        window.click(("contents-entry", 0usize), app);
    })?;
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 0);
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find(("contents-entry", 1usize)).visible());
    })?;
    window.update(&mut cx, |reader, _, cx| reader.select_page(2, cx))?;
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        window.click(("contents-disclosure", 0usize), app);
    })?;
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 2);
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.try_find(("contents-entry", 1usize)).is_none());
    })?;
    press_key(&mut cx, window, "right")?;
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find(("contents-entry", 1usize)).visible());
    })?;
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.find("reader-sidebar").visible());
        assert!(window.find("page").visible());
    })?;
    let empty = PathBuf::from("target/captures/contents-absent.pdf");
    std::fs::write(&empty, pdf_with_page_sizes([(384, 504); 3]))?;
    window.update(&mut cx, |reader, _, cx| reader.open_path(empty, cx))?;
    cx.allow_parking();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let empty = cx.update_window(window.into(), |_, window, app| {
            window.render_frame(app);
            window.try_find("contents-empty").is_some()
        })?;
        if empty {
            break;
        }
        if Instant::now() > deadline {
            return Err("old outline survived document replacement".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    cx.forbid_parking();
    assert_eq!(cx.update(|app| reader.read(app).page_index()), 0);
    cx.update_window(window.into(), |_, window, app| {
        window.render_frame(app);
        assert!(window.try_find(("contents-entry", 2usize)).is_none());
        assert!(window.find("contents-panel").visible());
    })?;
    std::fs::remove_file("target/captures/contents.pdf")?;
    std::fs::remove_file("target/captures/contents-absent.pdf")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("KVER_BENCH").is_some() {
        return measure_page_turns();
    }
    sidebar_host()?;
    contents_navigation()?;
    independent_reader_windows()?;
    opening_a_pdf_and_zooming_changes_the_rendered_page()?;
    fit_modes_follow_page_and_window()?;
    fit_page_transition_is_stable()?;
    short_final_page_fits_without_feedback()?;
    loading_errors_are_visible()?;
    opening_from_empty_window_and_replacing_documents()?;
    open_pdf_palette_in_empty_window()?;
    shared_reader_actions()?;
    select_and_copy_ordinary_text()?;
    multi_line_selection_survives_zoom_and_scroll()?;
    rotated_crop_and_scan_selection()?;
    menu_actions_and_shortcuts()?;
    long_path_keeps_toolbar_clickable()
}
