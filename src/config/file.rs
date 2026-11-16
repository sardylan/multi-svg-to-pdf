//! The TOML configuration file (§3.1): a thin read, and a pure parse.

use std::io::ErrorKind;
use std::path::Path;

use super::Partial;
use crate::errors::{Error, Result};

/// Reads the file; `None` when it does not exist, which is not an error.
pub(crate) fn read_config_file(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::ReadConfig {
            path: path.to_owned(),
            source,
        }),
    }
}

/// Parses the file's text. Unknown keys are refused.
pub(crate) fn parse_config_file(text: &str, path: &Path) -> Result<Partial> {
    toml::from_str(text).map_err(|source| Error::ParseConfig {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = include_str!("../../tests/fixtures/config/full.toml");

    #[test]
    fn parses_every_key() {
        let partial = parse_config_file(FULL, Path::new("full.toml")).unwrap();
        assert_eq!(partial.template.path, Some("badge.svg".into()));
        assert_eq!(partial.template.placeholder_open.as_deref(), Some("${"));
        assert_eq!(partial.data.csv_delimiter.as_deref(), Some(";"));
        assert_eq!(partial.output.merge, Some("all.pdf".into()));
        assert_eq!(partial.output.force, Some(true));
        assert_eq!(partial.render.jobs, Some(4));
        assert_eq!(partial.render.font_dirs, Some(vec!["fonts".into()]));
        assert_eq!(partial.render.engine.as_deref(), Some("inkscape"));
        assert_eq!(partial.render.inkscape, Some("/usr/bin/inkscape".into()));
        assert_eq!(partial.render.format.as_deref(), Some("raster"));
        assert_eq!(partial.render.dpi, Some(150));
        assert_eq!(partial.log.level.as_deref(), Some("debug"));
    }

    #[test]
    fn empty_file_sets_nothing() {
        assert_eq!(
            parse_config_file("", Path::new("e.toml")).unwrap(),
            Partial::default()
        );
    }

    #[test]
    fn unknown_keys_are_refused() {
        ["[data]\npaht = \"x\"\n", "[unknown]\n"]
            .iter()
            .for_each(|text| {
                assert!(matches!(
                    parse_config_file(text, Path::new("u.toml")),
                    Err(Error::ParseConfig { .. })
                ));
            });
    }

    #[test]
    fn wrong_types_are_refused() {
        let e = parse_config_file("[render]\njobs = \"four\"\n", Path::new("w.toml"));
        assert!(matches!(e, Err(Error::ParseConfig { .. })));
    }

    #[test]
    fn missing_file_is_none() {
        let path = std::env::temp_dir().join(format!(
            "multi-svg-to-pdf-test-missing-{}.toml",
            std::process::id()
        ));
        assert_eq!(read_config_file(&path).unwrap(), None);
    }

    #[test]
    fn existing_file_is_read() {
        let path = std::env::temp_dir().join(format!(
            "multi-svg-to-pdf-test-read-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, FULL).unwrap();
        let text = read_config_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(text.as_deref(), Some(FULL));
    }

    #[test]
    fn unreadable_path_is_an_error() {
        // A directory exists but cannot be read as a file.
        assert!(matches!(
            read_config_file(&std::env::temp_dir()),
            Err(Error::ReadConfig { .. })
        ));
    }
}
