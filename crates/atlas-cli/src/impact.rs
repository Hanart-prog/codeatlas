//! `codeatlas impact`.

use std::path::Path;

use atlas_analysis::{find_repository, impact};
use atlas_model::{AtlasConfig, AtlasError, ImpactRequest, ImpactResponse};
use atlas_storage::Database;

use crate::cli::{ImpactArgs, OutputFormat};
use crate::history::safe;

pub fn run(args: ImpactArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let request = ImpactRequest {
        selected_paths: args
            .files
            .iter()
            .map(|file| from_repository_root(&repository.path, file))
            .collect(),
        cutoff: Some(args.git_ref),
        max_depth: Some(args.depth),
        top_k: Some(args.top),
        history_window: Some(args.window),
        static_weight: None,
        historical_weight: None,
    };
    let response = impact(&database, &repository, &request, config)?;
    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&response)
                .map_err(|error| AtlasError::InvalidInput(error.to_string()))?;
            println!("{json}");
        }
        OutputFormat::Text => print_text(&response),
    }
    Ok(())
}

/// Accepts what a shell or editor gives: `./src/a.ts`, or a full path inside the
/// repository. Anything else is left for validation to report.
fn from_repository_root(root: &str, file: &str) -> String {
    let relative = file.strip_prefix("./").unwrap_or(file);
    let path = Path::new(relative);
    let resolved = path.canonicalize().ok();
    let inside = path
        .strip_prefix(root)
        .ok()
        .or_else(|| resolved.as_deref()?.strip_prefix(root).ok());
    inside.and_then(Path::to_str).unwrap_or(relative).to_owned()
}

fn print_text(response: &ImpactResponse) {
    let history = &response.history;
    println!("as of commit {}", response.cutoff);
    println!(
        "history: {} commits, {} eligible, {} bulk commits skipped",
        history.commit_count, history.eligible_commit_count, history.bulk_commit_count
    );
    println!("Scores rank files to inspect. A score is not a probability of failure.\n");

    for item in &response.recommendations {
        println!(
            "{:>2}. {:.3}  {}",
            item.rank,
            item.score,
            safe(item.path.as_str())
        );
        if let Some(evidence) = &item.static_evidence {
            let chain: Vec<_> = evidence.path.iter().map(|p| safe(p.as_str())).collect();
            println!(
                "      imports ({} hops): {}",
                evidence.distance,
                chain.join(" -> ")
            );
        }
        if let Some(evidence) = &item.historical_evidence {
            println!(
                "      changed with {} in {} of {} of its commits (lift {:.1})",
                safe(evidence.selected.as_str()),
                evidence.shared_commit_count,
                evidence.selected_commit_count,
                evidence.lift
            );
        }
        for caveat in &item.caveats {
            println!("      note: {}", safe(caveat));
        }
    }
    for note in &response.notes {
        println!("note: {}", safe(note));
    }
}
