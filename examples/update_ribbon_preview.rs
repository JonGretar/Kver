//! Interactive preview, isolated from the real app instance and update engines.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn main() {
    use gpui_kit::platform;
    use kver::{app, assets::ReaderAssets};
    let managed = !std::env::args().any(|arg| arg == "--source");
    let temp = tempfile::tempdir().expect("preview directory");
    let pdf = temp.path().join("Linux update preview.pdf");
    std::fs::write(&pdf, include_bytes!("../examples/readme-sample.pdf")).expect("copy sample PDF");
    platform::application()
        .with_assets(ReaderAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            app::keymap::install_with_paths(cx, Default::default(), None);
            kver::settings_ui::install_with_paths(cx, None);
            app::windows::open(cx, Some(pdf)).expect("preview window");
            app::preview_ribbon(managed, cx);
            cx.on_window_closed(move |cx, _| {
                let _keep_alive = &temp;
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn main() {
    eprintln!("Use macOS or Linux for the Linux update ribbon preview.");
}
