use std::{path::PathBuf, sync::Arc};

use gpui::{
    App, Bounds, Context, Image, ImageFormat, Window, WindowBounds, WindowOptions, div, img,
    prelude::*, px, rgb, size,
};
use kver::pdf;

struct Preview {
    path: PathBuf,
    scale: f32,
    display_scale: f32,
    request: u64,
    status: String,
    // Cached image and unscaled page dimensions, in PDF points.
    page: Option<(Arc<Image>, f32, f32)>,
}

fn raster_scale(zoom: f32, display_scale: f32, page_size: Option<(f32, f32)>) -> f32 {
    let requested = (zoom * display_scale).clamp(0.25, 8.0);
    let Some((width, height)) = page_size else {
        return requested;
    };
    // Match the PDF adapter's bitmap bounds. An old image remains visible
    // at logical zoom while a density-limited replacement is rendered.
    let max_scale = (32_000_000.0 / (width * height))
        .sqrt()
        .min(8192.0 / width)
        .min(8192.0 / height);
    requested.min(max_scale)
}

impl Preview {
    fn new(path: PathBuf, display_scale: f32, cx: &mut Context<Self>) -> Self {
        let mut preview = Self {
            path,
            scale: 1.0,
            display_scale,
            request: 0,
            status: String::new(),
            page: None,
        };
        preview.request_render(cx);
        preview
    }

    #[cfg(feature = "screenshot")]
    fn from_rendered(path: PathBuf, rendered: pdf::RenderedPage) -> Self {
        Self {
            path,
            scale: 1.0,
            display_scale: 1.0,
            request: 0,
            status: "First page · 100%".into(),
            page: Some((
                Arc::new(Image::from_bytes(ImageFormat::Png, rendered.png)),
                rendered.page_width,
                rendered.page_height,
            )),
        }
    }

    fn change_scale(&mut self, factor: f32, cx: &mut Context<Self>) {
        let next = (self.scale * factor).clamp(0.25, 4.0);
        if next != self.scale {
            self.scale = next;
            self.request_render(cx);
        }
    }

    fn request_render(&mut self, cx: &mut Context<Self>) {
        self.request += 1;
        let request = self.request;
        let path = self.path.clone();
        let scale = self.scale;
        // Zoom determines the logical image size; display density determines
        // how many pixels we need to rasterize for that size.
        let raster_scale = raster_scale(
            scale,
            self.display_scale,
            self.page
                .as_ref()
                .map(|(_, width, height)| (*width, *height)),
        );
        self.status = format!("Rendering at {}%…", (scale * 100.0).round());
        cx.notify();

        // Parsing, rasterization, and PNG encoding happen off the UI thread.
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_first_page(&path, raster_scale) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                // A slower earlier zoom must never overwrite the latest image.
                if this.request != request {
                    return;
                }
                match result {
                    Ok(rendered) => {
                        this.page = Some((
                            Arc::new(Image::from_bytes(ImageFormat::Png, rendered.png)),
                            rendered.page_width,
                            rendered.page_height,
                        ));
                        this.status = format!("First page · {}%", (scale * 100.0).round());
                    }
                    Err(error) => {
                        this.status = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A window moved between displays needs a new bitmap, but retains its
        // logical size and keeps showing the old one until rendering completes.
        let display_scale = window.scale_factor();
        if self.display_scale != display_scale {
            self.display_scale = display_scale;
            self.request_render(cx);
        }
        let page = self.page.as_ref().map(|(image, width, height)| {
            let page_width = px(*width * self.scale);
            div()
                .w_full()
                .min_w(page_width)
                .flex()
                .justify_center()
                .child(
                    img(image.clone())
                        .w(page_width)
                        .h(px(*height * self.scale))
                        .flex_shrink_0(),
                )
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x30343b))
            .text_color(rgb(0xf5f5f5))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .p_3()
                    .child(self.path.display().to_string())
                    .child(
                        div()
                            .id("zoom-out")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("−")
                            .on_click(cx.listener(|this, _, _, cx| this.change_scale(0.8, cx))),
                    )
                    .child(
                        div()
                            .id("zoom-in")
                            .cursor_pointer()
                            .px_3()
                            .bg(rgb(0x545b65))
                            .child("+")
                            .on_click(cx.listener(|this, _, _, cx| this.change_scale(1.25, cx))),
                    )
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .id("page")
                    .flex_1()
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .p_4()
                    .when_some(page, |this, page| this.child(page)),
            )
    }
}

#[cfg(feature = "screenshot")]
enum Step {
    Capture(PathBuf),
    ZoomIn,
    ZoomOut,
}

#[cfg(feature = "screenshot")]
fn parse_scenario(text: &str, output_dir: &std::path::Path) -> Result<Vec<Step>, String> {
    use std::path::Component;

    let mut steps = Vec::new();
    for (line, raw) in text.lines().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let tokens: Vec<_> = raw.split_whitespace().collect();
        let step = match tokens.as_slice() {
            ["zoom-in"] => Step::ZoomIn,
            ["zoom-out"] => Step::ZoomOut,
            ["capture", name]
                if std::path::Path::new(name)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                    && std::path::Path::new(name).components().count() == 1
                    && name.ends_with(".png") =>
            {
                Step::Capture(output_dir.join(name))
            }
            _ => return Err(format!("Invalid scenario step on line {}", line + 1)),
        };
        steps.push(step);
    }
    if !steps.iter().any(|step| matches!(step, Step::Capture(_))) {
        return Err("Scenario must contain at least one capture".into());
    }
    Ok(steps)
}

#[cfg(feature = "screenshot")]
fn run_headless(path: PathBuf, steps: Vec<Step>) -> Result<(), Box<dyn std::error::Error>> {
    use gpui::HeadlessAppContext;

    // Render synchronously before opening the test window so the captured frame
    // contains the page rather than the normal asynchronous loading state.
    let rendered = pdf::render_first_page(&path, 1.0)?;
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(
        text_system,
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let window = cx.open_window(size(px(920.0), px(800.0)), |_, cx| {
        cx.new(|_| Preview::from_rendered(path, rendered))
    })?;
    cx.run_until_parked();
    for step in steps {
        match step {
            Step::ZoomIn | Step::ZoomOut => {
                let factor = if matches!(step, Step::ZoomIn) {
                    1.25
                } else {
                    0.8
                };
                // This is the same command the on_click handlers invoke.
                let zoom = window.update(&mut cx, |preview, _, cx| {
                    preview.change_scale(factor, cx);
                    preview.scale
                })?;
                cx.run_until_parked();
                let status =
                    cx.update(|app| window.read(app).map(|preview| preview.status.clone()))?;
                let expected = format!("First page · {}%", (zoom * 100.0).round());
                if status != expected {
                    return Err(format!("Zoom render did not finish: {status}").into());
                }
            }
            Step::Capture(output) => {
                let status =
                    cx.update(|app| window.read(app).map(|preview| preview.status.clone()))?;
                if !status.starts_with("First page") {
                    return Err(format!("Cannot capture while page is not ready: {status}").into());
                }
                cx.capture_screenshot(window.into())?.save(output)?;
            }
        }
    }
    Ok(())
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        eprintln!("Usage: kver path/to/document.pdf");
        #[cfg(feature = "screenshot")]
        eprintln!("       kver --screenshot output.png path/to/document.pdf");
        #[cfg(feature = "screenshot")]
        eprintln!("       kver --automate scenario.txt output-dir path/to/document.pdf");
        std::process::exit(2);
    };
    #[cfg(feature = "screenshot")]
    if first == "--screenshot" || first == "--automate" {
        let result = if first == "--screenshot" {
            match (args.next(), args.next(), args.next()) {
                (Some(output), Some(path), None) => run_headless(
                    PathBuf::from(path),
                    vec![Step::Capture(PathBuf::from(output))],
                ),
                _ => Err("Usage: kver --screenshot output.png path/to/document.pdf".into()),
            }
        } else {
            match (args.next(), args.next(), args.next(), args.next()) {
                (Some(scenario), Some(output_dir), Some(path), None) => {
                    let output_dir = PathBuf::from(output_dir);
                    (|| {
                        let text = std::fs::read_to_string(scenario)?;
                        let steps = parse_scenario(&text, &output_dir)?;
                        std::fs::create_dir_all(output_dir)?;
                        run_headless(PathBuf::from(path), steps)
                    })()
                }
                _ => Err(
                    "Usage: kver --automate scenario.txt output-dir path/to/document.pdf".into(),
                ),
            }
        };
        if let Err(error) = result {
            eprintln!("Cannot run headless: {error}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(not(feature = "screenshot"))]
    if first == "--screenshot" || first == "--automate" {
        eprintln!("Headless capture requires --features screenshot");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_increases_pixels_without_changing_logical_zoom() {
        assert_eq!(raster_scale(1.0, 2.0, Some((612.0, 792.0))), 2.0);
        assert_eq!(raster_scale(1.0, 1.0, Some((612.0, 792.0))), 1.0);
    }

    #[test]
    fn high_zoom_is_bounded_for_a4_scans() {
        let scale = raster_scale(4.0, 2.0, Some((595.44, 842.04)));
        assert!(scale < 8.0);
        assert!(scale > 7.0);
        assert!(595.44 * 842.04 * scale * scale <= 32_000_000.0);
    }

    #[cfg(feature = "screenshot")]
    #[test]
    fn scenario_accepts_commands_and_safe_capture_names() {
        let steps = parse_scenario(
            "# comment\ncapture before.png\nzoom-in\nzoom-out\ncapture after.png\n",
            std::path::Path::new("captures"),
        )
        .unwrap();
        assert_eq!(steps.len(), 4);
        assert!(
            matches!(&steps[0], Step::Capture(path) if path == std::path::Path::new("captures/before.png"))
        );
        assert!(matches!(steps[1], Step::ZoomIn));
        assert!(
            matches!(&steps[3], Step::Capture(path) if path == std::path::Path::new("captures/after.png"))
        );
        assert!(parse_scenario("capture ../escape.png", std::path::Path::new("captures")).is_err());
        assert!(
            parse_scenario("capture /tmp/escape.png", std::path::Path::new("captures")).is_err()
        );
        assert!(parse_scenario("zoom-in", std::path::Path::new("captures")).is_err());
        assert!(
            parse_scenario(
                "click 10 10\ncapture a.png",
                std::path::Path::new("captures")
            )
            .is_err()
        );
    }
}
