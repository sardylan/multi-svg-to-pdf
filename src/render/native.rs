//! The built-in engine: `usvg` parses the SVG, `svg2pdf` writes the PDF.

use std::path::{Path, PathBuf};

use svg2pdf::usvg;

use crate::errors::{Error, Result};

/// Parsing options with the fonts loaded once and shared by every row.
pub(crate) struct Native {
    options: usvg::Options<'static>,
}

impl Native {
    /// Builds the engine. `system_fonts` loads the fonts installed on the machine (I/O).
    pub(crate) fn load(font_dirs: &[PathBuf], resources_dir: &Path, system_fonts: bool) -> Native {
        let mut options = usvg::Options {
            resources_dir: Some(resources_dir.to_owned()),
            ..usvg::Options::default()
        };
        let fonts = options.fontdb_mut();
        if system_fonts {
            fonts.load_system_fonts();
        }
        font_dirs.iter().for_each(|dir| fonts.load_fonts_dir(dir));
        Native { options }
    }

    /// Number of font faces available to the renderer.
    pub(crate) fn font_count(&self) -> usize {
        self.options.fontdb.len()
    }

    /// Renders `svg` to a PNG image at `dpi`.
    pub(crate) fn rasterize(&self, svg: &str, dpi: u32) -> Result<Vec<u8>> {
        let tree = usvg::Tree::from_str(svg, &self.options)?;
        // usvg sizes are pixels at `options.dpi` (96 by default).
        let scale = dpi as f32 / self.options.dpi;
        let size = tree.size();
        let (width, height) = (
            (size.width() * scale).round() as u32,
            (size.height() * scale).round() as u32,
        );
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or(Error::RasterSize { width, height })?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        Ok(pixmap.encode_png()?)
    }

    pub(crate) fn convert(&self, svg: &str) -> Result<Vec<u8>> {
        let tree = usvg::Tree::from_str(svg, &self.options)?;
        // svg2pdf assumes 72 dpi unless told; usvg sizes are pixels at `options.dpi`.
        let page = svg2pdf::PageOptions {
            dpi: self.options.dpi,
        };
        Ok(svg2pdf::to_pdf(
            &tree,
            svg2pdf::ConversionOptions::default(),
            page,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = include_str!("../../tests/fixtures/template/card.svg");

    fn engine() -> Native {
        Native::load(&[], Path::new("."), false)
    }

    #[test]
    fn converts_svg_to_pdf() {
        let pdf = engine().convert(CARD).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn vector_page_has_the_template_size() {
        // 87 x 57 mm is 246.61 x 161.57 points.
        let pdf = engine().convert(CARD).unwrap();
        let (w, h) = crate::render::raster::tests::page_size(&pdf);
        assert!(
            (w - 246.61).abs() < 0.01 && (h - 161.57).abs() < 0.01,
            "{w} x {h}"
        );
    }

    #[test]
    fn rasterizes_at_the_requested_resolution() {
        // The card is 87 x 57 mm: 1028 x 673 pixels at 300 dpi (rounded, as Inkscape does).
        let png = engine().rasterize(CARD, 300).unwrap();
        assert_eq!(crate::render::raster::png_size(&png).unwrap(), (1028, 673));
        let png = engine().rasterize(CARD, 96).unwrap();
        assert_eq!(crate::render::raster::png_size(&png).unwrap(), (329, 215));
    }

    #[test]
    fn invalid_svg_is_an_error() {
        assert!(matches!(engine().convert("<svg"), Err(Error::Svg(_))));
    }

    #[test]
    fn without_system_fonts_no_fonts_are_loaded() {
        assert_eq!(engine().font_count(), 0);
    }
}
