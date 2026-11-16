//! Pure merge of the configuration layers, with validation of every value (§3).

use std::path::{Path, PathBuf};

use super::{
    Config, DataConfig, DataFormat, Engine, Layer, LogConfig, OutputConfig, Partial, RenderConfig,
    RenderFormat, Source, TemplateConfig, defaults,
};
use crate::errors::{Error, Result};
use crate::logging::LogLevel;

/// Upper bound of `render.jobs`, which also keeps the value representable in TOML.
const MAX_JOBS: usize = 1024;

/// The setting and source a value came from, used to report it when it is invalid.
#[derive(Debug, Clone, Copy)]
struct Origin<'a> {
    setting: &'static str,
    source: &'a Source,
}

impl Origin<'_> {
    fn invalid(self, input: impl Into<String>, reason: impl Into<String>) -> Error {
        Error::Invalid {
            setting: self.setting,
            source: self.source.clone(),
            input: input.into(),
            reason: reason.into(),
        }
    }
}

/// The value of the first layer that sets it, checked by `parse`; `None` when no layer does.
fn setting<'a, T: ?Sized + 'a, U>(
    layers: &'a [Layer],
    setting: &'static str,
    get: impl Fn(&'a Partial) -> Option<&'a T>,
    parse: impl Fn(&'a T, Origin<'a>) -> Result<U>,
) -> Result<Option<U>> {
    layers
        .iter()
        .find_map(|layer| get(&layer.values).map(|value| (&layer.source, value)))
        .map(|(source, value)| parse(value, Origin { setting, source }))
        .transpose()
}

fn any<T: Clone>(value: &T, _: Origin<'_>) -> Result<T> {
    Ok(value.clone())
}

fn non_empty_text(value: &str, origin: Origin<'_>) -> Result<String> {
    match value.is_empty() {
        true => Err(origin.invalid(value, "must not be empty")),
        false => Ok(value.to_owned()),
    }
}

fn non_empty_path(value: &Path, origin: Origin<'_>) -> Result<PathBuf> {
    match value.as_os_str().is_empty() {
        true => Err(origin.invalid("", "must not be empty")),
        false => Ok(value.to_owned()),
    }
}

fn data_format(value: &str, origin: Origin<'_>) -> Result<DataFormat> {
    DataFormat::parse(value)
        .ok_or_else(|| origin.invalid(value, format!("expected one of {}", DataFormat::NAMES)))
}

fn csv_delimiter(value: &str, origin: Origin<'_>) -> Result<u8> {
    let mut chars = value.chars();
    match (value, chars.next(), chars.next()) {
        ("tab" | "\\t", _, _) => Ok(b'\t'),
        (_, Some(c), None) if c.is_ascii() => Ok(c as u8),
        _ => Err(origin.invalid(value, "expected one ASCII character, \"tab\" or \"\\t\"")),
    }
}

fn font_dirs(value: &[PathBuf], origin: Origin<'_>) -> Result<Vec<PathBuf>> {
    match value.iter().any(|dir| dir.as_os_str().is_empty()) {
        true => Err(origin.invalid(format!("{value:?}"), "directories must not be empty")),
        false => Ok(value.to_vec()),
    }
}

fn engine(value: &str, origin: Origin<'_>) -> Result<Engine> {
    Engine::parse(value)
        .ok_or_else(|| origin.invalid(value, format!("expected one of {}", Engine::NAMES)))
}

fn render_format(value: &str, origin: Origin<'_>) -> Result<RenderFormat> {
    RenderFormat::parse(value)
        .ok_or_else(|| origin.invalid(value, format!("expected one of {}", RenderFormat::NAMES)))
}

/// Range of `render.dpi`.
const DPI_RANGE: std::ops::RangeInclusive<u32> = 1..=2400;

fn dpi(value: &u32, origin: Origin<'_>) -> Result<u32> {
    match DPI_RANGE.contains(value) {
        true => Ok(*value),
        false => Err(origin.invalid(value.to_string(), "must be from 1 to 2400")),
    }
}

fn jobs(value: &usize, origin: Origin<'_>) -> Result<usize> {
    match *value <= MAX_JOBS {
        true => Ok(*value),
        false => Err(origin.invalid(value.to_string(), format!("must be at most {MAX_JOBS}"))),
    }
}

fn log_level(value: &str, origin: Origin<'_>) -> Result<LogLevel> {
    LogLevel::parse(value)
        .ok_or_else(|| origin.invalid(value, format!("expected one of {}", LogLevel::NAMES)))
}

/// The default output name: the row variable between the configured delimiters.
pub(crate) fn default_output_name(open: &str, close: &str) -> String {
    format!("{open}{}{close}.pdf", defaults::ROW_VARIABLE)
}

/// Parses a built-in default with the same check as a configured value.
fn parse_default<U>(
    parse: fn(&str, Origin<'_>) -> Result<U>,
    setting: &'static str,
    value: &str,
) -> Result<U> {
    parse(
        value,
        Origin {
            setting,
            source: &Source::Default,
        },
    )
}

/// Merges `layers` (highest precedence first) over the built-in defaults.
pub(crate) fn resolve(config_path: PathBuf, layers: &[Layer]) -> Result<Config> {
    let placeholder_open = setting(
        layers,
        "template.placeholder_open",
        |p| p.template.placeholder_open.as_deref(),
        non_empty_text,
    )?
    .unwrap_or_else(|| defaults::PLACEHOLDER_OPEN.to_owned());
    let placeholder_close = setting(
        layers,
        "template.placeholder_close",
        |p| p.template.placeholder_close.as_deref(),
        non_empty_text,
    )?
    .unwrap_or_else(|| defaults::PLACEHOLDER_CLOSE.to_owned());

    let template = TemplateConfig {
        path: setting(
            layers,
            "template.path",
            |p| p.template.path.as_deref(),
            non_empty_path,
        )?
        .unwrap_or_else(|| defaults::TEMPLATE_PATH.into()),
        placeholder_open,
        placeholder_close,
    };

    let data = DataConfig {
        path: setting(
            layers,
            "data.path",
            |p| p.data.path.as_deref(),
            non_empty_path,
        )?
        .unwrap_or_else(|| defaults::DATA_PATH.into()),
        format: setting(
            layers,
            "data.format",
            |p| p.data.format.as_deref(),
            data_format,
        )?
        .map_or_else(
            || parse_default(data_format, "data.format", defaults::DATA_FORMAT),
            Ok,
        )?,
        csv_delimiter: setting(
            layers,
            "data.csv_delimiter",
            |p| p.data.csv_delimiter.as_deref(),
            csv_delimiter,
        )?
        .map_or_else(
            || parse_default(csv_delimiter, "data.csv_delimiter", defaults::CSV_DELIMITER),
            Ok,
        )?,
    };

    let output = OutputConfig {
        dir: setting(
            layers,
            "output.dir",
            |p| p.output.dir.as_deref(),
            non_empty_path,
        )?
        .unwrap_or_else(|| defaults::OUTPUT_DIR.into()),
        name: setting(
            layers,
            "output.name",
            |p| p.output.name.as_deref(),
            non_empty_text,
        )?
        .unwrap_or_else(|| {
            default_output_name(&template.placeholder_open, &template.placeholder_close)
        }),
        merge: setting(
            layers,
            "output.merge",
            |p| p.output.merge.as_deref(),
            non_empty_path,
        )?,
        force: setting(layers, "output.force", |p| p.output.force.as_ref(), any)?.unwrap_or(false),
    };

    let render = RenderConfig {
        jobs: setting(layers, "render.jobs", |p| p.render.jobs.as_ref(), jobs)?
            .unwrap_or(defaults::JOBS),
        font_dirs: setting(
            layers,
            "render.font_dirs",
            |p| p.render.font_dirs.as_deref(),
            font_dirs,
        )?
        .unwrap_or_default(),
        engine: setting(
            layers,
            "render.engine",
            |p| p.render.engine.as_deref(),
            engine,
        )?
        .map_or_else(
            || parse_default(engine, "render.engine", defaults::ENGINE),
            Ok,
        )?,
        inkscape: setting(
            layers,
            "render.inkscape",
            |p| p.render.inkscape.as_deref(),
            non_empty_path,
        )?
        .unwrap_or_else(|| defaults::INKSCAPE.into()),
        format: setting(
            layers,
            "render.format",
            |p| p.render.format.as_deref(),
            render_format,
        )?
        .map_or_else(
            || parse_default(render_format, "render.format", defaults::FORMAT),
            Ok,
        )?,
        dpi: setting(layers, "render.dpi", |p| p.render.dpi.as_ref(), dpi)?
            .unwrap_or(defaults::DPI),
    };

    let log = LogConfig {
        level: setting(layers, "log.level", |p| p.log.level.as_deref(), log_level)?.map_or_else(
            || parse_default(log_level, "log.level", defaults::LOG_LEVEL),
            Ok,
        )?,
    };

    Ok(Config {
        config_path,
        template,
        data,
        output,
        render,
        log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PartialData, PartialLog, PartialOutput, PartialRender, PartialTemplate};

    fn layer(source: Source, values: Partial) -> Layer {
        Layer { source, values }
    }

    fn file() -> Source {
        Source::File("multi-svg-to-pdf.toml".into())
    }

    #[test]
    fn no_layers_gives_defaults() {
        let config = resolve("multi-svg-to-pdf.toml".into(), &[]).unwrap();
        assert_eq!(config.template.path, PathBuf::from("template.svg"));
        assert_eq!(config.template.placeholder_open, "{{");
        assert_eq!(config.data.format, DataFormat::Auto);
        assert_eq!(config.data.csv_delimiter, b',');
        assert_eq!(config.output.name, "{{_row}}.pdf");
        assert_eq!(config.output.merge, None);
        assert!(!config.output.force);
        assert_eq!(config.render.jobs, 0);
        assert!(config.render.font_dirs.is_empty());
        assert_eq!(config.render.engine, Engine::Native);
        assert_eq!(config.render.inkscape, PathBuf::from("inkscape"));
        assert_eq!(config.render.format, RenderFormat::Vector);
        assert_eq!(config.render.dpi, 300);
        assert_eq!(config.log.level, LogLevel::Info);
    }

    #[test]
    fn first_layer_that_sets_a_value_wins() {
        let cli = Partial {
            log: PartialLog {
                level: Some("debug".into()),
            },
            ..Partial::default()
        };
        let env = Partial {
            log: PartialLog {
                level: Some("trace".into()),
            },
            render: PartialRender {
                jobs: Some(2),
                font_dirs: None,
                ..PartialRender::default()
            },
            ..Partial::default()
        };
        let toml = Partial {
            render: PartialRender {
                jobs: Some(8),
                font_dirs: Some(vec!["fonts".into()]),
                ..PartialRender::default()
            },
            ..Partial::default()
        };
        let config = resolve(
            "c.toml".into(),
            &[
                layer(Source::CommandLine, cli),
                layer(Source::Environment, env),
                layer(file(), toml),
            ],
        )
        .unwrap();
        assert_eq!(config.log.level, LogLevel::Debug);
        assert_eq!(config.render.jobs, 2);
        assert_eq!(config.render.font_dirs, vec![PathBuf::from("fonts")]);
    }

    #[test]
    fn default_output_name_follows_the_delimiters() {
        let values = Partial {
            template: PartialTemplate {
                placeholder_open: Some("${".into()),
                placeholder_close: Some("}".into()),
                path: None,
            },
            ..Partial::default()
        };
        let config = resolve("c.toml".into(), &[layer(file(), values)]).unwrap();
        assert_eq!(config.output.name, "${_row}.pdf");
    }

    #[test]
    fn delimiter_accepts_one_ascii_character_or_tab() {
        let origin = Origin {
            setting: "data.csv_delimiter",
            source: &Source::Default,
        };
        assert_eq!(csv_delimiter(";", origin).unwrap(), b';');
        assert_eq!(csv_delimiter("tab", origin).unwrap(), b'\t');
        assert_eq!(csv_delimiter("\\t", origin).unwrap(), b'\t');
        assert_eq!(csv_delimiter("\t", origin).unwrap(), b'\t');
        ["", ";;", "é"]
            .iter()
            .for_each(|bad| assert!(csv_delimiter(bad, origin).is_err(), "{bad:?}"));
    }

    #[test]
    fn invalid_value_names_setting_and_source() {
        let values = Partial {
            data: PartialData {
                format: Some("xml".into()),
                ..PartialData::default()
            },
            ..Partial::default()
        };
        let e = resolve("c.toml".into(), &[layer(Source::Environment, values)]).unwrap_err();
        match e {
            Error::Invalid {
                setting,
                source,
                input,
                ..
            } => {
                assert_eq!(
                    (setting, source, input.as_str()),
                    ("data.format", Source::Environment, "xml")
                );
            }
            other => panic!("unexpected error {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_and_out_of_range_values() {
        let cases = [
            Partial {
                render: PartialRender {
                    dpi: Some(0),
                    ..PartialRender::default()
                },
                ..Partial::default()
            },
            Partial {
                render: PartialRender {
                    format: Some("png".into()),
                    ..PartialRender::default()
                },
                ..Partial::default()
            },
            Partial {
                template: PartialTemplate {
                    placeholder_open: Some(String::new()),
                    ..PartialTemplate::default()
                },
                ..Partial::default()
            },
            Partial {
                output: PartialOutput {
                    name: Some(String::new()),
                    ..PartialOutput::default()
                },
                ..Partial::default()
            },
            Partial {
                output: PartialOutput {
                    merge: Some(PathBuf::new()),
                    ..PartialOutput::default()
                },
                ..Partial::default()
            },
            Partial {
                data: PartialData {
                    path: Some(PathBuf::new()),
                    ..PartialData::default()
                },
                ..Partial::default()
            },
            Partial {
                render: PartialRender {
                    jobs: Some(MAX_JOBS + 1),
                    font_dirs: None,
                    ..PartialRender::default()
                },
                ..Partial::default()
            },
            Partial {
                render: PartialRender {
                    jobs: None,
                    font_dirs: Some(vec![PathBuf::new()]),
                    ..PartialRender::default()
                },
                ..Partial::default()
            },
            Partial {
                log: PartialLog {
                    level: Some("warn".into()),
                },
                ..Partial::default()
            },
        ];
        cases.into_iter().for_each(|values| {
            let result = resolve(
                "c.toml".into(),
                &[layer(Source::CommandLine, values.clone())],
            );
            assert!(matches!(result, Err(Error::Invalid { .. })), "{values:?}");
        });
    }
}
