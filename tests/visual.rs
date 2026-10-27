//! Main-thread Cargo test: drive the actual reader, assert state, and inspect PNGs.
//!
//! Add exploratory checks as functions and call them from `main`, then remove
//! them after inspection unless deliberately promoted to CI. Rust's default
//! #[test] runner uses worker threads; Metal requires the process main thread.

use gpui::{
    AnyWindowHandle, AppContext as _, HeadlessAppContext, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, Pixels, Point, WindowHandle, px, size,
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
        if !status.starts_with("Rendering at ") || Instant::now() >= deadline {
            return Err(format!("Expected {expected:?}, got {status:?}").into());
        }
        std::thread::sleep(Duration::from_millis(10));
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

    wait_for_page(&mut cx, &window, "First page · 100%")?;
    let before = cx.capture_screenshot(window.into())?;
    let output = std::env::var_os("KVER_CAPTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/captures"));
    std::fs::create_dir_all(&output)?;
    before.save(output.join("before.png"))?;

    window.update(&mut cx, |reader, _, cx| reader.zoom_in(cx))?;
    wait_for_page(&mut cx, &window, "First page · 125%")?;
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
    println!("Reader zoom passed; captures in {}", output.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    opening_a_pdf_and_zooming_changes_the_rendered_page()
}
