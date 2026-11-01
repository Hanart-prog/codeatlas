//! `codeatlas report`.

use std::fs;
use std::path::Path;

use atlas_analysis::{find_repository, render_report, report};
use atlas_model::{AtlasConfig, AtlasError};
use atlas_storage::Database;

use crate::cli::{ReportArgs, ReportFormat};

pub fn run(args: ReportArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let document = report(&database, &repository, config)?;
    let content = match args.format {
        ReportFormat::Html => render_report(&document),
        ReportFormat::Json => serde_json::to_string_pretty(&document)
            .map_err(|error| AtlasError::InvalidInput(error.to_string()))?,
    };
    fs::write(&args.output, content).map_err(|source| AtlasError::Io {
        context: format!("could not write {}", args.output.display()),
        source,
    })?;
    println!("{}", args.output.display());
    Ok(())
}
