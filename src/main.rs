use std::path::PathBuf;

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};
use kver::reader::Preview;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        eprintln!("Usage: kver path/to/document.pdf");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("Usage: kver path/to/document.pdf");
        std::process::exit(2);
    }
    let path = PathBuf::from(first);

    gpui_platform::application().run(move |cx: &mut App| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(920.0), px(800.0)), cx);
        if let Err(error) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Preview::new(path, window.scale_factor(), cx)),
        ) {
            eprintln!("Cannot open window: {error}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
