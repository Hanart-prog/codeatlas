//! `codeatlas demo`: real analysis of a generated repository, needing no network
//! and leaving the user's own database alone.

use std::fs;
use std::path::Path;

use atlas_analysis::{AnalyzeOptions, analyze, backtest};
use atlas_model::{AtlasConfig, AtlasError, BacktestRequest};
use atlas_storage::Database;

use crate::cli::{DemoArgs, ServeArgs};

/// The demo history is short, so it needs less warm-up than the default.
const DEMO_WARMUP: u32 = 60;

pub fn run(args: DemoArgs, config: AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let demo_dir = data_dir.join("demo");
    let repository = demo_dir.join("product");
    if !repository.join(".git").is_dir() {
        // A half-written repository from an interrupted run cannot be resumed.
        if repository.exists() {
            fs::remove_dir_all(&repository).map_err(|source| AtlasError::Io {
                context: format!("could not clear {}", repository.display()),
                source,
            })?;
        }
        fs::create_dir_all(&repository).map_err(|source| AtlasError::Io {
            context: format!("could not create {}", repository.display()),
            source,
        })?;
        eprintln!("Generating the demo repository");
        atlas_fixtures::generate_demo_fixture(&repository)
            .map_err(|error| AtlasError::Git(error.to_string()))?;
    }

    let mut database = Database::open(&crate::database_path(&demo_dir)?)?;
    eprintln!("Analyzing it");
    let outcome = analyze(
        &mut database,
        &repository,
        &AnalyzeOptions::default(),
        &config,
    )?;
    eprintln!("Backtesting its rankings");
    let request = BacktestRequest {
        warmup: Some(DEMO_WARMUP),
        ..BacktestRequest::default()
    };
    backtest(&database, &outcome.repository, &request, &config, |_, _| {})?;
    drop(database);

    eprintln!(
        "\nThis repository is generated, so its numbers describe the sample, not real projects.\n\
         Files: {}",
        repository.display()
    );
    if args.prepare_only {
        return Ok(());
    }
    crate::serve::run(
        ServeArgs {
            repo: Some(repository),
            host: None,
            port: args.port,
            web_dir: args.web_dir,
        },
        config,
        &demo_dir,
    )
}
