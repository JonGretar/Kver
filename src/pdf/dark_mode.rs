//! Display-only recoloring of opaque BGRA page rasters, including scans.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageColors {
    paper: [f32; 3],
    ink: [f32; 3],
}

impl PageColors {
    pub(crate) fn new(paper: [f32; 3], ink: [f32; 3]) -> Self {
        Self { paper, ink }
    }

    pub(crate) fn paper(self) -> [f32; 3] {
        self.paper
    }

    pub(crate) fn apply(self, bgra: &mut [u8]) {
        for pixel in bgra.as_chunks_mut::<4>().0 {
            let rgb = [pixel[2], pixel[1], pixel[0]].map(|channel| channel as f32 / 255.);
            let lightness = rgb[0] * 0.30 + rgb[1] * 0.59 + rgb[2] * 0.11;
            let base = std::array::from_fn::<_, 3, _>(|i| {
                self.ink[i] + lightness * (self.paper[i] - self.ink[i])
            });
            let chroma = rgb.map(|channel| channel - lightness);
            // Retain the hue direction, reducing chroma uniformly only as much
            // as needed to fit the RGB gamut. Clipping channels separately
            // would shift hues. Gray pixels interpolate the theme endpoints.
            let mut saturation = 1.0_f32;
            for i in 0..3 {
                if chroma[i] > 0. {
                    saturation = saturation.min((1. - base[i]) / chroma[i]);
                } else if chroma[i] < 0. {
                    saturation = saturation.min(-base[i] / chroma[i]);
                }
            }
            for i in 0..3 {
                pixel[2 - i] =
                    ((base[i] + saturation * chroma[i]).clamp(0., 1.) * 255.).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paper_ink_and_antialiasing_without_changing_alpha() {
        let colors = PageColors::new([0.1, 0.12, 0.15], [0.9, 0.92, 0.95]);
        let mut pixels = [255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255];
        colors.apply(&mut pixels);
        assert_eq!(&pixels[..4], &[38, 31, 26, 255]);
        assert_eq!(&pixels[4..8], &[242, 235, 230, 255]);
        for i in 0..3 {
            assert!(pixels[i] < pixels[8 + i] && pixels[8 + i] < pixels[4 + i]);
        }
    }

    #[test]
    fn preserves_primary_hues_instead_of_inverting_them() {
        let colors = PageColors::new([0.1; 3], [0.9; 3]);
        for channel in 0..3 {
            let mut pixel = [0, 0, 0, 255];
            pixel[channel] = 255;
            colors.apply(&mut pixel);
            assert!(pixel[channel] > pixel[(channel + 1) % 3]);
            assert!(pixel[channel] > pixel[(channel + 2) % 3]);
            assert_eq!(pixel[3], 255);
        }
    }
}
