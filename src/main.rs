//! multi-svg-to-pdf: render one PDF per data row from an SVG template.
//!
//! See CLAUDE.md and doc/ for the design and the coding rules.

mod check;
mod config;
mod data;
mod errors;
mod logging;
mod output;
mod pipeline;
mod render;
mod template;

use std::process::ExitCode;

use tracing::{debug, info};

use crate::config::Action;
use crate::errors::{Error, Result};

fn main() -> ExitCode {
    match program() {
        Ok(()) => ExitCode::SUCCESS,
        // clap renders --help, --version and usage errors itself, on the right stream.
        Err(Error::Cli(e)) => {
            // Nothing is left to report a failure to if printing to the terminal fails.
            let _ = e.print();
            Error::Cli(e).exit_code()
        }
        Err(e) => {
            eprintln!("error: {e}");
            e.exit_code()
        }
    }
}

fn program() -> Result<()> {
    let loaded = config::load_config(config::command(), std::env::args_os())?;
    logging::start_logging(loaded.config.log.level)?;
    loaded.notes.iter().for_each(|note| info!("{note}"));
    debug!(config = ?loaded.config, "configuration resolved");

    match loaded.action {
        Action::DumpConfig => save(&loaded.config),
        Action::SaveConfig => save(&loaded.config).and_then(|()| run(&loaded.config)),
        Action::Run => run(&loaded.config),
    }
}

fn save(config: &config::Config) -> Result<()> {
    config::save_config(config)?;
    info!("configuration written to {}", config.config_path.display());
    Ok(())
}

fn run(config: &config::Config) -> Result<()> {
    pipeline::run(config)
}
