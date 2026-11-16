//! Command line and environment variables, parsed by clap (doc/rust-guidelines.md §3.3, §3.4).

use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::parser::ValueSource;
use clap::{ArgAction, CommandFactory, Parser};

use super::{
    Partial, PartialData, PartialLog, PartialOutput, PartialRender, PartialTemplate, defaults,
};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default());

const H_CONFIG: &str = "⚙️  Configuration";
const H_TEMPLATE: &str = "🖼️  Template";
const H_DATA: &str = "📊 Data";
const H_OUTPUT: &str = "📄 Output";
const H_RENDER: &str = "🎨 Rendering";
const H_LOG: &str = "📝 Logging";

const EXAMPLES: &str = "\
📖 Examples:
  One PDF per CSV row, named after the \"id\" column:
    multi-svg-to-pdf -t badge.svg -d people.csv -o out --output-name '{{id}}.pdf'

  Semicolon-separated data from standard input, all rows in one PDF:
    cat people.csv | multi-svg-to-pdf -t badge.svg -d - --csv-delimiter ';' --merge badges.pdf

  Write every current setting to multi-svg-to-pdf.toml and exit:
    multi-svg-to-pdf --dump-config";

/// 🧾 Render one PDF per data row from an SVG template.
///
/// multi-svg-to-pdf reads an SVG template containing placeholders such as {{name}}, and a list of rows
/// from a CSV or JSON file. For each row it replaces every placeholder with the value of the
/// column of the same name, and renders the resulting SVG to a PDF file.
///
/// Every setting can be given on the command line, in an environment variable, or in the TOML
/// configuration file, in that order of precedence. See doc/ in the source tree for details.
#[derive(Debug, Parser)]
#[command(name = "multi-svg-to-pdf", version, styles = STYLES, after_long_help = EXAMPLES)]
pub(crate) struct Cli {
    /// Path of the TOML configuration file.
    ///
    /// Settings on the command line and in environment variables override the ones in this
    /// file. A missing file is not an error; a file that cannot be read or parsed is.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_CONFIG", default_value = defaults::CONFIG_PATH, help_heading = H_CONFIG)]
    pub(crate) config: PathBuf,

    /// Write the resolved configuration to the configuration file, then run.
    ///
    /// Every setting is written with the value the program uses, defaults included. An existing
    /// file is replaced.
    #[arg(long, conflicts_with = "dump_config", help_heading = H_CONFIG)]
    pub(crate) save_config: bool,

    /// Write the resolved configuration to the configuration file, then exit.
    ///
    /// Like --save-config, but nothing is rendered. Use it to create a starting configuration
    /// file.
    #[arg(long, help_heading = H_CONFIG)]
    pub(crate) dump_config: bool,

    /// Path of the SVG template.
    ///
    /// The template is an ordinary SVG file whose text contains placeholders, which are
    /// replaced with the values of each data row before rendering.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_TEMPLATE_PATH", default_value = defaults::TEMPLATE_PATH, help_heading = H_TEMPLATE)]
    pub(crate) template: PathBuf,

    /// Text that opens a placeholder.
    ///
    /// Change it, together with --placeholder-close, when the template already contains this
    /// text for another purpose. Must not be empty.
    #[arg(long, env = "MULTI_SVG_TO_PDF_TEMPLATE_PLACEHOLDER_OPEN", default_value = defaults::PLACEHOLDER_OPEN, help_heading = H_TEMPLATE)]
    pub(crate) placeholder_open: String,

    /// Text that closes a placeholder.
    ///
    /// See --placeholder-open. Must not be empty.
    #[arg(long, env = "MULTI_SVG_TO_PDF_TEMPLATE_PLACEHOLDER_CLOSE", default_value = defaults::PLACEHOLDER_CLOSE, help_heading = H_TEMPLATE)]
    pub(crate) placeholder_close: String,

    /// Path of the data file, or - for standard input.
    ///
    /// Each row produces one PDF. Column names come from the CSV header row or from the keys
    /// of the JSON objects.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_DATA_PATH", default_value = defaults::DATA_PATH, help_heading = H_DATA)]
    pub(crate) data: PathBuf,

    /// Format of the data file: auto, csv or json.
    ///
    /// With auto, a path ending in .json is read as JSON (an array of flat objects) and
    /// anything else, standard input included, as CSV.
    #[arg(long, env = "MULTI_SVG_TO_PDF_DATA_FORMAT", default_value = defaults::DATA_FORMAT, help_heading = H_DATA)]
    pub(crate) data_format: String,

    /// Field delimiter of CSV data: one ASCII character.
    ///
    /// Use "tab" or "\t" for tab-separated data. Ignored for JSON.
    #[arg(long, env = "MULTI_SVG_TO_PDF_DATA_CSV_DELIMITER", default_value = defaults::CSV_DELIMITER, help_heading = H_DATA)]
    pub(crate) csv_delimiter: String,

    /// Directory where the PDF files are written.
    ///
    /// Created when missing. Ignored with --merge.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_OUTPUT_DIR", default_value = defaults::OUTPUT_DIR, help_heading = H_OUTPUT)]
    pub(crate) output_dir: PathBuf,

    /// File name pattern of each PDF, with placeholders.
    ///
    /// Uses the same placeholders as the template, plus _row for the 1-based row number.
    /// Default: {{_row}}.pdf, written with the configured placeholder delimiters.
    #[arg(long, env = "MULTI_SVG_TO_PDF_OUTPUT_NAME", help_heading = H_OUTPUT)]
    pub(crate) output_name: Option<String>,

    /// Write all rows as pages of this one PDF file instead of one file per row.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_OUTPUT_MERGE", help_heading = H_OUTPUT)]
    pub(crate) merge: Option<PathBuf>,

    /// Replace output files that already exist.
    ///
    /// Without it, the program stops before rendering anything if any output file exists.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_OUTPUT_FORCE", action = ArgAction::SetTrue, help_heading = H_OUTPUT)]
    pub(crate) force: bool,

    /// Number of rows rendered in parallel; 0 uses one per CPU core.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_RENDER_JOBS", default_value_t = defaults::JOBS, help_heading = H_RENDER)]
    pub(crate) jobs: usize,

    /// Extra directory to load fonts from; may be repeated.
    ///
    /// System fonts are always loaded. In the environment variable, separate directories
    /// with ':'.
    #[arg(long = "font-dir", env = "MULTI_SVG_TO_PDF_RENDER_FONT_DIRS", value_delimiter = ':', help_heading = H_RENDER)]
    pub(crate) font_dirs: Vec<PathBuf>,

    /// Program that converts SVG to PDF: native or inkscape.
    ///
    /// native is built in and needs nothing installed, but supports plain SVG 1.1 only: Inkscape
    /// flowed text (text in a shape, text-align) is placed wrongly. inkscape runs the Inkscape
    /// program and renders Inkscape templates faithfully; --font-dir does not apply to it.
    #[arg(short, long, env = "MULTI_SVG_TO_PDF_RENDER_ENGINE", default_value = defaults::ENGINE, help_heading = H_RENDER)]
    pub(crate) engine: String,

    /// Inkscape program used by the inkscape engine.
    ///
    /// A bare name is looked up on PATH.
    #[arg(long, env = "MULTI_SVG_TO_PDF_RENDER_INKSCAPE", default_value = defaults::INKSCAPE, help_heading = H_RENDER)]
    pub(crate) inkscape: PathBuf,

    /// Content of the PDF: vector or raster.
    ///
    /// vector keeps shapes and text as they are: sharp at any zoom, small files, selectable
    /// text. raster renders each card to one image at --dpi and puts that image in the PDF,
    /// as an export to PNG followed by a conversion to PDF would.
    #[arg(long, env = "MULTI_SVG_TO_PDF_RENDER_FORMAT", default_value = defaults::FORMAT, help_heading = H_RENDER)]
    pub(crate) format: String,

    /// Resolution of raster output, in dots per inch.
    ///
    /// Only used with --format raster. The page keeps the template's size; the image has
    /// size × dpi pixels. From 1 to 2400.
    #[arg(long, env = "MULTI_SVG_TO_PDF_RENDER_DPI", default_value_t = defaults::DPI, help_heading = H_RENDER)]
    pub(crate) dpi: u32,

    /// Log level: off, error, warning, info, debug or trace.
    ///
    /// A level shows its own messages and every more severe one. Any letter case is accepted.
    #[arg(long, env = "MULTI_SVG_TO_PDF_LOG_LEVEL", default_value = defaults::LOG_LEVEL, help_heading = H_LOG)]
    pub(crate) log_level: String,
}

/// The clap command used to parse the real command line and environment.
pub(crate) fn command() -> clap::Command {
    Cli::command()
}

/// First line of an option's help, used as the comment of its key in the configuration file.
pub(crate) fn short_help(id: &str) -> String {
    Cli::command()
        .get_arguments()
        .find(|arg| arg.get_id() == id)
        .and_then(|arg| arg.get_help())
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Keeps only the values whose source is `want` (the command line or the environment).
pub(crate) fn layer(
    cli: &Cli,
    want: ValueSource,
    source_of: &impl Fn(&str) -> Option<ValueSource>,
) -> Partial {
    let take = |id: &str| source_of(id) == Some(want);
    let pick = |id: &str, value: &String| take(id).then(|| value.clone());
    let pick_path = |id: &str, value: &PathBuf| take(id).then(|| value.clone());
    Partial {
        template: PartialTemplate {
            path: pick_path("template", &cli.template),
            placeholder_open: pick("placeholder_open", &cli.placeholder_open),
            placeholder_close: pick("placeholder_close", &cli.placeholder_close),
        },
        data: PartialData {
            path: pick_path("data", &cli.data),
            format: pick("data_format", &cli.data_format),
            csv_delimiter: pick("csv_delimiter", &cli.csv_delimiter),
        },
        output: PartialOutput {
            dir: pick_path("output_dir", &cli.output_dir),
            name: cli.output_name.clone().filter(|_| take("output_name")),
            merge: cli.merge.clone().filter(|_| take("merge")),
            force: take("force").then_some(cli.force),
        },
        render: PartialRender {
            jobs: take("jobs").then_some(cli.jobs),
            font_dirs: take("font_dirs").then(|| cli.font_dirs.clone()),
            engine: pick("engine", &cli.engine),
            inkscape: pick_path("inkscape", &cli.inkscape),
            format: pick("format", &cli.format),
            dpi: take("dpi").then_some(cli.dpi),
        },
        log: PartialLog {
            level: pick("log_level", &cli.log_level),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::FromArgMatches;

    /// The command with every `env` attribute removed, so tests never read the real environment.
    pub(crate) fn command_without_env() -> clap::Command {
        Cli::command().mut_args(|arg| arg.env(None::<&str>))
    }

    fn parse(args: &[&str]) -> (Cli, clap::ArgMatches) {
        let matches = command_without_env().try_get_matches_from(args).unwrap();
        (Cli::from_arg_matches(&matches).unwrap(), matches)
    }

    #[test]
    fn command_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_gives_empty_layers() {
        let (cli, matches) = parse(&["multi-svg-to-pdf"]);
        let source_of = |id: &str| matches.value_source(id);
        assert_eq!(
            layer(&cli, ValueSource::CommandLine, &source_of),
            Partial::default()
        );
        assert_eq!(
            layer(&cli, ValueSource::EnvVariable, &source_of),
            Partial::default()
        );
    }

    #[test]
    fn command_line_values_go_to_the_command_line_layer() {
        let (cli, matches) = parse(&[
            "multi-svg-to-pdf",
            "-t",
            "t.svg",
            "--data=d.json",
            "--csv-delimiter",
            ";",
            "-f",
            "-j",
            "4",
            "--font-dir",
            "a",
            "--font-dir",
            "b",
            "--merge",
            "all.pdf",
            "--log-level",
            "debug",
        ]);
        let source_of = |id: &str| matches.value_source(id);
        let partial = layer(&cli, ValueSource::CommandLine, &source_of);
        assert_eq!(partial.template.path, Some("t.svg".into()));
        assert_eq!(partial.data.path, Some("d.json".into()));
        assert_eq!(partial.data.csv_delimiter.as_deref(), Some(";"));
        assert_eq!(partial.output.force, Some(true));
        assert_eq!(partial.output.merge, Some("all.pdf".into()));
        assert_eq!(partial.render.jobs, Some(4));
        assert_eq!(partial.render.font_dirs, Some(vec!["a".into(), "b".into()]));
        assert_eq!(partial.log.level.as_deref(), Some("debug"));
        assert_eq!(partial.output.dir, None);
        assert_eq!(
            layer(&cli, ValueSource::EnvVariable, &source_of),
            Partial::default()
        );
    }

    #[test]
    fn environment_values_go_to_the_environment_layer() {
        let (cli, _) = parse(&[
            "multi-svg-to-pdf",
            "--output-dir",
            "out",
            "--log-level",
            "trace",
        ]);
        // Pretend the output directory came from the environment.
        let source_of = |id: &str| match id {
            "output_dir" => Some(ValueSource::EnvVariable),
            "log_level" => Some(ValueSource::CommandLine),
            _ => Some(ValueSource::DefaultValue),
        };
        let env = layer(&cli, ValueSource::EnvVariable, &source_of);
        assert_eq!(env.output.dir, Some("out".into()));
        assert_eq!(env.log.level, None);
    }

    #[test]
    fn save_and_dump_exclude_each_other() {
        assert!(
            command_without_env()
                .try_get_matches_from(["multi-svg-to-pdf", "--save-config", "--dump-config"])
                .is_err()
        );
    }

    #[test]
    fn short_help_is_the_first_doc_line() {
        assert_eq!(short_help("template"), "Path of the SVG template");
        assert_eq!(short_help("no_such_option"), "");
    }
}
