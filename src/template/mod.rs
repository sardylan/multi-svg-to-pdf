//! The SVG template and the output name pattern: a thin read, and the pure parser in `parse.rs`.

mod parse;

use std::path::Path;

use crate::errors::{Error, Result};

pub(crate) use parse::{Escape, Template};

/// Reads the template file as UTF-8 text.
pub(crate) fn read_template(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::ReadTemplate {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template/card.svg");
        assert!(read_template(&path).unwrap().contains("{{name}}"));
    }

    #[test]
    fn missing_template_is_an_error() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/template/missing.svg");
        assert!(matches!(
            read_template(&path),
            Err(Error::ReadTemplate { .. })
        ));
    }
}
