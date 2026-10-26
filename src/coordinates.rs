//! PDF document coordinates (y up), rendered page coordinates (y down), and
//! window coordinates in logical points (not physical display pixels).
//! This module does not depend on a PDF backend or on GPUI.

/// A point in PDF points, relative to the PDF's own coordinate origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Affine transform from PDF document coordinates to rendered page coordinates.
/// Coefficients follow the PDF convention: x' = ax + cy + e; y' = bx + dy + f.
#[derive(Clone, Copy, Debug)]
pub struct PageTransform {
    coefficients: [f64; 6],
}

impl PageTransform {
    pub fn from_coefficients(coefficients: [f64; 6]) -> Self {
        Self { coefficients }
    }

    pub fn document_to_page(self, point: Point) -> Point {
        let [a, b, c, d, e, f] = self.coefficients;
        Point {
            x: a * point.x + c * point.y + e,
            y: b * point.x + d * point.y + f,
        }
    }

    /// Returns `None` for a degenerate transform that cannot be inverted.
    pub fn page_to_document(self, point: Point) -> Option<Point> {
        let [a, b, c, d, e, f] = self.coefficients;
        let determinant = a * d - b * c;
        if !determinant.is_finite() || determinant.abs() < f64::EPSILON {
            return None;
        }
        let x = point.x - e;
        let y = point.y - f;
        Some(Point {
            x: (d * x - c * y) / determinant,
            y: (a * y - b * x) / determinant,
        })
    }
}

/// Placement of a rendered page in a window, before and after scrolling.
/// `page_origin` is the page's unscrolled top-left in window logical points;
/// positive `scroll` moves content left/up. Display density affects raster
/// resolution, not this mapping or persistent document coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PageViewport {
    page: PageTransform,
    zoom: f64,
    page_origin: Point,
    scroll: Point,
}

impl PageViewport {
    pub fn new(page: PageTransform, zoom: f64, page_origin: Point, scroll: Point) -> Option<Self> {
        if !zoom.is_finite()
            || zoom <= 0.0
            || !page_origin.x.is_finite()
            || !page_origin.y.is_finite()
            || !scroll.x.is_finite()
            || !scroll.y.is_finite()
        {
            return None;
        }
        Some(Self {
            page,
            zoom,
            page_origin,
            scroll,
        })
    }

    pub fn document_to_screen(self, document: Point) -> Point {
        let page = self.page.document_to_page(document);
        Point {
            x: self.page_origin.x + page.x * self.zoom - self.scroll.x,
            y: self.page_origin.y + page.y * self.zoom - self.scroll.y,
        }
    }

    pub fn screen_to_document(self, screen: Point) -> Option<Point> {
        let page = Point {
            x: (screen.x + self.scroll.x - self.page_origin.x) / self.zoom,
            y: (screen.y + self.scroll.y - self.page_origin.y) / self.zoom,
        };
        self.page.page_to_document(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_noninvertible_transform() {
        let transform = PageTransform::from_coefficients([1.0, 0.0, 2.0, 0.0, 0.0, 0.0]);
        assert_eq!(transform.page_to_document(Point { x: 2.0, y: 3.0 }), None);
    }

    #[test]
    fn rejects_invalid_viewport_zoom_and_offset() {
        let page = PageTransform::from_coefficients([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let origin = Point { x: 0.0, y: 0.0 };
        assert!(PageViewport::new(page, 0.0, origin, origin).is_none());
        assert!(PageViewport::new(page, f64::NAN, origin, origin).is_none());
        assert!(
            PageViewport::new(
                page,
                1.0,
                Point {
                    x: f64::INFINITY,
                    y: 0.0
                },
                origin
            )
            .is_none()
        );
    }
}
