//! Provisional PDF backend for the single-page prototype.
//!
//! The UI receives only PNG bytes and dimensions; Hayro is not part of its API.
use std::path::Path;

use hayro::{
    PixmapSettings, RenderCache, RenderSettings, hayro_interpret::InterpreterSettings,
    hayro_syntax::Pdf, render, vello_cpu::color::palette::css::WHITE,
};

pub struct RenderedPage {
    pub png: Vec<u8>,
    pub width: f32,
    pub height: f32,
}

pub fn render_first_page(path: &Path, scale: f32) -> Result<RenderedPage, String> {
    let data =
        std::fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    render_bytes(data, scale)
}

fn render_bytes(data: Vec<u8>, scale: f32) -> Result<RenderedPage, String> {
    if !scale.is_finite() || !(0.25..=4.0).contains(&scale) {
        return Err("Scale must be between 0.25 and 4".into());
    }

    let pdf = Pdf::new(data).map_err(|error| format!("Cannot open PDF: {error:?}"))?;
    let page = pdf.pages().first().ok_or("PDF has no pages")?;
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
    let png = pixmap
        .into_png()
        .map_err(|error| format!("Cannot encode page image: {error}"))?;
    Ok(RenderedPage {
        png,
        width: pixel_width,
        height: pixel_height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A small, valid PDF assembled in memory so the rendering smoke test does
    // not depend on a developer's documents or a downloaded fixture.
    fn blank_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let objects: [&[u8]; 3] = [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 96] /Resources << >> >>",
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

    #[test]
    fn renders_first_page_at_requested_scale() {
        let original = render_bytes(blank_pdf(), 1.0).unwrap();
        let zoomed = render_bytes(blank_pdf(), 2.0).unwrap();
        assert_eq!((original.width, original.height), (72.0, 96.0));
        assert_eq!((zoomed.width, zoomed.height), (144.0, 192.0));
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
    fn renders_readme_example() {
        let sample = include_bytes!("../examples/readme-sample.pdf").to_vec();
        let page = render_bytes(sample, 1.0).unwrap();
        assert_eq!((page.width, page.height), (612.0, 792.0));
        assert!(page.png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn rejects_invalid_pdf_and_scale() {
        assert!(render_bytes(b"not a PDF".to_vec(), 1.0).is_err());
        assert!(render_bytes(blank_pdf(), f32::NAN).is_err());
    }

    #[test]
    fn reports_missing_file() {
        let error = render_first_page(Path::new("/this-file-does-not-exist.pdf"), 1.0)
            .err()
            .unwrap();
        assert!(error.starts_with("Cannot read "));
    }
}
