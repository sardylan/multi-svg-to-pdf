//! The rendering run: read inputs, check them, render every row in parallel, write the PDFs.

use std::path::Path;

use rayon::prelude::*;
use tracing::{debug, info, warn};

use crate::check::{self, Job};
use crate::config::{Config, Engine};
use crate::data;
use crate::errors::{Error, Result};
use crate::output;
use crate::render::Renderer;
use crate::template::{self, Escape, Template};

/// The directory relative links in the template resolve against.
fn template_dir(template: &Path) -> &Path {
    template
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// Renders one job to PDF bytes; errors name the row.
fn render_job(renderer: &Renderer, template: &Template, job: &Job) -> Result<Vec<u8>> {
    renderer
        .render_pdf(&template.render(&job.values, Escape::Xml))
        .map_err(|e| Error::Row {
            row: job.row,
            source: Box::new(e),
        })
}

/// Runs the whole pipeline with `config`.
pub(crate) fn run(config: &Config) -> Result<()> {
    let (open, close) = (
        &config.template.placeholder_open,
        &config.template.placeholder_close,
    );
    let template = Template::parse(
        &template::read_template(&config.template.path)?,
        open,
        close,
        "template",
    )?;
    let name = match config.output.merge {
        Some(_) => None,
        None => Some(Template::parse(
            &config.output.name,
            open,
            close,
            "output name",
        )?),
    };
    let text = data::read_data(&config.data.path)?;
    let table = data::parse_data(
        &text,
        &config.data.path,
        config.data.format,
        config.data.csv_delimiter,
    )?;
    let jobs = check::plan(&template, name.as_ref(), &table, &config.output.dir)?;

    let targets: Vec<&Path> = jobs
        .iter()
        .filter_map(|job| job.file.as_deref())
        .chain(config.output.merge.as_deref())
        .collect();
    output::refuse_existing(output::find_existing(&targets), config.output.force)?;

    if config.render.engine == Engine::Inkscape && !config.render.font_dirs.is_empty() {
        warn!("render.font_dirs is ignored by the inkscape engine; install the fonts instead");
    }
    let renderer = Renderer::load(&config.render, template_dir(&config.template.path));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.render.jobs)
        .build()?;
    info!(
        "rendering {} rows with the {} on {} threads",
        jobs.len(),
        renderer.describe(),
        pool.current_num_threads()
    );

    match &config.output.merge {
        None => {
            output::create_dir(&config.output.dir)?;
            pool.install(|| {
                jobs.par_iter()
                    .map(|job| {
                        let pdf = render_job(&renderer, &template, job)?;
                        job.file.as_deref().map_or(Ok(()), |file| {
                            debug!("row {}: writing {}", job.row, file.display());
                            output::write_pdf(file, &pdf)
                        })
                    })
                    .collect::<Result<Vec<()>>>()
            })?;
            info!(
                "wrote {} files to {}",
                jobs.len(),
                config.output.dir.display()
            );
        }
        Some(file) => {
            let pdfs = pool.install(|| {
                jobs.par_iter()
                    .map(|job| render_job(&renderer, &template, job))
                    .collect::<Result<Vec<_>>>()
            })?;
            let merged = output::merge_pdfs(&pdfs)?;
            output::create_dir(file.parent().unwrap_or(Path::new("")))?;
            output::write_pdf(file, &merged)?;
            info!("wrote {} pages to {}", pdfs.len(), file.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::resolve;
    use std::path::PathBuf;

    const CARD: &str = include_str!("../tests/fixtures/template/card.svg");
    const PEOPLE: &str = include_str!("../tests/fixtures/data/people.csv");

    /// A temporary directory with the fixtures, and a configuration that uses it.
    fn setup(name: &str) -> (PathBuf, Config) {
        let dir = std::env::temp_dir().join(format!("msvg-test-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("card.svg"), CARD).unwrap();
        std::fs::write(dir.join("people.csv"), PEOPLE).unwrap();
        let mut config = resolve("unused.toml".into(), &[]).unwrap();
        config.template.path = dir.join("card.svg");
        config.data.path = dir.join("people.csv");
        config.output.dir = dir.join("out");
        config.output.name =
            "{{surname}}_{{name}}{{#call_upper}}_{{call_upper}}{{call_lower}}{{/call_upper}}.pdf"
                .into();
        config.render.jobs = 2;
        (dir, config)
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn template_dir_of_a_bare_name_is_current() {
        assert_eq!(template_dir(Path::new("t.svg")), Path::new("."));
        assert_eq!(template_dir(Path::new("a/t.svg")), Path::new("a"));
    }

    #[test]
    fn renders_one_file_per_row_and_refuses_to_overwrite() {
        let (dir, config) = setup("files");
        run(&config).unwrap();
        assert_eq!(
            listing(&config.output.dir),
            [
                "ANNA Erminia_VERDI, JR_X0TEST.pdf",
                "LAURA_BIANCHI.pdf",
                "OTTAVIO_ROSSI_N0CALL.pdf"
            ]
        );
        assert!(matches!(run(&config), Err(Error::OutputExists { .. })));
        let forced = Config {
            output: crate::config::OutputConfig {
                force: true,
                ..config.output.clone()
            },
            ..config
        };
        run(&forced).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn merges_all_rows_into_one_file() {
        let (dir, mut config) = setup("merge");
        config.output.merge = Some(dir.join("all").join("cards.pdf"));
        run(&config).unwrap();
        let doc = lopdf::Document::load(dir.join("all").join("cards.pdf")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn unknown_placeholder_stops_before_writing() {
        let (dir, mut config) = setup("unknown");
        config.output.name = "{{nope}}.pdf".into();
        let result = run(&config);
        let wrote = config.output.dir.exists();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(matches!(result, Err(Error::UnknownPlaceholders { .. })));
        assert!(!wrote);
    }
}
