use std::path::PathBuf;

use gpui_kit::App;
use gpui_kit::platform as gpui_platform;
use kver::assets::ReaderAssets;
use kver::reader::bind_reader_keys;
use kver::reader_windows;

fn local_path(url: &str) -> Option<PathBuf> {
    url::Url::parse(url).ok()?.to_file_path().ok()
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if args.next().is_some() {
        eprintln!("Usage: kver path/to/document.pdf");
        std::process::exit(2);
    }
    let path = first.map(PathBuf::from);

    // Register before launching so Finder's initial open event is not lost.
    let (requests, incoming) = async_channel::unbounded();
    let application = gpui_platform::application().with_assets(ReaderAssets);
    application.on_open_urls(move |urls| {
        for url in urls {
            if let Some(path) = local_path(&url) {
                let _ = requests.try_send(path);
            }
        }
    });
    application.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        gpui_kit::component::theme::Theme::sync_system_appearance(None, cx);
        bind_reader_keys(cx);
        reader_windows::install(cx);
        kver::settings_ui::install(cx);
        kver::recent::install(cx, kver::settings::AppPaths::resolve());
        #[cfg(target_os = "macos")]
        kver::app_menu::install(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let initial = match reader_windows::open(cx, path) {
            Ok(reader) => reader.downgrade(),
            Err(error) => {
                eprintln!("Cannot open window: {error}");
                cx.quit();
                return;
            }
        };
        cx.spawn(async move |cx| {
            while let Ok(path) = incoming.recv().await {
                cx.update(|cx| {
                    // Reuse the launch window only while it is still empty.
                    if let Some(reader) = initial.upgrade()
                        && reader.read(cx).path().is_none()
                    {
                        reader.update(cx, |reader, cx| reader.open_path(path, cx));
                    } else if let Err(error) = reader_windows::open(cx, Some(path)) {
                        eprintln!("Cannot open window: {error}");
                    }
                    cx.activate(true);
                });
            }
        })
        .detach();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_file_urls_preserve_special_characters() {
        let path = std::env::temp_dir().join("PDF space # % æ.pdf");
        let url = url::Url::from_file_path(&path).unwrap();
        assert_eq!(local_path(url.as_str()), Some(path));
    }

    #[test]
    fn ignore_non_file_and_invalid_urls() {
        assert_eq!(local_path("https://example.com/book.pdf"), None);
        assert_eq!(local_path("not a URL"), None);
    }
}
