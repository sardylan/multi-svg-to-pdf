//! Configuration: command line, environment, TOML file and defaults, resolved once at startup
//! into an immutable [`Config`] (doc/rust-guidelines.md §3).
//!
//! - `cli.rs`: the clap parser and the split of its values into a command-line and an
//!   environment layer.
//! - `file.rs`: reading and parsing the TOML file.
//! - `resolve.rs`: the pure merge of the layers and the validation of each value.
//! - `write.rs`: rendering and writing the file for `--save-config` / `--dump-config`.

mod cli;
mod file;
mod resolve;
mod write;

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use clap::parser::ValueSource;
use clap::{Command, FromArgMatches};
use serde::Deserialize;

use crate::errors::Result;
use crate::logging::LogLevel;

pub(crate) use cli::command;
pub(crate) use resolve::resolve;

/// Built-in default values. The clap parser shows the same constants in `--help`.
pub(crate) mod defaults {
    pub(crate) const CONFIG_PATH: &str = "multi-svg-to-pdf.toml";
    pub(crate) const TEMPLATE_PATH: &str = "template.svg";
    pub(crate) const PLACEHOLDER_OPEN: &str = "{{";
    pub(crate) const PLACEHOLDER_CLOSE: &str = "}}";
    pub(crate) const DATA_PATH: &str = "data.csv";
    pub(crate) const DATA_FORMAT: &str = "auto";
    pub(crate) const CSV_DELIMITER: &str = ",";
    pub(crate) const OUTPUT_DIR: &str = "output";
    /// Built-in variable holding the 1-based row number; the default output name uses it.
    pub(crate) const ROW_VARIABLE: &str = "_row";
    pub(crate) const JOBS: usize = 0;
    pub(crate) const ENGINE: &str = "native";
    pub(crate) const INKSCAPE: &str = "inkscape";
    pub(crate) const FORMAT: &str = "vector";
    pub(crate) const DPI: u32 = 300;
    pub(crate) const LOG_LEVEL: &str = "info";
}

/// Where a setting's value came from, for error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    CommandLine,
    Environment,
    File(PathBuf),
    Default,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::CommandLine => write!(f, "the command line"),
            Source::Environment => write!(f, "the environment"),
            Source::File(path) => write!(f, "configuration file {}", path.display()),
            Source::Default => write!(f, "the built-in default"),
        }
    }
}

/// How the data file is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataFormat {
    /// Chosen from the file extension (`.json` is JSON, anything else and stdin are CSV).
    Auto,
    Csv,
    Json,
}

impl DataFormat {
    pub(crate) const NAMES: &'static str = "auto, csv, json";

    pub(crate) fn parse(input: &str) -> Option<DataFormat> {
        match input.to_ascii_lowercase().as_str() {
            "auto" => Some(DataFormat::Auto),
            "csv" => Some(DataFormat::Csv),
            "json" => Some(DataFormat::Json),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DataFormat::Auto => "auto",
            DataFormat::Csv => "csv",
            DataFormat::Json => "json",
        }
    }
}

/// Which program turns SVG into PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Engine {
    /// Built in (the usvg and svg2pdf crates). Plain SVG 1.1; no Inkscape flowed text.
    Native,
    /// The external `inkscape` program. Faithful for templates made with Inkscape.
    Inkscape,
}

impl Engine {
    pub(crate) const NAMES: &'static str = "native, inkscape";

    pub(crate) fn parse(input: &str) -> Option<Engine> {
        match input.to_ascii_lowercase().as_str() {
            "native" => Some(Engine::Native),
            "inkscape" => Some(Engine::Inkscape),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Engine::Native => "native",
            Engine::Inkscape => "inkscape",
        }
    }
}

/// What the PDF contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderFormat {
    /// Vector shapes and real text.
    Vector,
    /// One image rendered at `render.dpi`.
    Raster,
}

impl RenderFormat {
    pub(crate) const NAMES: &'static str = "vector, raster";

    pub(crate) fn parse(input: &str) -> Option<RenderFormat> {
        match input.to_ascii_lowercase().as_str() {
            "vector" => Some(RenderFormat::Vector),
            "raster" => Some(RenderFormat::Raster),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RenderFormat::Vector => "vector",
            RenderFormat::Raster => "raster",
        }
    }
}

/// The resolved configuration. Built once, never changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) config_path: PathBuf,
    pub(crate) template: TemplateConfig,
    pub(crate) data: DataConfig,
    pub(crate) output: OutputConfig,
    pub(crate) render: RenderConfig,
    pub(crate) log: LogConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TemplateConfig {
    pub(crate) path: PathBuf,
    pub(crate) placeholder_open: String,
    pub(crate) placeholder_close: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataConfig {
    /// `-` means standard input.
    pub(crate) path: PathBuf,
    pub(crate) format: DataFormat,
    pub(crate) csv_delimiter: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputConfig {
    pub(crate) dir: PathBuf,
    /// File name pattern, with the same placeholders as the template.
    pub(crate) name: String,
    /// When set, all rows go into this one multi-page PDF instead of one file per row.
    pub(crate) merge: Option<PathBuf>,
    pub(crate) force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderConfig {
    /// Worker threads; 0 means one per CPU core.
    pub(crate) jobs: usize,
    pub(crate) font_dirs: Vec<PathBuf>,
    pub(crate) engine: Engine,
    /// The Inkscape program, found on `PATH` when it is a bare name.
    pub(crate) inkscape: PathBuf,
    pub(crate) format: RenderFormat,
    /// Resolution of raster output, in dots per inch.
    pub(crate) dpi: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogConfig {
    pub(crate) level: LogLevel,
}

/// One source's values. Its shape is the TOML file's shape; the command line and the
/// environment are converted into it too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Partial {
    pub(crate) template: PartialTemplate,
    pub(crate) data: PartialData,
    pub(crate) output: PartialOutput,
    pub(crate) render: PartialRender,
    pub(crate) log: PartialLog,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PartialTemplate {
    pub(crate) path: Option<PathBuf>,
    pub(crate) placeholder_open: Option<String>,
    pub(crate) placeholder_close: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PartialData {
    pub(crate) path: Option<PathBuf>,
    pub(crate) format: Option<String>,
    pub(crate) csv_delimiter: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PartialOutput {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) name: Option<String>,
    pub(crate) merge: Option<PathBuf>,
    pub(crate) force: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PartialRender {
    pub(crate) jobs: Option<usize>,
    pub(crate) font_dirs: Option<Vec<PathBuf>>,
    pub(crate) engine: Option<String>,
    pub(crate) inkscape: Option<PathBuf>,
    pub(crate) format: Option<String>,
    pub(crate) dpi: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PartialLog {
    pub(crate) level: Option<String>,
}

/// A source together with its values, in order of precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layer {
    pub(crate) source: Source,
    pub(crate) values: Partial,
}

/// What the program was asked to do besides (or instead of) rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Run,
    SaveConfig,
    DumpConfig,
}

impl Action {
    fn from_flags(save_config: bool, dump_config: bool) -> Action {
        match (save_config, dump_config) {
            (_, true) => Action::DumpConfig,
            (true, false) => Action::SaveConfig,
            (false, false) => Action::Run,
        }
    }
}

/// The outcome of [`load_config`]: the configuration, the action, and messages to log once
/// logging has started.
#[derive(Debug)]
pub(crate) struct Loaded {
    pub(crate) config: Config,
    pub(crate) action: Action,
    pub(crate) notes: Vec<String>,
}

/// Parses `args` (and, through `command`, the environment), reads the configuration file, and
/// resolves them. Pass [`command()`] for the real program.
pub(crate) fn load_config<I, T>(command: Command, args: I) -> Result<Loaded>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = command.try_get_matches_from(args)?;
    let parsed = cli::Cli::from_arg_matches(&matches)?;
    let source_of = |id: &str| matches.value_source(id);
    let config_path = parsed.config.clone();

    let file_layer = file::read_config_file(&config_path)?
        .map(|text| file::parse_config_file(&text, &config_path))
        .transpose()?
        .map(|values| Layer {
            source: Source::File(config_path.clone()),
            values,
        });

    let notes = vec![match &file_layer {
        Some(_) => format!("configuration read from {}", config_path.display()),
        None => format!(
            "no configuration file at {}, using defaults",
            config_path.display()
        ),
    }];

    let layers: Vec<Layer> = [
        Some(Layer {
            source: Source::CommandLine,
            values: cli::layer(&parsed, ValueSource::CommandLine, &source_of),
        }),
        Some(Layer {
            source: Source::Environment,
            values: cli::layer(&parsed, ValueSource::EnvVariable, &source_of),
        }),
        file_layer,
    ]
    .into_iter()
    .flatten()
    .collect();

    Ok(Loaded {
        config: resolve(config_path, &layers)?,
        action: Action::from_flags(parsed.save_config, parsed.dump_config),
        notes,
    })
}

/// Writes the resolved configuration to its configuration file path.
pub(crate) fn save_config(config: &Config) -> Result<()> {
    let text = write::render_toml(config, &cli::short_help)?;
    write::write_config_file(&config.config_path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_from_flags() {
        assert_eq!(Action::from_flags(false, false), Action::Run);
        assert_eq!(Action::from_flags(true, false), Action::SaveConfig);
        assert_eq!(Action::from_flags(false, true), Action::DumpConfig);
    }

    #[test]
    fn engine_names_round_trip() {
        [Engine::Native, Engine::Inkscape]
            .iter()
            .for_each(|e| assert_eq!(Engine::parse(e.as_str()), Some(*e)));
        assert_eq!(Engine::parse("Inkscape"), Some(Engine::Inkscape));
        assert_eq!(Engine::parse("cairo"), None);
    }

    #[test]
    fn render_format_names_round_trip() {
        [RenderFormat::Vector, RenderFormat::Raster]
            .iter()
            .for_each(|f| assert_eq!(RenderFormat::parse(f.as_str()), Some(*f)));
        assert_eq!(RenderFormat::parse("png"), None);
    }

    #[test]
    fn data_format_names_round_trip() {
        [DataFormat::Auto, DataFormat::Csv, DataFormat::Json]
            .iter()
            .for_each(|f| assert_eq!(DataFormat::parse(f.as_str()), Some(*f)));
        assert_eq!(DataFormat::parse("JSON"), Some(DataFormat::Json));
        assert_eq!(DataFormat::parse("xml"), None);
    }

    #[test]
    fn source_display_is_not_empty() {
        [
            Source::CommandLine,
            Source::Environment,
            Source::File("a.toml".into()),
            Source::Default,
        ]
        .iter()
        .for_each(|s| assert!(!s.to_string().is_empty()));
    }

    #[test]
    fn load_reports_missing_file_and_uses_defaults() {
        let missing = std::env::temp_dir().join(format!(
            "multi-svg-to-pdf-missing-{}.toml",
            std::process::id()
        ));
        let loaded = load_config(
            cli::tests::command_without_env(),
            ["multi-svg-to-pdf", "--config", missing.to_str().unwrap()],
        )
        .unwrap();
        assert_eq!(loaded.action, Action::Run);
        assert_eq!(loaded.config.config_path, missing);
        assert!(loaded.notes[0].contains("no configuration file"));
    }

    #[test]
    fn load_rejects_conflicting_actions() {
        let e = load_config(
            cli::tests::command_without_env(),
            ["multi-svg-to-pdf", "--save-config", "--dump-config"],
        )
        .unwrap_err();
        assert!(matches!(e, crate::errors::Error::Cli(_)));
    }
}
