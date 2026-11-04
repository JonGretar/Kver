use std::path::PathBuf;

use gpui_kit::App;
use gpui_kit::platform as gpui_platform;
use kver::assets::ReaderAssets;
use kver::reader::bind_reader_keys;
use kver::reader_windows;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if args.next().is_some() {
        eprintln!("Usage: kver path/to/document.pdf");
        std::process::exit(2);
    }
    let path = first.map(PathBuf::from);

    gpui_platform::application()
        .with_assets(ReaderAssets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
            bind_reader_keys(cx);
            #[cfg(target_os = "macos")]
            kver::app_menu::install(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            if let Err(error) = reader_windows::open(cx, path) {
                eprintln!("Cannot open window: {error}");
                cx.quit();
                return;
            }
            cx.activate(true);
        });
}
