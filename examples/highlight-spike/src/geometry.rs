//! Fixture-scale geometry probe, independent of PDFKit.
//! Matching whole fixture text runs is not general reading-order analysis.
use std::{fs, path::Path};

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

const TARGETS: [&str; 3] = [
    "LEFT alpha selection",
    "LEFT beta selection",
    "RIGHT alpha selection",
];

#[derive(Debug, PartialEq)]
pub struct Regions {
    pub ink: Vec<[f32; 4]>,
    pub selection: Vec<[f32; 4]>,
}

#[derive(Default)]
struct Probe {
    regions: Vec<(String, Rect, Rect)>,
}

impl<'a> Device<'a> for Probe {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Stroke(_) | DrawMode::Invisible) {
            return;
        }
        let text: String = run
            .glyphs()
            .iter()
            .filter_map(|glyph| {
                glyph.as_unicode().map(|text| match text {
                    BfString::Char(c) => c.to_string(),
                    BfString::String(s) => s,
                })
            })
            .collect();
        if !TARGETS.contains(&text.as_str()) {
            return;
        }
        let mut ink: Option<Rect> = None;
        let mut selection: Option<Rect> = None;
        for glyph in run.glyphs() {
            let transform = props.transform * glyph.transform();
            let origin = transform * Point::ZERO;
            let Glyph::Outline(outline) = &**glyph else {
                continue;
            };
            let path = outline.outline();
            if path.elements().is_empty() {
                continue; // Spaces remain between the line's first/last advances.
            }
            let bounds = (transform * path).bounding_box();
            ink = Some(ink.map_or(bounds, |old| old.union(bounds)));
            let end = outline
                .advance_width()
                .map(|advance| transform * Point::new(f64::from(advance), 0.0))
                .unwrap_or(Point::new(bounds.x1 + 1.0, origin.y));
            // Same minimum vertical envelope used by the current reader's
            // TextCollector; horizontal end uses the glyph's real advance,
            // never the next column's position or a guessed text width.
            let selected = Rect::new(
                origin.x,
                bounds.y0.min(origin.y - 2.0),
                end.x,
                bounds.y1.max(origin.y + 8.0),
            );
            selection = Some(selection.map_or(selected, |old| old.union(selected)));
        }
        if let (Some(ink), Some(selection)) = (ink, selection) {
            self.regions.push((text, ink, selection));
        }
    }
}

pub fn extract(path: &Path, index: usize) -> super::Result<Regions> {
    let pdf = Pdf::new(fs::read(path)?).map_err(|e| format!("{e:?}"))?;
    let page = pdf.pages().get(index).ok_or("missing probe page")?;
    let media = page.media_box();
    let cache = InterpreterCache::new();
    let settings = InterpreterSettings {
        render_annotations: false,
        ..Default::default()
    };
    let mut context = Context::new(
        Affine::IDENTITY,
        Rect::new(media.x0, media.y0, media.x1, media.y1),
        &cache,
        page.xref(),
        settings,
    );
    let mut probe = Probe::default();
    interpret_page(page, &mut context, &mut probe);
    let mut regions = Regions {
        ink: Vec::new(),
        selection: Vec::new(),
    };
    for target in TARGETS {
        let candidates: Vec<_> = probe
            .regions
            .iter()
            .filter(|(text, _, _)| text == target)
            .collect();
        if candidates.len() != 1 {
            return Err(format!(
                "Expected one text run for {target}, got {}",
                candidates.len()
            )
            .into());
        }
        let (_, ink, selection) = candidates[0];
        for (output, bounds) in [(&mut regions.ink, ink), (&mut regions.selection, selection)] {
            output.push([
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.x1 as f32,
                bounds.y1 as f32,
            ]);
        }
    }
    Ok(regions)
}
