//! The crate error type and `Result` alias (doc/rust-guidelines.md §1).

use std::fmt;
use std::path::PathBuf;
use std::process::{ExitCode, ExitStatus};

use crate::config::Source;

#[derive(Debug)]
pub(crate) enum Error {
    // Foreign errors, each with a `From` conversion.
    Io(std::io::Error),
    Toml(toml::de::Error),
    Cli(clap::Error),
    Logging(tracing::subscriber::SetGlobalDefaultError),
    Csv(csv::Error),
    Json(serde_json::Error),
    Svg(svg2pdf::usvg::Error),
    Pdf(svg2pdf::ConversionError),
    Merge(lopdf::Error),
    ThreadPool(rayon::ThreadPoolBuildError),
    Png(png::EncodingError),

    // Context around a foreign error.
    ReadConfig {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseConfig {
        path: PathBuf,
        source: toml::de::Error,
    },
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    WriteConfig {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadTemplate {
        path: PathBuf,
        source: std::io::Error,
    },
    ReadData {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseData {
        path: PathBuf,
        source: Box<Error>,
    },
    RunInkscape {
        program: PathBuf,
        source: std::io::Error,
    },
    WriteOutput {
        path: PathBuf,
        source: std::io::Error,
    },
    Row {
        row: usize,
        source: Box<Error>,
    },

    // Our own checks.
    Invalid {
        setting: &'static str,
        source: Source,
        input: String,
        reason: String,
    },
    NonUtf8Path {
        setting: &'static str,
        path: PathBuf,
    },
    TemplateSyntax {
        what: &'static str,
        line: usize,
        reason: String,
    },
    DataShape {
        row: Option<usize>,
        reason: String,
    },
    ReservedColumn {
        name: String,
    },
    UnknownPlaceholders {
        what: &'static str,
        names: Vec<String>,
        known: Vec<String>,
    },
    NoRows,
    InvalidFileName {
        row: usize,
        name: String,
        reason: &'static str,
    },
    DuplicateFileName {
        name: String,
        rows: Vec<usize>,
    },
    OutputExists {
        paths: Vec<PathBuf>,
    },
    RasterSize {
        width: u32,
        height: u32,
    },
    InvalidPng {
        reason: &'static str,
    },
    InkscapeFailed {
        program: PathBuf,
        status: ExitStatus,
        stderr: String,
    },
}

impl Error {
    /// Exit code for this failure. Command-line errors keep clap's own code (0 for `--help`).
    pub(crate) fn exit_code(&self) -> ExitCode {
        match self {
            Error::Cli(e) => u8::try_from(e.exit_code()).map_or(ExitCode::FAILURE, ExitCode::from),
            _ => ExitCode::FAILURE,
        }
    }
}

fn list<T: fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn paths(items: &[PathBuf]) -> String {
    items
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Toml(e) => write!(f, "invalid TOML: {e}"),
            Error::Cli(e) => write!(f, "{e}"),
            Error::Logging(e) => write!(f, "cannot start logging: {e}"),
            Error::Csv(e) => write!(f, "invalid CSV: {e}"),
            Error::Json(e) => write!(f, "invalid JSON: {e}"),
            Error::Svg(e) => write!(f, "invalid SVG: {e}"),
            Error::Pdf(e) => write!(f, "cannot convert SVG to PDF: {e}"),
            Error::Merge(e) => write!(f, "cannot merge PDF pages: {e}"),
            Error::ThreadPool(e) => write!(f, "cannot start worker threads: {e}"),
            Error::Png(e) => write!(f, "cannot encode PNG: {e}"),
            Error::ReadConfig { path, source } => {
                write!(
                    f,
                    "cannot read configuration file {}: {source}",
                    path.display()
                )
            }
            Error::ParseConfig { path, source } => {
                write!(
                    f,
                    "cannot parse configuration file {}: {source}",
                    path.display()
                )
            }
            Error::CreateDir { path, source } => {
                write!(f, "cannot create directory {}: {source}", path.display())
            }
            Error::WriteConfig { path, source } => {
                write!(
                    f,
                    "cannot write configuration file {}: {source}",
                    path.display()
                )
            }
            Error::ReadTemplate { path, source } => {
                write!(f, "cannot read template {}: {source}", path.display())
            }
            Error::ReadData { path, source } => {
                write!(f, "cannot read data {}: {source}", path.display())
            }
            Error::ParseData { path, source } => {
                write!(f, "cannot parse data {}: {source}", path.display())
            }
            Error::RunInkscape { program, source } => {
                write!(f, "cannot run {}: {source}", program.display())
            }
            Error::WriteOutput { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
            Error::Row { row, source } => write!(f, "row {row}: {source}"),
            Error::Invalid {
                setting,
                source,
                input,
                reason,
            } => {
                write!(
                    f,
                    "invalid value {input:?} for setting {setting} (from {source}): {reason}"
                )
            }
            Error::NonUtf8Path { setting, path } => {
                write!(
                    f,
                    "path of setting {setting} is not valid UTF-8: {}",
                    path.display()
                )
            }
            Error::TemplateSyntax { what, line, reason } => {
                write!(f, "invalid {what}, line {line}: {reason}")
            }
            Error::DataShape {
                row: Some(row),
                reason,
            } => write!(f, "row {row}: {reason}"),
            Error::DataShape { row: None, reason } => write!(f, "{reason}"),
            Error::ReservedColumn { name } => {
                write!(
                    f,
                    "column {name:?} is reserved: names starting with '_' are built-in"
                )
            }
            Error::UnknownPlaceholders { what, names, known } => write!(
                f,
                "{what} uses unknown placeholders: {}; known names are: {}",
                list(names),
                list(known)
            ),
            Error::NoRows => write!(f, "the data has no rows"),
            Error::InvalidFileName { row, name, reason } => {
                write!(f, "row {row}: invalid output file name {name:?}: {reason}")
            }
            Error::DuplicateFileName { name, rows } => {
                write!(
                    f,
                    "rows {} all produce the output file name {name:?}",
                    list(rows)
                )
            }
            Error::OutputExists { paths: existing } => write!(
                f,
                "output files already exist (use --force to replace them): {}",
                paths(existing)
            ),
            Error::RasterSize { width, height } => {
                write!(f, "cannot render an image of {width} x {height} pixels")
            }
            Error::InvalidPng { reason } => write!(f, "invalid PNG image: {reason}"),
            Error::InkscapeFailed {
                program,
                status,
                stderr,
            } => {
                write!(
                    f,
                    "{} failed ({status}): {}",
                    program.display(),
                    stderr.trim()
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Toml(e) => Some(e),
            Error::Cli(e) => Some(e),
            Error::Logging(e) => Some(e),
            Error::Csv(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Svg(e) => Some(e),
            // svg2pdf's ConversionError implements Display but not std::error::Error, so it
            // cannot be returned here; its message is part of this error's Display.
            Error::Pdf(_) => None,
            Error::Merge(e) => Some(e),
            Error::ThreadPool(e) => Some(e),
            Error::Png(e) => Some(e),
            Error::ReadConfig { source, .. }
            | Error::CreateDir { source, .. }
            | Error::WriteConfig { source, .. }
            | Error::ReadTemplate { source, .. }
            | Error::ReadData { source, .. }
            | Error::RunInkscape { source, .. }
            | Error::WriteOutput { source, .. } => Some(source),
            Error::ParseConfig { source, .. } => Some(source),
            Error::ParseData { source, .. } | Error::Row { source, .. } => Some(source.as_ref()),
            Error::Invalid { .. }
            | Error::NonUtf8Path { .. }
            | Error::TemplateSyntax { .. }
            | Error::DataShape { .. }
            | Error::ReservedColumn { .. }
            | Error::UnknownPlaceholders { .. }
            | Error::NoRows
            | Error::InvalidFileName { .. }
            | Error::DuplicateFileName { .. }
            | Error::OutputExists { .. }
            | Error::RasterSize { .. }
            | Error::InvalidPng { .. }
            | Error::InkscapeFailed { .. } => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Toml(e)
    }
}

impl From<clap::Error> for Error {
    fn from(e: clap::Error) -> Self {
        Error::Cli(e)
    }
}

impl From<tracing::subscriber::SetGlobalDefaultError> for Error {
    fn from(e: tracing::subscriber::SetGlobalDefaultError) -> Self {
        Error::Logging(e)
    }
}

impl From<csv::Error> for Error {
    fn from(e: csv::Error) -> Self {
        Error::Csv(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<svg2pdf::usvg::Error> for Error {
    fn from(e: svg2pdf::usvg::Error) -> Self {
        Error::Svg(e)
    }
}

impl From<svg2pdf::ConversionError> for Error {
    fn from(e: svg2pdf::ConversionError) -> Self {
        Error::Pdf(e)
    }
}

impl From<lopdf::Error> for Error {
    fn from(e: lopdf::Error) -> Self {
        Error::Merge(e)
    }
}

impl From<png::EncodingError> for Error {
    fn from(e: png::EncodingError) -> Self {
        Error::Png(e)
    }
}

impl From<rayon::ThreadPoolBuildError> for Error {
    fn from(e: rayon::ThreadPoolBuildError) -> Self {
        Error::ThreadPool(e)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::os::unix::process::ExitStatusExt;

    fn io() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied")
    }

    fn toml_error() -> toml::de::Error {
        toml::from_str::<toml::Table>("= broken").unwrap_err()
    }

    fn clap_error() -> clap::Error {
        clap::Error::new(clap::error::ErrorKind::InvalidValue)
    }

    fn logging_error() -> tracing::subscriber::SetGlobalDefaultError {
        // A second global default always fails; the first may have been set by another test.
        let _ = tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::new());
        tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::new())
            .unwrap_err()
    }

    fn csv_error() -> csv::Error {
        csv::ReaderBuilder::new()
            .from_reader("a,b\n1,2,3\n".as_bytes())
            .records()
            .find_map(|r| r.err())
            .unwrap()
    }

    fn json_error() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("[").unwrap_err()
    }

    fn svg_error() -> svg2pdf::usvg::Error {
        svg2pdf::usvg::Tree::from_str("not svg", &svg2pdf::usvg::Options::default()).unwrap_err()
    }

    fn lopdf_error() -> lopdf::Error {
        lopdf::Document::load_mem(b"not a pdf").unwrap_err()
    }

    fn pool_error() -> rayon::ThreadPoolBuildError {
        // Building the global pool a second time always fails.
        let _ = rayon::ThreadPoolBuilder::new().build_global();
        rayon::ThreadPoolBuilder::new().build_global().unwrap_err()
    }

    fn png_error() -> png::EncodingError {
        png::EncodingError::IoError(io())
    }

    fn all() -> Vec<(Error, bool)> {
        vec![
            (Error::Png(png_error()), true),
            (
                Error::RasterSize {
                    width: 0,
                    height: 0,
                },
                false,
            ),
            (
                Error::InvalidPng {
                    reason: "too short",
                },
                false,
            ),
            (Error::Io(io()), true),
            (Error::Toml(toml_error()), true),
            (Error::Cli(clap_error()), true),
            (Error::Logging(logging_error()), true),
            (Error::Csv(csv_error()), true),
            (Error::Json(json_error()), true),
            (Error::Svg(svg_error()), true),
            (Error::Pdf(svg2pdf::ConversionError::UnknownError), false),
            (Error::Merge(lopdf_error()), true),
            (Error::ThreadPool(pool_error()), true),
            (
                Error::ReadConfig {
                    path: "a.toml".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::ParseConfig {
                    path: "a.toml".into(),
                    source: toml_error(),
                },
                true,
            ),
            (
                Error::CreateDir {
                    path: "out".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::WriteConfig {
                    path: "a.toml".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::ReadTemplate {
                    path: "t.svg".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::ReadData {
                    path: "d.csv".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::ParseData {
                    path: "d.csv".into(),
                    source: Box::new(Error::NoRows),
                },
                true,
            ),
            (
                Error::RunInkscape {
                    program: "inkscape".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::WriteOutput {
                    path: "o.pdf".into(),
                    source: io(),
                },
                true,
            ),
            (
                Error::Row {
                    row: 3,
                    source: Box::new(Error::NoRows),
                },
                true,
            ),
            (
                Error::Invalid {
                    setting: "data.format",
                    source: Source::CommandLine,
                    input: "xml".into(),
                    reason: "expected one of auto, csv, json".into(),
                },
                false,
            ),
            (
                Error::NonUtf8Path {
                    setting: "data.path",
                    path: "x".into(),
                },
                false,
            ),
            (
                Error::TemplateSyntax {
                    what: "template",
                    line: 2,
                    reason: "unclosed".into(),
                },
                false,
            ),
            (
                Error::DataShape {
                    row: Some(1),
                    reason: "nested".into(),
                },
                false,
            ),
            (
                Error::DataShape {
                    row: None,
                    reason: "not an array".into(),
                },
                false,
            ),
            (
                Error::ReservedColumn {
                    name: "_row".into(),
                },
                false,
            ),
            (
                Error::UnknownPlaceholders {
                    what: "template",
                    names: vec!["x".into()],
                    known: vec!["name".into()],
                },
                false,
            ),
            (Error::NoRows, false),
            (
                Error::InvalidFileName {
                    row: 1,
                    name: "a/b".into(),
                    reason: "contains '/'",
                },
                false,
            ),
            (
                Error::DuplicateFileName {
                    name: "a.pdf".into(),
                    rows: vec![1, 2],
                },
                false,
            ),
            (
                Error::OutputExists {
                    paths: vec!["a.pdf".into()],
                },
                false,
            ),
            (
                Error::InkscapeFailed {
                    program: "inkscape".into(),
                    status: ExitStatus::from_raw(256),
                    stderr: "boom".into(),
                },
                false,
            ),
        ]
    }

    #[test]
    fn every_variant_has_a_message() {
        all()
            .iter()
            .for_each(|(e, _)| assert!(!e.to_string().is_empty(), "{e:?}"));
    }

    #[test]
    fn wrapping_variants_expose_their_source() {
        all()
            .iter()
            .for_each(|(e, wraps)| assert_eq!(e.source().is_some(), *wraps, "{e:?}"));
    }

    #[test]
    fn from_conversions_pick_the_matching_variant() {
        assert!(matches!(Error::from(io()), Error::Io(_)));
        assert!(matches!(Error::from(toml_error()), Error::Toml(_)));
        assert!(matches!(Error::from(clap_error()), Error::Cli(_)));
        assert!(matches!(Error::from(logging_error()), Error::Logging(_)));
        assert!(matches!(Error::from(csv_error()), Error::Csv(_)));
        assert!(matches!(Error::from(json_error()), Error::Json(_)));
        assert!(matches!(Error::from(svg_error()), Error::Svg(_)));
        assert!(matches!(
            Error::from(svg2pdf::ConversionError::UnknownError),
            Error::Pdf(_)
        ));
        assert!(matches!(Error::from(lopdf_error()), Error::Merge(_)));
        assert!(matches!(Error::from(pool_error()), Error::ThreadPool(_)));
        assert!(matches!(Error::from(png_error()), Error::Png(_)));
    }

    #[test]
    fn invalid_names_setting_and_source() {
        let e = Error::Invalid {
            setting: "log.level",
            source: Source::Environment,
            input: "warn".into(),
            reason: "unknown level".into(),
        };
        let text = e.to_string();
        assert!(
            text.contains("log.level") && text.contains("environment") && text.contains("\"warn\"")
        );
    }

    #[test]
    fn row_context_names_the_row() {
        let e = Error::Row {
            row: 7,
            source: Box::new(Error::NoRows),
        };
        assert_eq!(e.to_string(), "row 7: the data has no rows");
    }

    #[test]
    fn exit_code_follows_clap_for_help() {
        let help = clap::Error::new(clap::error::ErrorKind::DisplayHelp);
        assert_eq!(Error::Cli(help).exit_code(), ExitCode::SUCCESS);
        assert_eq!(Error::Io(io()).exit_code(), ExitCode::FAILURE);
    }
}
