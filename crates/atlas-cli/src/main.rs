//! The `codeatlas` command. Results go to stdout and diagnostics to stderr.

mod cli;
mod history;
mod serve;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use atlas_model::{AtlasConfig, AtlasError};
use clap::Parser;
use directories::ProjectDirs;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

const DATABASE_FILE: &str = "codeatlas.sqlite";

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn init_logging(verbosity: u8) {
    let default_level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn run(cli: Cli) -> Result<(), AtlasError> {
    let config = load_config(cli.config.as_deref())?;
    let data_dir = cli
        .data_dir
        .or_else(|| config.storage.data_dir.clone())
        .map_or_else(default_data_dir, Ok)?;

    match cli.command {
        Command::Serve(args) => serve::run(args, config, &data_dir),
        Command::Analyze(args) => history::analyze(args, &config, &data_dir),
        Command::Overview(args) => history::overview(args, &data_dir),
        Command::Impact(_) => planned("`codeatlas impact`", 3),
        Command::Compare(_) => planned("`codeatlas compare`", 4),
        Command::Backtest(_) => planned("`codeatlas backtest`", 5),
        Command::Report(_) => planned("`codeatlas report`", 6),
        Command::Demo => planned("`codeatlas demo`", 6),
    }
}

/// The path of the database inside `data_dir`, which is created if needed.
fn database_path(data_dir: &Path) -> Result<PathBuf, AtlasError> {
    fs::create_dir_all(data_dir).map_err(|source| AtlasError::Io {
        context: format!("could not create {}", data_dir.display()),
        source,
    })?;
    Ok(data_dir.join(DATABASE_FILE))
}

fn planned(feature: &'static str, phase: u8) -> Result<(), AtlasError> {
    Err(AtlasError::NotImplemented { feature, phase })
}

fn load_config(path: Option<&Path>) -> Result<AtlasConfig, AtlasError> {
    let Some(path) = path else {
        return Ok(AtlasConfig::default());
    };
    let source = fs::read_to_string(path).map_err(|source| AtlasError::Io {
        context: format!("could not read {}", path.display()),
        source,
    })?;
    AtlasConfig::from_toml_str(&source)
}

/// Data lives outside analyzed repositories so that using CodeAtlas never writes
/// to them.
fn default_data_dir() -> Result<PathBuf, AtlasError> {
    ProjectDirs::from("", "", "codeatlas")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .ok_or_else(|| AtlasError::Config("no home directory found; pass --data-dir".to_owned()))
}
