//! Experimental Hayro text probe; glyph bounds are not yet a reader API.
//! Run with `cargo +stable run --no-default-features --example text_geometry_probe -- PDF PAGE`.
use std::{env, error::Error, fs};

use hayro::{
    hayro_interpret::{
        BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
        InterpreterCache, InterpreterSettings, SoftMask,
        font::{Glyph, GlyphRun},
        hayro_cmap::BfString,
        interpret_page,
    },
    hayro_syntax::Pdf,
    kurbo::{Affine, BezPath, Point, Rect, Shape},
};
use kver::coordinates::{PageTransform, Point as DocumentPoint};

struct GlyphSample {
    text: String,
    origin: Point,
    bounds: Option<Rect>,
}

struct TextProbe {
    glyphs: usize,
    decoded: usize,
    in_crop: usize,
    crop_box: Rect,
    page_transform: PageTransform,
    preview: Vec<GlyphSample>,
    outside_preview: Vec<GlyphSample>,
}

impl TextProbe {
    fn new(crop_box: Rect, page_transform: PageTransform) -> Self {
        Self {
            glyphs: 0,
            decoded: 0,
            in_crop: 0,
            crop_box,
            page_transform,
            preview: Vec::new(),
            outside_preview: Vec::new(),
        }
    }
}

impl<'a> Device<'a> for TextProbe {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        // Fill+stroke invokes this callback twice. Only count the filled pass.
        if matches!(mode, DrawMode::Stroke(_)) {
            return;
        }
        for positioned in run.glyphs() {
            self.glyphs += 1;
            let Some(unicode) = positioned.as_unicode() else {
                continue;
            };
            self.decoded += 1;
            // With an identity interpretation transform these bounds remain
            // in PDF document coordinates, including its nonzero origin.
            let transform = props.transform * positioned.transform();
            let origin = transform * Point::ZERO;
            let bounds = match &**positioned {
                Glyph::Outline(glyph) => {
                    let path = glyph.outline();
                    (!path.elements().is_empty()).then(|| (transform * path).bounding_box())
                }
                Glyph::Type3(_) => None,
            };
            // This is only a crop-box candidate, not full visibility: other
            // PDF clipping paths and transparency may hide a glyph.
            let in_crop = bounds
                .map(|bounds| bounds.intersect(self.crop_box).area() > 0.0)
                .unwrap_or_else(|| self.crop_box.contains(origin));
            if in_crop {
                self.in_crop += 1;
            }
            let preview = if in_crop {
                &mut self.preview
            } else {
                &mut self.outside_preview
            };
            if preview.len() >= if in_crop { 20 } else { 5 } {
                continue;
            }
            let text = match unicode {
                BfString::Char(value) => value.to_string(),
                BfString::String(value) => value,
            };
            preview.push(GlyphSample {
                text,
                origin,
                bounds,
            });
        }
    }
}

fn probe_page(data: Vec<u8>, page_index: usize) -> Result<TextProbe, Box<dyn Error>> {
    let pdf = Pdf::new(data).map_err(|error| format!("Cannot open PDF: {error:?}"))?;
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or("Page is outside the document")?;
    let box_ = page.media_box();
    let crop = page.intersected_crop_box();
    let page_transform = PageTransform::from_coefficients(page.initial_transform(true).as_coeffs());
    let cache = InterpreterCache::new();
    let mut context = Context::new(
        Affine::IDENTITY,
        Rect::new(box_.x0, box_.y0, box_.x1, box_.y1),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut probe = TextProbe::new(
        Rect::new(crop.x0, crop.y0, crop.x1, crop.y1),
        page_transform,
    );
    interpret_page(page, &mut context, &mut probe);
    Ok(probe)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let (Some(path), Some(page_number), None) = (args.next(), args.next(), args.next()) else {
        eprintln!("Usage: text_geometry_probe PDF PAGE_NUMBER (1-based)");
        std::process::exit(2);
    };
    let page_number: usize = page_number.to_string_lossy().parse()?;
    let page_index = page_number
        .checked_sub(1)
        .ok_or("Page number must be at least 1")?;
    let probe = probe_page(fs::read(path)?, page_index)?;
    println!(
        "glyphs: {}, decoded: {}, in crop: {}",
        probe.glyphs, probe.decoded, probe.in_crop
    );
    println!("in-crop glyphs (first 20), then excluded (first 5):");
    println!(
        "text\tdocument_x\tdocument_y\tpage_x\tpage_y\tbounds_x0\tbounds_y0\tbounds_x1\tbounds_y1"
    );
    for sample in probe.preview.iter().chain(&probe.outside_preview) {
        let page = probe.page_transform.document_to_page(DocumentPoint {
            x: sample.origin.x,
            y: sample.origin.y,
        });
        if let Some(bounds) = sample.bounds {
            println!(
                "{:?}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t{:.1}",
                sample.text,
                sample.origin.x,
                sample.origin.y,
                page.x,
                page.y,
                bounds.x0,
                bounds.y0,
                bounds.x1,
                bounds.y1
            );
        } else {
            println!(
                "{:?}\t{:.1}\t{:.1}\t{:.1}\t{:.1}\t(no outline bounds)",
                sample.text, sample.origin.x, sample.origin.y, page.x, page.y
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cropped_pdf() -> Vec<u8> {
        let text = b"BT /F1 12 Tf 20 50 Td (OUTSIDE) Tj 70 0 Td (IN) Tj ET";
        let objects = [
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /CropBox [80 0 120 100] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
            [format!("<< /Length {} >>\nstream\n", text.len()).as_bytes(), text, b"\nendstream"].concat(),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n")
                .as_bytes(),
        );
        pdf
    }

    #[test]
    fn sample_exposes_positioned_text_in_document_coordinates() {
        let probe = probe_page(include_bytes!("readme-sample.pdf").to_vec(), 0).unwrap();
        assert_eq!(probe.glyphs, probe.decoded);
        assert!(probe.glyphs > 900);
        let first = &probe.preview[0];
        assert_eq!(first.text, "K");
        assert!((first.origin.x - 50.0).abs() < 0.01);
        assert!((first.origin.y - 750.0).abs() < 0.01);
        assert!(first.bounds.unwrap().width() > 0.0);
        assert_eq!(probe.preview[4].text, " ");
        assert!(probe.preview[4].bounds.is_none());
    }

    #[test]
    fn crop_excludes_hidden_text_without_changing_document_coordinates() {
        let probe = probe_page(cropped_pdf(), 0).unwrap();
        assert_eq!((probe.glyphs, probe.decoded, probe.in_crop), (9, 9, 2));
        assert_eq!(probe.preview[0].text, "I");
        assert_eq!(probe.preview[1].text, "N");
        assert_eq!(probe.outside_preview[0].text, "O");
        let first = &probe.preview[0];
        assert!((first.origin.x - 90.0).abs() < 0.01);
        assert!((first.origin.y - 50.0).abs() < 0.01);
        let page = probe.page_transform.document_to_page(DocumentPoint {
            x: first.origin.x,
            y: first.origin.y,
        });
        assert!((page.x - 10.0).abs() < 0.01);
        assert!((page.y - 50.0).abs() < 0.01);
    }
}
