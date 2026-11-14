use super::*;
use gpui_kit::component::theme::{Theme, ThemeMode};

fn render_window(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

fn toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(TogglePdfDarkMode), cx);
    })
    .unwrap();
    cx.run_until_parked();
    render_window(cx, window);
}

#[gpui_kit::test]
fn pdf_dark_mode_override_is_local_unsaved_and_cleared_on_open(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    let file = root.path().join("scan.pdf");
    let other_file = root.path().join("other.pdf");
    let original = image_only_pdf();
    std::fs::write(&file, &original).unwrap();
    std::fs::write(&other_file, &original).unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::Appearance(crate::settings::Appearance::Light),
    )
    .unwrap();
    let config = std::fs::read(paths.config()).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(file.clone()));
    // Even another window showing the same PDF has an independent override.
    let (other_window, second) = recent_test_window(cx, Some(file.clone()));
    let (different_window, different) = recent_test_window(cx, Some(other_file.clone()));
    cx.run_until_parked();
    render_window(cx, window);
    let (old_image, position) = first.read_with(cx, |reader, _| {
        assert!(reader.page_colors.is_none());
        (
            reader.rasters.image(0).unwrap().clone(),
            reader.reading_position(),
        )
    });
    toggle(cx, window);
    first.read_with(cx, |reader, cx| {
        assert!(!cx.theme().mode.is_dark());
        let paper = reader.page_colors.unwrap().paper();
        assert!(paper.iter().all(|channel| *channel < 0.5));
        assert_eq!(reader.pdf_dark_mode_override, Some(true));
        assert!(!Arc::ptr_eq(&old_image, reader.rasters.image(0).unwrap()));
        assert_eq!(reader.reading_position(), position);
    });
    for (window, reader) in [(other_window, &second), (different_window, &different)] {
        render_window(cx, window);
        reader.read_with(cx, |reader, _| {
            assert!(reader.page_colors.is_none());
            assert_eq!(reader.pdf_dark_mode_override, None);
        });
    }
    assert_eq!(std::fs::read(paths.config()).unwrap(), config);
    toggle(cx, window);
    first.read_with(cx, |reader, _| {
        assert!(reader.page_colors.is_none());
        assert_eq!(reader.pdf_dark_mode_override, Some(false));
    });
    toggle(cx, window);
    first.update(cx, |reader, cx| reader.open_path(other_file, cx));
    cx.run_until_parked();
    render_window(cx, window);
    first.read_with(cx, |reader, _| {
        assert!(reader.page_colors.is_none());
        assert_eq!(reader.pdf_dark_mode_override, None);
    });
    first.update(cx, |reader, cx| reader.open_path(file.clone(), cx));
    cx.run_until_parked();
    render_window(cx, window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
    assert_eq!(std::fs::read(paths.config()).unwrap(), config);
    assert_eq!(std::fs::read(file).unwrap(), original);
}

#[gpui_kit::test]
fn pdf_appearance_preference_updates_followers_but_not_session_overrides(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::settings::AppPaths::for_locations(
        root.path().join("config.toml"),
        root.path().join("local"),
    );
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::Appearance(crate::settings::Appearance::Dark),
    )
    .unwrap();
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::PdfDarkMode(true),
    )
    .unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_reader_keys(cx);
        settings_ui::install_with_paths(cx, Some(paths.clone()));
    });
    let (window, first) = recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    let (other_window, second) =
        recent_test_window(cx, Some(PathBuf::from("examples/field-guide.pdf")));
    cx.run_until_parked();
    render_window(cx, window);
    render_window(cx, other_window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_some()));
    toggle(cx, window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
    second.read_with(cx, |reader, _| assert!(reader.page_colors.is_some()));
    cx.update(|cx| Theme::change(ThemeMode::Light, None, cx));
    render_window(cx, window);
    render_window(cx, other_window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
    second.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
    cx.update(|cx| Theme::change(ThemeMode::Dark, None, cx));
    render_window(cx, window);
    render_window(cx, other_window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
    second.read_with(cx, |reader, _| assert!(reader.page_colors.is_some()));
    toggle(cx, window);
    crate::settings::save(
        paths.config(),
        crate::settings::SettingsChange::PdfDarkMode(false),
    )
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    render_window(cx, window);
    render_window(cx, other_window);
    first.read_with(cx, |reader, _| assert!(reader.page_colors.is_some()));
    second.read_with(cx, |reader, _| assert!(reader.page_colors.is_none()));
}

#[gpui_kit::test]
fn color_change_retires_in_flight_page_pixels(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        settings_ui::install_with_paths(cx, None);
    });
    let (window, reader) = recent_test_window(cx, None);
    let request = reader.update(cx, |reader, _| {
        reader.pages = vec![(100., 100.)];
        let layout = PageLayout::new(&reader.pages, 1., ReadingMode::Continuous, 0);
        let Schedule::Render(request) = reader.rasters.schedule(&layout, 0., 100., 1.) else {
            panic!("expected a raster request");
        };
        reader.page_colors = Some(pdf::PageColors::new([0.1; 3], [0.9; 3]));
        request
    });
    render_window(cx, window);
    reader.update(cx, |reader, _| {
        assert!(reader.page_colors.is_none());
        assert!(matches!(
            reader
                .rasters
                .finish(request, Err("obsolete pixels".into()), 0..1),
            Completion::Obsolete
        ));
        assert!(reader.rasters.image(0).is_none());
    });
}
