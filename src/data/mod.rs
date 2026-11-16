//! The data rows: a thin read (file or standard input), and the pure parsers in `parse.rs`.

mod parse;

use std::io::Read;
use std::path::Path;

use crate::config::DataFormat;
use crate::errors::{Error, Result};

pub(crate) use parse::{Row, Table};

/// The path that means standard input.
const STDIN: &str = "-";

/// Reads the data text from `path`, or from standard input when `path` is `-`.
pub(crate) fn read_data(path: &Path) -> Result<String> {
    let read = || match path == Path::new(STDIN) {
        true => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).map(|_| text)
        }
        false => std::fs::read_to_string(path),
    };
    read().map_err(|source| Error::ReadData {
        path: path.to_owned(),
        source,
    })
}

/// The format to parse with: `auto` picks JSON for a `.json` path, CSV otherwise.
fn effective_format(path: &Path, format: DataFormat) -> DataFormat {
    match format {
        DataFormat::Auto => {
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("json"));
            match is_json {
                true => DataFormat::Json,
                false => DataFormat::Csv,
            }
        }
        explicit => explicit,
    }
}

/// Parses `text`, read from `path`, in the configured format.
pub(crate) fn parse_data(
    text: &str,
    path: &Path,
    format: DataFormat,
    delimiter: u8,
) -> Result<Table> {
    match effective_format(path, format) {
        DataFormat::Json => parse::parse_json(text),
        DataFormat::Csv | DataFormat::Auto => parse::parse_csv(text, delimiter),
    }
    .map_err(|e| Error::ParseData {
        path: path.to_owned(),
        source: Box::new(e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_format_follows_the_extension() {
        assert_eq!(
            effective_format(Path::new("a.json"), DataFormat::Auto),
            DataFormat::Json
        );
        assert_eq!(
            effective_format(Path::new("a.JSON"), DataFormat::Auto),
            DataFormat::Json
        );
        assert_eq!(
            effective_format(Path::new("a.csv"), DataFormat::Auto),
            DataFormat::Csv
        );
        assert_eq!(
            effective_format(Path::new("-"), DataFormat::Auto),
            DataFormat::Csv
        );
        assert_eq!(
            effective_format(Path::new("a.txt"), DataFormat::Json),
            DataFormat::Json
        );
    }

    #[test]
    fn parse_errors_carry_the_path() {
        let e = parse_data("[1", Path::new("d.json"), DataFormat::Auto, b',').unwrap_err();
        assert!(matches!(e, Error::ParseData { .. }));
        assert!(e.to_string().contains("d.json"));
    }

    #[test]
    fn reads_the_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/people.csv");
        let text = read_data(&path).unwrap();
        let table = parse_data(&text, &path, DataFormat::Auto, b',').unwrap();
        assert_eq!(table.rows.len(), 3);
    }

    #[test]
    fn missing_file_is_an_error() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/missing.csv");
        assert!(matches!(read_data(&path), Err(Error::ReadData { .. })));
    }
}
