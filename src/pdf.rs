//! Provisional PDF backend for the reader prototype.
//!
//! The UI receives bitmap pixels and document-space text geometry, never Hayro types.
use std::path::Path;

use hayro::{
    PixmapSettings, RenderCache, RenderSettings,
    hayro_interpret::{
        BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
        InterpreterCache, InterpreterSettings, SoftMask,
        font::{Glyph, GlyphRun},
        hayro_cmap::BfString,
        interpret_page,
    },
    hayro_syntax::{LoadPdfError, Pdf},
    kurbo::{Affine, BezPath, Point as KurboPoint, Rect, Shape},
    render,
    vello_cpu::{Pixmap, color::palette::css::WHITE},
};

use crate::coordinates::{PageTransform, Point};

pub struct RenderedPage {
    pub png: Vec<u8>,
    /// Number of pages in the opened document.
    pub page_count: usize,
    /// Actual bitmap width in image pixels.
    pub width: f32,
    /// Actual bitmap height in image pixels.
    pub height: f32,
    /// Unscaled page width in PDF points (logical display units at 100% zoom).
    pub page_width: f32,
    /// Unscaled page height in PDF points.
    pub page_height: f32,
}

/// Raw bitmap for direct display by the UI (premultiplied BGRA, row-major).
#[cfg(feature = "gui")]
pub struct RenderedBitmap {
    pub bgra: Vec<u8>,
    pub page_count: usize,
    pub width: u32,
    pub height: u32,
    pub page_width: f32,
    pub page_height: f32,
}

/// Rendered page size in PDF points, with the PDF-to-page transform.
pub struct PageGeometry {
    pub width: f32,
    pub height: f32,
    pub transform: PageTransform,
}

/// A decoded glyph's advance and visible height in document coordinates.
#[derive(Clone, Debug)]
pub struct TextGlyph {
    pub text: String,
    pub start: Point,
    pub end: Point,
    pub top: f64,
    pub bottom: f64,
}

pub struct PageText {
    pub transform: PageTransform,
    pub glyphs: Vec<TextGlyph>,
    /// Visual reading order for ordinary horizontal text, in PDF page coordinates.
    pub lines: Vec<std::ops::Range<usize>>,
}

fn order_visible_lines(glyphs: &mut [TextGlyph]) -> Vec<std::ops::Range<usize>> {
    glyphs.sort_by(|a, b| b.start.y.total_cmp(&a.start.y));
    let mut lines = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let baseline = glyphs[start].start.y;
        let mut end = start + 1;
        while end < glyphs.len() && (glyphs[end].start.y - baseline).abs() <= 3.0 {
            end += 1;
        }
        glyphs[start..end].sort_by(|a, b| a.start.x.total_cmp(&b.start.x));
        lines.push(start..end);
        start = end;
    }
    lines
}

struct TextCollector {
    crop: Rect,
    glyphs: Vec<TextGlyph>,
}

impl<'a> Device<'a> for TextCollector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        // This slice only targets filled visible text. Do not offer invisible
        // extraction text or duplicate a fill+stroke run on its stroke pass.
        if matches!(mode, DrawMode::Stroke(_) | DrawMode::Invisible) {
            return;
        }
        for (index, glyph) in run.glyphs().iter().enumerate() {
            let Some(unicode) = glyph.as_unicode() else {
                continue;
            };
            let transform = props.transform * glyph.transform();
            let origin = transform * KurboPoint::ZERO;
            let outline = match &**glyph {
                Glyph::Outline(outline) if !outline.outline().elements().is_empty() => {
                    Some((transform * outline.outline()).bounding_box())
                }
                _ => None,
            };
            let next = run
                .glyphs()
                .get(index + 1)
                .map(|next| props.transform * next.transform() * KurboPoint::ZERO)
                .unwrap_or_else(|| {
                    KurboPoint::new(
                        outline.map(|bounds| bounds.x1 + 1.0).unwrap_or(origin.x),
                        origin.y,
                    )
                });
            // Spaces have no outline but their advance belongs in the copy range.
            let bounds = outline.unwrap_or_else(|| {
                Rect::new(
                    origin.x.min(next.x),
                    origin.y - 2.0,
                    origin.x.max(next.x),
                    origin.y + 10.0,
                )
            });
            // An overlapping glyph may be clipped halfway by the page crop.
            // Its Unicode mapping is for the whole glyph, not the visible
            // fragment, so only offer fully contained glyphs for selection.
            if bounds.x0 < self.crop.x0
                || bounds.x1 > self.crop.x1
                || bounds.y0 < self.crop.y0
                || bounds.y1 > self.crop.y1
                || next.x <= origin.x
            {
                continue;
            }
            self.glyphs.push(TextGlyph {
                text: match unicode {
                    BfString::Char(c) => c.to_string(),
                    BfString::String(s) => s,
                },
                start: Point {
                    x: origin.x,
                    y: origin.y,
                },
                end: Point {
                    x: next.x,
                    y: next.y,
                },
                top: bounds.y1.max(origin.y + 8.0),
                bottom: bounds.y0.min(origin.y - 2.0),
            });
        }
    }
}

/// Decode text for one page. Call on a background worker, not the UI thread.
pub fn page_text(path: &Path, page_index: usize) -> Result<PageText, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let pdf = open_pdf(data)?;
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| format!("PDF has no page {}", page_index.saturating_add(1)))?;
    let box_ = page.media_box();
    let crop = page.intersected_crop_box();
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        Affine::IDENTITY,
        Rect::new(box_.x0, box_.y0, box_.x1, box_.y1),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut collector = TextCollector {
        crop: Rect::new(crop.x0, crop.y0, crop.x1, crop.y1),
        glyphs: Vec::new(),
    };
    interpret_page(page, &mut context, &mut collector);
    let lines = order_visible_lines(&mut collector.glyphs);
    Ok(PageText {
        transform: PageTransform::from_coefficients(page.initial_transform(true).as_coeffs()),
        glyphs: collector.glyphs,
        lines,
    })
}

/// Lightweight page sizes for laying out a continuous document without rasterizing it.
pub fn document_pages(path: &Path) -> Result<Vec<(f32, f32)>, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let pdf = open_pdf(data)?;
    Ok(pdf
        .pages()
        .iter()
        .map(|page| page.render_dimensions())
        .collect())
}

/// Renders a zero-based page index, independently of the UI.
pub fn render_page(path: &Path, page_index: usize, scale: f32) -> Result<RenderedPage, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    render_bytes(data, page_index, scale)
}

/// Render directly into display pixels, avoiding PNG encoding and decoding on page turns.
#[cfg(feature = "gui")]
pub fn render_bitmap(path: &Path, page_index: usize, scale: f32) -> Result<RenderedBitmap, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let (pixmap, page_count, page_width, page_height) = rasterize(data, page_index, scale)?;
    let width = u32::from(pixmap.width());
    let height = u32::from(pixmap.height());
    let mut bgra = pixmap.data_as_u8_slice().to_vec();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(RenderedBitmap {
        bgra,
        page_count,
        width,
        height,
        page_width,
        page_height,
    })
}

pub fn page_geometry(path: &Path, page_index: usize) -> Result<PageGeometry, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    geometry_bytes(data, page_index)
}

fn geometry_bytes(data: Vec<u8>, page_index: usize) -> Result<PageGeometry, String> {
    let pdf = open_pdf(data)?;
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| format!("PDF has no page {}", page_index.saturating_add(1)))?;
    let (width, height) = page.render_dimensions();
    Ok(PageGeometry {
        width,
        height,
        transform: PageTransform::from_coefficients(page.initial_transform(true).as_coeffs()),
    })
}

fn open_pdf(data: Vec<u8>) -> Result<Pdf, String> {
    Pdf::new(data).map_err(|error| match error {
        LoadPdfError::Invalid => "Cannot open PDF: invalid or unsupported PDF file".into(),
        other => format!("Cannot open PDF: {other:?}"),
    })
}

fn render_bytes(data: Vec<u8>, page_index: usize, scale: f32) -> Result<RenderedPage, String> {
    let (pixmap, page_count, width, height) = rasterize(data, page_index, scale)?;
    let bitmap_width = f32::from(pixmap.width());
    let bitmap_height = f32::from(pixmap.height());
    let png = pixmap
        .into_png()
        .map_err(|error| format!("Cannot encode page image: {error}"))?;
    Ok(RenderedPage {
        png,
        page_count,
        width: bitmap_width,
        height: bitmap_height,
        page_width: width,
        page_height: height,
    })
}

fn rasterize(
    data: Vec<u8>,
    page_index: usize,
    scale: f32,
) -> Result<(Pixmap, usize, f32, f32), String> {
    if !scale.is_finite() || !(0.25..=8.0).contains(&scale) {
        return Err("Raster scale must be between 0.25 and 8".into());
    }

    let pdf = open_pdf(data)?;
    let page_count = pdf.pages().len();
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| format!("PDF has no page {}", page_index.saturating_add(1)))?;
    let (width, height) = page.render_dimensions();
    let pixel_width = width * scale;
    let pixel_height = height * scale;
    // Hayro's simplified renderer converts dimensions to u16. Reject pages
    // that would overflow it or allocate an unexpectedly large bitmap.
    if !pixel_width.is_finite()
        || !pixel_height.is_finite()
        || pixel_width < 1.0
        || pixel_height < 1.0
        || pixel_width > 8192.0
        || pixel_height > 8192.0
        || pixel_width * pixel_height > 32_000_000.0
    {
        return Err("Page is too large to render at this scale".into());
    }

    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    Ok((pixmap, page_count, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinates::{PageViewport, Point};

    #[test]
    fn ordinary_text_exposes_copyable_advances_and_spaces() {
        let page = page_text(Path::new("examples/readme-sample.pdf"), 0).unwrap();
        let heading: String = page
            .glyphs
            .iter()
            .take(20)
            .map(|glyph| glyph.text.as_str())
            .collect();
        assert_eq!(heading, "KVER / READER SAMPLE");
        assert!(
            page.glyphs
                .iter()
                .take(20)
                .any(|glyph| glyph.text == " " && glyph.end.x > glyph.start.x)
        );
        assert!(page.glyphs.iter().all(|glyph| glyph.end.x > glyph.start.x));
    }

    #[test]
    fn visual_lines_do_not_follow_glyph_callback_order() {
        let mut glyphs: Vec<TextGlyph> = [
            (20.0, 80.0, "B"),
            (20.0, 100.0, "D"),
            (10.0, 80.0, "A"),
            (10.0, 100.0, "C"),
        ]
        .into_iter()
        .map(|(x, y, text)| TextGlyph {
            text: text.into(),
            start: Point { x, y },
            end: Point { x: x + 10.0, y },
            top: y + 10.0,
            bottom: y - 2.0,
        })
        .collect();
        let lines = order_visible_lines(&mut glyphs);
        assert_eq!(lines, [0..2, 2..4]);
        assert_eq!(
            glyphs
                .iter()
                .map(|glyph| glyph.text.as_str())
                .collect::<String>(),
            "CDAB"
        );
    }

    // A small, valid PDF assembled in memory so the rendering smoke test does
    // not depend on a developer's documents or a downloaded fixture.
    fn blank_pdf() -> Vec<u8> {
        blank_pdf_with_size("72", "96")
    }

    fn blank_pdf_with_size(width: &str, height: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Resources << >> >>"
            )
            .into_bytes(),
        ];
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
    }

    // Two pages with nonzero origins: a 90-degree rotated crop, then an
    // unrotated crop. Labels and outlines make rendered output inspectable.
    fn geometry_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let rotated = b"30 50 100 200 re S BT /F1 12 Tf 40 80 Td (ROTATED) Tj ET BT /F1 12 Tf 126 80 Td (CROSS) Tj ET BT /F1 12 Tf 40 280 Td (HIDDEN) Tj ET";
        let cropped = b"30 50 100 200 re S BT /F1 12 Tf 40 80 Td (CROPPED) Tj ET";
        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [10 20 210 320] /CropBox [30 50 130 250] /Rotate 90 /Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>".to_vec(),
            format!("<< /Length {} >>\nstream\n", rotated.len())
                .into_bytes()
                .into_iter()
                .chain(rotated.iter().copied())
                .chain(b"\nendstream".iter().copied())
                .collect(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [10 20 210 320] /CropBox [30 50 130 250] /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>".to_vec(),
            format!("<< /Length {} >>\nstream\n", cropped.len())
                .into_bytes()
                .into_iter()
                .chain(cropped.iter().copied())
                .chain(b"\nendstream".iter().copied())
                .collect(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 8\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
    }

    fn assert_point_near(actual: Point, expected: Point) {
        assert!(
            (actual.x - expected.x).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual.y - expected.y).abs() < 1e-6,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn rotated_crop_exposes_only_visible_glyphs() {
        let path = std::env::temp_dir().join(format!("kver-41-{}.pdf", std::process::id()));
        std::fs::write(&path, geometry_pdf()).unwrap();
        let page = page_text(&path, 0).unwrap();
        std::fs::remove_file(path).unwrap();
        let extracted: String = page
            .glyphs
            .iter()
            .map(|glyph| glyph.text.as_str())
            .collect();
        assert_eq!(extracted, "ROTATED");
        let start = page.transform.document_to_page(page.glyphs[0].start);
        let end = page.transform.document_to_page(page.glyphs[0].end);
        assert!(end.y > start.y, "rotated line follows the rendered y axis");
    }

    #[test]
    fn cropped_and_rotated_geometry_round_trips() {
        let corners = [Point { x: 30.0, y: 50.0 }, Point { x: 130.0, y: 250.0 }];
        for (index, size, opposite) in [
            (0, (200.0, 100.0), Point { x: 200.0, y: 100.0 }),
            (1, (100.0, 200.0), Point { x: 100.0, y: 200.0 }),
        ] {
            let geometry = geometry_bytes(geometry_pdf(), index).unwrap();
            assert_eq!((geometry.width, geometry.height), size);
            let rendered = render_bytes(geometry_pdf(), index, 2.0).unwrap();
            assert_eq!(
                (rendered.width, rendered.height),
                (size.0 * 2.0, size.1 * 2.0)
            );
            let png_size = (
                u32::from_be_bytes(rendered.png[16..20].try_into().unwrap()),
                u32::from_be_bytes(rendered.png[20..24].try_into().unwrap()),
            );
            assert_eq!(png_size, (rendered.width as u32, rendered.height as u32));
            assert_point_near(
                geometry.transform.document_to_page(corners[0]),
                if index == 0 {
                    Point { x: 0.0, y: 0.0 }
                } else {
                    Point { x: 0.0, y: 200.0 }
                },
            );
            assert_point_near(
                geometry.transform.document_to_page(corners[1]),
                if index == 0 {
                    opposite
                } else {
                    Point { x: 100.0, y: 0.0 }
                },
            );
            for corner in corners {
                let page = geometry.transform.document_to_page(corner);
                assert_point_near(geometry.transform.page_to_document(page).unwrap(), corner);
            }
        }
    }

    #[test]
    fn cropped_and_rotated_document_positions_survive_zoom_and_scroll() {
        let document = Point { x: 110.0, y: 80.0 };
        for (page_index, expected_screen) in [
            (0, Point { x: 145.0, y: 280.0 }),
            (1, Point { x: 220.0, y: 415.0 }),
        ] {
            let geometry = geometry_bytes(geometry_pdf(), page_index).unwrap();
            let viewport = PageViewport::new(
                geometry.transform,
                1.5,
                Point { x: 100.0, y: 200.0 },
                Point { x: 0.0, y: 40.0 },
            )
            .unwrap();
            let screen = viewport.document_to_screen(document);
            assert_point_near(screen, expected_screen);
            assert_point_near(viewport.screen_to_document(screen).unwrap(), document);

            let after_scroll = PageViewport::new(
                geometry.transform,
                1.5,
                Point { x: 100.0, y: 200.0 },
                Point { x: 25.0, y: 70.0 },
            )
            .unwrap();
            assert_point_near(
                after_scroll.document_to_screen(document),
                Point {
                    x: expected_screen.x - 25.0,
                    y: expected_screen.y - 30.0,
                },
            );
            assert_point_near(
                after_scroll
                    .screen_to_document(after_scroll.document_to_screen(document))
                    .unwrap(),
                document,
            );
        }
    }

    #[test]
    fn renders_first_page_at_requested_scale() {
        let original = render_bytes(blank_pdf(), 0, 1.0).unwrap();
        let zoomed = render_bytes(blank_pdf(), 0, 2.0).unwrap();
        assert_eq!((original.width, original.height), (72.0, 96.0));
        assert_eq!((zoomed.width, zoomed.height), (144.0, 192.0));
        assert_eq!((original.page_width, original.page_height), (72.0, 96.0));
        assert_eq!((zoomed.page_width, zoomed.page_height), (72.0, 96.0));
        assert!(zoomed.png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            u32::from_be_bytes(zoomed.png[16..20].try_into().unwrap()),
            144
        );
        assert_eq!(
            u32::from_be_bytes(zoomed.png[20..24].try_into().unwrap()),
            192
        );
    }

    #[test]
    fn reports_actual_bitmap_size_for_fractional_page_dimensions() {
        let original = render_bytes(blank_pdf_with_size("72.75", "96.5"), 0, 1.0).unwrap();
        let zoomed = render_bytes(blank_pdf_with_size("72.75", "96.5"), 0, 2.0).unwrap();
        assert_eq!((zoomed.page_width, zoomed.page_height), (72.75, 96.5));
        assert_eq!((original.width, original.height), (72.0, 96.0));
        assert_eq!((zoomed.width, zoomed.height), (145.0, 193.0));
        assert_eq!(
            u32::from_be_bytes(zoomed.png[16..20].try_into().unwrap()),
            zoomed.width as u32
        );
        assert_eq!(
            u32::from_be_bytes(zoomed.png[20..24].try_into().unwrap()),
            zoomed.height as u32
        );
    }

    #[test]
    fn renders_readme_example() {
        let sample = include_bytes!("../examples/readme-sample.pdf").to_vec();
        let page = render_bytes(sample, 0, 1.0).unwrap();
        assert_eq!((page.width, page.height), (612.0, 792.0));
        assert!(page.png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn renders_display_pixels_without_png_round_trip() {
        let start = std::time::Instant::now();
        let page = render_bitmap(Path::new("examples/readme-sample.pdf"), 49, 2.0).unwrap();
        println!(
            "2x bitmap elapsed_ms: {:.1}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(page.page_count, 100);
        assert_eq!((page.width, page.height), (1224, 1584));
        assert_eq!(page.bgra.len(), 1224 * 1584 * 4);
        assert_eq!(&page.bgra[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn rejects_invalid_pdf_and_scale() {
        assert!(render_bytes(b"not a PDF".to_vec(), 0, 1.0).is_err());
        assert!(render_bytes(blank_pdf(), 0, f32::NAN).is_err());
        assert!(render_bytes(blank_pdf(), 0, 8.0).is_ok());
    }

    #[test]
    fn rejects_page_outside_document() {
        let error = render_bytes(blank_pdf(), 1, 1.0).err().unwrap();
        assert_eq!(error, "PDF has no page 2");
    }

    #[test]
    fn reports_missing_file() {
        let error = render_page(Path::new("/this-file-does-not-exist.pdf"), 0, 1.0)
            .err()
            .unwrap();
        assert!(error.starts_with("Cannot read "));
    }
}
