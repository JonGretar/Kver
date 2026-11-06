//! Opt-in main-thread capture tool, not a regression gate.
//!
//! `corgi test --features screenshot --test visual` on macOS.
//! Override `KVER_TEST_PDF` and `KVER_CAPTURE_DIR` to inspect another document.
//! Add temporary interactions and captures here while developing a feature,
//! then restore this baseline. Put lasting behavior checks in ordinary tests.
//! Metal requires this process's main thread, so this target has no test harness.

use gpui_kit::platform;
use gpui_kit::{AppContext as _, HeadlessAppContext, px, size};
use kver::assets::ReaderAssets;
use kver::reader::Preview;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf = std::env::var_os("KVER_TEST_PDF")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/readme-sample.pdf"));
    let captures = std::env::var_os("KVER_CAPTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/captures"));

    let mut cx = HeadlessAppContext::with_platform(
        platform::current_platform(true).text_system(),
        Arc::new(ReaderAssets),
        platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    let window = cx.open_window(size(px(920.0), px(800.0)), |window, cx| {
        cx.new(|cx| Preview::new(Some(pdf), window.scale_factor(), cx))
    })?;

    let deadline = Instant::now() + Duration::from_secs(30);
    cx.allow_parking();
    loop {
        cx.run_until_parked();
        let (status, displayed) = cx.update(|app| {
            let reader = window.read(app).expect("capture window is open");
            (reader.status().to_owned(), reader.displayed_page_index())
        });
        if status.starts_with("Page 1 of ") && displayed == Some(0) {
            break;
        }
        if !matches!(status.as_str(), "Loading document…") && !status.starts_with("Rendering page ")
            || Instant::now() >= deadline
        {
            return Err(format!("PDF was not ready for capture: {status}").into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    cx.forbid_parking();

    // Interact with `window` or its `Preview` here before taking more captures.
    std::fs::create_dir_all(&captures)?;
    let image = captures.join("reader.png");
    cx.capture_screenshot(window.into())?.save(&image)?;
    println!("Captured {}", image.display());
    Ok(())
}
