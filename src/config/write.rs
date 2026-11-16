//! `--save-config` / `--dump-config` (§3.5): a pure render of the TOML text, a thin write.

use std::path::{Path, PathBuf};

use super::Config;
use crate::errors::{Error, Result};

/// One `key = value` line with its help comment; `None` writes a commented-out key.
struct Entry {
    id: &'static str,
    key: &'static str,
    value: Option<String>,
}

fn string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn path(setting: &'static str, value: &Path) -> Result<String> {
    value
        .to_str()
        .map(string)
        .ok_or_else(|| Error::NonUtf8Path {
            setting,
            path: value.to_owned(),
        })
}

fn paths(setting: &'static str, values: &[PathBuf]) -> Result<String> {
    values
        .iter()
        .map(|value| path(setting, value))
        .collect::<Result<Vec<_>>>()
        .map(|items| format!("[{}]", items.join(", ")))
}

fn render_section(name: &str, entries: &[Entry], help: &impl Fn(&str) -> String) -> String {
    let body: String = entries
        .iter()
        .map(|entry| {
            let line = match &entry.value {
                Some(value) => format!("{} = {value}", entry.key),
                None => format!("# {} = \"\"", entry.key),
            };
            format!("# {}\n{line}\n", help(entry.id))
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("[{name}]\n{body}")
}

/// Renders every setting of `config` as TOML. `help` gives the comment of a clap argument id.
pub(crate) fn render_toml(config: &Config, help: &impl Fn(&str) -> String) -> Result<String> {
    let sections = [
        (
            "template",
            vec![
                Entry {
                    id: "template",
                    key: "path",
                    value: Some(path("template.path", &config.template.path)?),
                },
                Entry {
                    id: "placeholder_open",
                    key: "placeholder_open",
                    value: Some(string(&config.template.placeholder_open)),
                },
                Entry {
                    id: "placeholder_close",
                    key: "placeholder_close",
                    value: Some(string(&config.template.placeholder_close)),
                },
            ],
        ),
        (
            "data",
            vec![
                Entry {
                    id: "data",
                    key: "path",
                    value: Some(path("data.path", &config.data.path)?),
                },
                Entry {
                    id: "data_format",
                    key: "format",
                    value: Some(string(config.data.format.as_str())),
                },
                Entry {
                    id: "csv_delimiter",
                    key: "csv_delimiter",
                    value: Some(string(&char::from(config.data.csv_delimiter).to_string())),
                },
            ],
        ),
        (
            "output",
            vec![
                Entry {
                    id: "output_dir",
                    key: "dir",
                    value: Some(path("output.dir", &config.output.dir)?),
                },
                Entry {
                    id: "output_name",
                    key: "name",
                    value: Some(string(&config.output.name)),
                },
                Entry {
                    id: "merge",
                    key: "merge",
                    value: config
                        .output
                        .merge
                        .as_deref()
                        .map(|p| path("output.merge", p))
                        .transpose()?,
                },
                Entry {
                    id: "force",
                    key: "force",
                    value: Some(config.output.force.to_string()),
                },
            ],
        ),
        (
            "render",
            vec![
                Entry {
                    id: "jobs",
                    key: "jobs",
                    value: Some(config.render.jobs.to_string()),
                },
                Entry {
                    id: "font_dirs",
                    key: "font_dirs",
                    value: Some(paths("render.font_dirs", &config.render.font_dirs)?),
                },
                Entry {
                    id: "engine",
                    key: "engine",
                    value: Some(string(config.render.engine.as_str())),
                },
                Entry {
                    id: "inkscape",
                    key: "inkscape",
                    value: Some(path("render.inkscape", &config.render.inkscape)?),
                },
                Entry {
                    id: "format",
                    key: "format",
                    value: Some(string(config.render.format.as_str())),
                },
                Entry {
                    id: "dpi",
                    key: "dpi",
                    value: Some(config.render.dpi.to_string()),
                },
            ],
        ),
        (
            "log",
            vec![Entry {
                id: "log_level",
                key: "level",
                value: Some(string(config.log.level.as_str())),
            }],
        ),
    ];

    let body = sections
        .iter()
        .map(|(name, entries)| render_section(name, entries, help))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "# multi-svg-to-pdf configuration file, written by --save-config / --dump-config.\n\n{body}"
    ))
}

/// Writes `text` to `path`, creating a missing parent directory and replacing an existing file.
pub(crate) fn write_config_file(path: &Path, text: &str) -> Result<()> {
    match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => std::fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
            path: parent.to_owned(),
            source,
        }),
        None => Ok(()),
    }?;
    std::fs::write(path, text).map_err(|source| Error::WriteConfig {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::cli::short_help;
    use crate::config::file::parse_config_file;
    use crate::config::{DataFormat, Layer, Source, resolve};
    use crate::logging::LogLevel;

    fn read_back(text: &str) -> Config {
        let path = PathBuf::from("multi-svg-to-pdf.toml");
        let values = parse_config_file(text, &path).unwrap();
        resolve(
            path.clone(),
            &[Layer {
                source: Source::File(path),
                values,
            }],
        )
        .unwrap()
    }

    #[test]
    fn defaults_round_trip() {
        let config = resolve("multi-svg-to-pdf.toml".into(), &[]).unwrap();
        let text = render_toml(&config, &short_help).unwrap();
        assert_eq!(read_back(&text), config);
        assert!(
            text.contains("# merge = \"\""),
            "unset merge is commented out:\n{text}"
        );
    }

    #[test]
    fn every_setting_round_trips() {
        let base = resolve("multi-svg-to-pdf.toml".into(), &[]).unwrap();
        let config = Config {
            template: crate::config::TemplateConfig {
                path: "a \"quoted\" dir/t.svg".into(),
                placeholder_open: "<%".into(),
                placeholder_close: "%>".into(),
            },
            data: crate::config::DataConfig {
                path: "-".into(),
                format: DataFormat::Json,
                csv_delimiter: b'\t',
            },
            output: crate::config::OutputConfig {
                dir: "out".into(),
                name: "<%id%>.pdf".into(),
                merge: Some("all.pdf".into()),
                force: true,
            },
            render: crate::config::RenderConfig {
                jobs: 3,
                font_dirs: vec!["f1".into(), "f2".into()],
                engine: crate::config::Engine::Inkscape,
                inkscape: "/opt/ink/inkscape".into(),
                format: crate::config::RenderFormat::Raster,
                dpi: 150,
            },
            log: crate::config::LogConfig {
                level: LogLevel::Trace,
            },
            ..base
        };
        assert_eq!(
            read_back(&render_toml(&config, &short_help).unwrap()),
            config
        );
    }

    #[test]
    fn keys_carry_their_help() {
        let config = resolve("multi-svg-to-pdf.toml".into(), &[]).unwrap();
        let text = render_toml(&config, &short_help).unwrap();
        assert!(
            text.contains("# Path of the SVG template\npath = \"template.svg\""),
            "{text}"
        );
    }

    #[test]
    fn writes_into_a_new_directory_and_replaces() {
        let dir = std::env::temp_dir().join(format!(
            "multi-svg-to-pdf-test-write-{}",
            std::process::id()
        ));
        let path = dir.join("nested").join("multi-svg-to-pdf.toml");
        write_config_file(&path, "first").unwrap();
        write_config_file(&path, "second").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(text, "second");
    }

    #[test]
    fn write_into_a_file_path_fails() {
        let file = std::env::temp_dir().join(format!(
            "multi-svg-to-pdf-test-blocker-{}",
            std::process::id()
        ));
        std::fs::write(&file, "").unwrap();
        let result = write_config_file(&file.join("multi-svg-to-pdf.toml"), "x");
        std::fs::remove_file(&file).unwrap();
        assert!(matches!(result, Err(Error::CreateDir { .. })));
    }
}
