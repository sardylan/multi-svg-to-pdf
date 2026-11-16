//! Logging setup (doc/rust-guidelines.md §3.6).

use std::io::IsTerminal;

use time::format_description::BorrowedFormatItem;
use time::macros::format_description;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::time::UtcTime;

use crate::errors::Result;

/// UTC, RFC 3339, exactly six fraction digits, `Z` suffix.
const TIMESTAMP_FORMAT: &[BorrowedFormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z");

/// The canonical log level names accepted by the `log.level` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogLevel {
    Off,
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub(crate) const NAMES: &'static str = "off, error, warning, info, debug, trace";

    /// Parses a canonical level name in any letter case. Other spellings give `None`.
    pub(crate) fn parse(input: &str) -> Option<LogLevel> {
        match input.to_ascii_lowercase().as_str() {
            "off" => Some(LogLevel::Off),
            "error" => Some(LogLevel::Error),
            "warning" => Some(LogLevel::Warning),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warning => "warning",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }

    fn filter(self) -> LevelFilter {
        match self {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warning => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

/// Installs the global subscriber, writing to standard error. `RUST_LOG` is not read.
pub(crate) fn start_logging(level: LogLevel) -> Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_timer(UtcTime::new(TIMESTAMP_FORMAT))
        .with_max_level(level.filter())
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .finish();
    Ok(tracing::subscriber::set_global_default(subscriber)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn timestamp_is_utc_rfc3339_with_microseconds() {
        let at = datetime!(2026-11-13 10:15:03.123456789 UTC);
        assert_eq!(
            at.format(TIMESTAMP_FORMAT).unwrap(),
            "2026-11-13T10:15:03.123456Z"
        );
    }

    #[test]
    fn parses_canonical_names_in_any_case() {
        assert_eq!(LogLevel::parse("WARNING"), Some(LogLevel::Warning));
        assert_eq!(LogLevel::parse("Info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::parse("off"), Some(LogLevel::Off));
    }

    #[test]
    fn refuses_other_spellings() {
        ["warn", "3", "", "info,foo=debug", " info"]
            .iter()
            .for_each(|input| assert_eq!(LogLevel::parse(input), None, "{input:?}"));
    }

    #[test]
    fn names_round_trip() {
        [
            LogLevel::Off,
            LogLevel::Error,
            LogLevel::Warning,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ]
        .iter()
        .for_each(|level| assert_eq!(LogLevel::parse(level.as_str()), Some(*level)));
    }

    #[test]
    fn warning_maps_to_warn_filter() {
        assert_eq!(LogLevel::Warning.filter(), LevelFilter::WARN);
        assert_eq!(LogLevel::Off.filter(), LevelFilter::OFF);
    }
}
