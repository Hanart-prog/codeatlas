//! `codeatlas backtest`.

use std::path::Path;

use atlas_analysis::{backtest, find_repository};
use atlas_model::{AtlasConfig, AtlasError, BacktestRequest, BacktestResult, Ranker};
use atlas_storage::Database;

use crate::cli::{BacktestArgs, OutputFormat};
use crate::history::safe;

pub fn run(args: BacktestArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let request = BacktestRequest {
        warmup: Some(args.warmup),
        max_cases: Some(args.max_cases),
        top_k: Some(args.top_k),
        history_window: Some(args.window),
        seed: Some(args.seed),
    };
    let entry = backtest(&database, &repository, &request, config, |_, _| {})?;
    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&entry.result)
                .map_err(|error| AtlasError::InvalidInput(error.to_string()))?;
            println!("{json}");
        }
        OutputFormat::Text => print_text(&entry.result),
    }
    Ok(())
}

fn label(ranker: Ranker) -> &'static str {
    match ranker {
        Ranker::Combined => "Change Impact",
        Ranker::CoChange => "co-change only",
        Ranker::ReverseDependency => "imports only",
        Ranker::MostChanged => "most changed",
        Ranker::Random => "random",
    }
}

fn print_text(result: &BacktestResult) {
    let counts = &result.counts;
    println!("This measures co-edit prediction, not bugs, causes or productivity.");
    println!(
        "{} of {} evaluable cases used; coverage {:.0}% of {} candidates\n",
        counts.evaluated_cases,
        counts.evaluable_cases,
        result.coverage * 100.0,
        counts.candidate_cases
    );
    if counts.evaluated_cases > 0 {
        for ranker in &result.rankers {
            println!("{}", label(ranker.ranker));
            for metrics in &ranker.metrics {
                println!(
                    "  @{:<3} precision {:.3}  recall {:.3}  hit rate {:.3} ({:.3} to {:.3})",
                    metrics.k,
                    metrics.precision.mean,
                    metrics.recall.mean,
                    metrics.hit_rate.mean,
                    metrics.hit_rate.low,
                    metrics.hit_rate.high
                );
            }
        }
    }
    for note in &result.notes {
        println!("note: {}", safe(note));
    }
}
