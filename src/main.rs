use std::{path::PathBuf, sync::Arc};

use gpui::{
    App, Application, Bounds, Context, Image, ImageFormat, Window, WindowBounds, WindowOptions,
    div, img, prelude::*, px, rgb, size,
};
use kver::pdf;

struct Preview {
    path: PathBuf,
    scale: f32,
    request: u64,
    status: String,
    page: Option<(Arc<Image>, f32, f32)>,
}

impl Preview {
    fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut preview = Self {
            path,
            scale: 1.0,
            request: 0,
            status: String::new(),
            page: None,
        };
        preview.request_render(cx);
        preview
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
        self.status = format!("Rendering at {}%…", (scale * 100.0).round());
        cx.notify();

        // Parsing, rasterization, and PNG encoding happen off the UI thread.
        let work = cx
            .background_executor()
            .spawn(async move { pdf::render_first_page(&path, scale) });
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
                            rendered.width,
                            rendered.height,
                        ));
                        this.status = format!("First page · {}%", (scale * 100.0).round());
                    }
                    Err(error) => {
                        this.page = None;
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page.as_ref().map(|(image, width, height)| {
            div()
                .flex()
                .justify_center()
                .child(img(image.clone()).w(px(*width)).h(px(*height)))
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

fn main() {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: cargo run -- path/to/document.pdf");
        std::process::exit(2);
    };

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(920.0), px(800.0)), cx);
        if let Err(error) = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| Preview::new(path, cx)),
        ) {
            eprintln!("Cannot open window: {error}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}
