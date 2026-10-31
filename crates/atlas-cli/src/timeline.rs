//! `codeatlas timeline`.

use std::path::Path;

use atlas_analysis::{find_repository, timeline};
use atlas_model::{AtlasConfig, AtlasError};
use atlas_storage::Database;

use crate::cli::{OutputFormat, TimelineArgs};
use crate::history::safe;

pub fn run(args: TimelineArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let response = timeline(&database, &repository, args.points, config)?;
    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&response)
                .map_err(|error| AtlasError::InvalidInput(error.to_string()))?;
            println!("{json}");
        }
        OutputFormat::Text => {
            println!(
                "{} of {} commits",
                response.points.len(),
                response.commit_count
            );
            for point in &response.points {
                println!(
                    "{:>6}  {}  {:>5} files {:>6} imports {:>3} cycles  {}",
                    point.position,
                    &point.commit.oid.as_str()[..10],
                    point.file_count,
                    point.import_count,
                    point.cycle_count,
                    safe(&point.commit.subject)
                );
            }
        }
    }
    Ok(())
}
