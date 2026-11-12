//! Backend-neutral PDF text markup shared by browsing and editing.
//!
//! Coordinates and quad point order are preserved from the PDF. Identity is
//! valid only within a session epoch, never across an external reload.

use crate::coordinates::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AnnotationId {
    epoch: u64,
    serial: u64,
}

impl AnnotationId {
    pub fn new(epoch: u64, serial: u64) -> Self {
        Self { epoch, serial }
    }
    pub fn epoch(self) -> u64 {
        self.epoch
    }
    pub fn serial(self) -> u64 {
        self.serial
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl MarkupKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::StrikeOut => "Strikethrough",
            Self::Squiggly => "Squiggly underline",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    points: [Point; 4],
}

impl Quad {
    pub fn new(points: [Point; 4]) -> Option<Self> {
        if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return None;
        }
        // Accept both PDF perimeter order and the widely used Z order, but
        // reject collapsed/collinear regions. Do not reorder stored points.
        let cross =
            |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if (1..3).all(|i| cross(points[0], points[i], points[i + 1]).abs() < 1e-8) {
            return None;
        }
        Some(Self { points })
    }
    pub fn points(self) -> [Point; 4] {
        self.points
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnnotationColor {
    Unspecified,
    Gray(f32),
    Rgb([f32; 3]),
    Cmyk([f32; 4]),
}

impl AnnotationColor {
    /// Display conversion only; saving preserves the original color space.
    pub fn rgb(self) -> Option<[f32; 3]> {
        match self {
            Self::Unspecified => None,
            Self::Gray(gray) => Some([gray; 3]),
            Self::Rgb(rgb) => Some(rgb),
            Self::Cmyk([c, m, y, k]) => Some([
                1. - (c + k).min(1.),
                1. - (m + k).min(1.),
                1. - (y + k).min(1.),
            ]),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Annotation {
    id: AnnotationId,
    page: usize,
    kind: MarkupKind,
    quads: Vec<Quad>,
    color: AnnotationColor,
    opacity: f32,
    author: Option<String>,
    comment: Option<String>,
    name: Option<String>,
    readonly: bool,
    fallback_rect: Option<[f64; 4]>,
}

impl Annotation {
    pub fn new(
        id: AnnotationId,
        page: usize,
        kind: MarkupKind,
        quads: Vec<Quad>,
        color: AnnotationColor,
    ) -> Self {
        Self {
            id,
            page,
            kind,
            quads,
            color,
            opacity: 1.,
            author: None,
            comment: None,
            name: None,
            readonly: false,
            fallback_rect: None,
        }
    }
    pub fn id(&self) -> AnnotationId {
        self.id
    }
    pub fn page(&self) -> usize {
        self.page
    }
    pub fn kind(&self) -> MarkupKind {
        self.kind
    }
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }
    pub fn color(&self) -> AnnotationColor {
        self.color
    }
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn is_readonly(&self) -> bool {
        self.readonly
    }
    pub fn with_readonly(mut self, readonly: bool) -> Self {
        self.readonly = readonly;
        self
    }
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn with_metadata(
        mut self,
        author: Option<String>,
        comment: Option<String>,
        name: Option<String>,
    ) -> Self {
        self.author = author;
        self.comment = comment;
        self.name = name;
        self
    }
    pub fn with_fallback_rect(mut self, rect: Option<[f64; 4]>) -> Self {
        self.fallback_rect = rect;
        self
    }
    pub fn fallback_rect(&self) -> Option<[f64; 4]> {
        self.fallback_rect
    }
}
