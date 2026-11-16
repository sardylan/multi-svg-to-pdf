//! Turning one rendered SVG into PDF bytes, with the configured engine and format.

mod inkscape;
mod native;
pub(crate) mod raster;

use std::path::Path;

use crate::config::{Engine, RenderConfig, RenderFormat};
use crate::errors::Result;

use inkscape::Export;

enum Backend {
    Native(native::Native),
    Inkscape(inkscape::Inkscape),
}

/// The engine, set up once and shared by all rows (and threads).
pub(crate) struct Renderer {
    backend: Backend,
    format: RenderFormat,
    dpi: u32,
}

impl Renderer {
    /// Sets up the engine. `template_dir` is where relative links in the template resolve.
    /// The native engine loads the system fonts here (I/O).
    pub(crate) fn load(config: &RenderConfig, template_dir: &Path) -> Renderer {
        let backend = match config.engine {
            Engine::Native => {
                Backend::Native(native::Native::load(&config.font_dirs, template_dir, true))
            }
            Engine::Inkscape => {
                Backend::Inkscape(inkscape::Inkscape::new(&config.inkscape, template_dir))
            }
        };
        Renderer {
            backend,
            format: config.format,
            dpi: config.dpi,
        }
    }

    /// Short description for the logs.
    pub(crate) fn describe(&self) -> String {
        let engine = match &self.backend {
            Backend::Native(native) => {
                format!("native engine ({} font faces)", native.font_count())
            }
            Backend::Inkscape(_) => "inkscape engine".to_owned(),
        };
        match self.format {
            RenderFormat::Vector => format!("{engine}, vector output"),
            RenderFormat::Raster => format!("{engine}, raster output at {} dpi", self.dpi),
        }
    }

    pub(crate) fn render_pdf(&self, svg: &str) -> Result<Vec<u8>> {
        match (self.format, &self.backend) {
            (RenderFormat::Vector, Backend::Native(native)) => native.convert(svg),
            (RenderFormat::Vector, Backend::Inkscape(inkscape)) => {
                inkscape.run_inkscape(svg, Export::Pdf)
            }
            (RenderFormat::Raster, Backend::Native(native)) => {
                raster::png_to_pdf(&native.rasterize(svg, self.dpi)?, self.dpi)
            }
            (RenderFormat::Raster, Backend::Inkscape(inkscape)) => raster::png_to_pdf(
                &inkscape.run_inkscape(svg, Export::Png { dpi: self.dpi })?,
                self.dpi,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1in" height="1in"><rect width="48" height="48"/></svg>"#;

    fn config(engine: Engine, format: RenderFormat) -> RenderConfig {
        RenderConfig {
            jobs: 0,
            font_dirs: vec![],
            engine,
            inkscape: PathBuf::from("/nonexistent/ink"),
            format,
            dpi: 150,
        }
    }

    fn native(format: RenderFormat) -> Renderer {
        Renderer {
            backend: Backend::Native(native::Native::load(&[], Path::new("."), false)),
            format,
            dpi: 150,
        }
    }

    fn image_count(pdf: &[u8]) -> usize {
        let doc = lopdf::Document::load_mem(pdf).unwrap();
        doc.objects
            .values()
            .filter(|o| {
                o.as_stream().is_ok_and(|s| {
                    s.dict
                        .get(b"Subtype")
                        .and_then(|v| v.as_name())
                        .is_ok_and(|n| n == b"Image")
                })
            })
            .count()
    }

    #[test]
    fn picks_the_configured_engine() {
        let inkscape = Renderer::load(
            &config(Engine::Inkscape, RenderFormat::Raster),
            Path::new("."),
        );
        assert!(matches!(inkscape.backend, Backend::Inkscape(_)));
        assert_eq!(
            inkscape.describe(),
            "inkscape engine, raster output at 150 dpi"
        );
        assert!(inkscape.render_pdf("<svg/>").is_err());
    }

    #[test]
    fn native_vector_has_no_image() {
        let pdf = native(RenderFormat::Vector).render_pdf(SQUARE).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(image_count(&pdf), 0);
        assert!(
            native(RenderFormat::Vector)
                .describe()
                .ends_with("vector output")
        );
    }

    #[test]
    fn native_raster_is_one_image() {
        let pdf = native(RenderFormat::Raster).render_pdf(SQUARE).unwrap();
        assert!(image_count(&pdf) >= 1);
    }
}
