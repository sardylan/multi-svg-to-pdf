//! Raster output: a PNG image wrapped into a one-page PDF. The page size is the image size at
//! the given resolution, so the card keeps its physical size.

use base64::Engine as _;
use svg2pdf::usvg;

use crate::errors::{Error, Result};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_be_bytes)
}

/// Width and height in pixels, from the PNG header (IHDR, always the first chunk).
pub(crate) fn png_size(png: &[u8]) -> Result<(u32, u32)> {
    let invalid = |reason| Error::InvalidPng { reason };
    match (png.get(..8), png.get(12..16)) {
        (Some(signature), _) if signature != SIGNATURE => Err(invalid("not a PNG signature")),
        (Some(_), Some(b"IHDR")) => read_u32(png, 16)
            .zip(read_u32(png, 20))
            .filter(|(w, h)| *w > 0 && *h > 0)
            .ok_or_else(|| invalid("missing or empty image size")),
        (Some(_), Some(_)) => Err(invalid("the first chunk is not IHDR")),
        _ => Err(invalid("too short")),
    }
}

/// An SVG page that shows `png` (`width` × `height` pixels) at `dpi`.
fn wrapper_svg(png: &[u8], width: u32, height: u32, dpi: u32) -> String {
    let points = |pixels: u32| f64::from(pixels) * 72.0 / f64::from(dpi);
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
            r#"width="{}pt" height="{}pt" viewBox="0 0 {w} {h}">"#,
            r#"<image width="{w}" height="{h}" preserveAspectRatio="none" "#,
            r#"xlink:href="data:image/png;base64,{data}"/></svg>"#
        ),
        points(width),
        points(height),
        w = width,
        h = height,
        data = base64::engine::general_purpose::STANDARD.encode(png),
    )
}

/// Wraps `png` into a PDF page of its size at `dpi`.
pub(crate) fn png_to_pdf(png: &[u8], dpi: u32) -> Result<Vec<u8>> {
    let (width, height) = png_size(png)?;
    let tree = usvg::Tree::from_str(
        &wrapper_svg(png, width, height, dpi),
        &usvg::Options::default(),
    )?;
    Ok(svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        // usvg sizes are pixels at `Options::dpi`; svg2pdf assumes 72 dpi unless told.
        svg2pdf::PageOptions {
            dpi: usvg::Options::default().dpi,
        },
    )?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `width` × `height` red PNG.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        pixmap.encode_png().unwrap()
    }

    pub(crate) fn page_size(pdf: &[u8]) -> (f64, f64) {
        let doc = lopdf::Document::load_mem(pdf).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let media = doc
            .get_dictionary(page)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap();
        let n = |o: &lopdf::Object| {
            o.as_float()
                .map(f64::from)
                .or_else(|_| o.as_i64().map(|i| i as f64))
                .unwrap()
        };
        (n(&media[2]) - n(&media[0]), n(&media[3]) - n(&media[1]))
    }

    #[test]
    fn reads_the_size_from_the_header() {
        assert_eq!(png_size(&png(1028, 673)).unwrap(), (1028, 673));
    }

    #[test]
    fn rejects_what_is_not_a_png() {
        let bad = |bytes: &[u8]| matches!(png_size(bytes), Err(Error::InvalidPng { .. }));
        assert!(bad(b""));
        assert!(bad(b"%PDF-1.7 not a png at all"));
        let mut no_ihdr = png(2, 2);
        no_ihdr[12..16].copy_from_slice(b"IDAT");
        assert!(bad(&no_ihdr));
        assert!(bad(&png(2, 2)[..18]));
    }

    #[test]
    fn page_keeps_the_physical_size() {
        // 1028 x 673 pixels at 300 dpi is 246.72 x 161.52 points.
        let (w, h) = page_size(&png_to_pdf(&png(1028, 673), 300).unwrap());
        assert!(
            (w - 246.72).abs() < 0.01 && (h - 161.52).abs() < 0.01,
            "{w} x {h}"
        );
    }

    #[test]
    fn pdf_holds_one_image() {
        let pdf = png_to_pdf(&png(10, 10), 72).unwrap();
        let doc = lopdf::Document::load_mem(&pdf).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        let images = doc
            .objects
            .values()
            .filter(|o| {
                o.as_stream().is_ok_and(|s| {
                    s.dict
                        .get(b"Subtype")
                        .and_then(|v| v.as_name())
                        .is_ok_and(|n| n == b"Image")
                })
            })
            .count();
        assert!(images >= 1);
    }
}
