//! `codeatlas analyze` and `codeatlas overview`.

use std::path::Path;

use atlas_analysis::{AnalyzeOptions, analyze as run_analysis, find_repository, overview as load};
use atlas_model::history::ExclusionReason;
use atlas_model::{AtlasConfig, AtlasError, OverviewResponse};
use atlas_storage::Database;

use crate::cli::{AnalyzeArgs, OutputFormat, OverviewArgs};

/// Files shown in text output.
const TEXT_CHURN_ROWS: usize = 10;

pub fn analyze(args: AnalyzeArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let mut database = Database::open(&crate::database_path(data_dir)?)?;
    tracing::info!("analyzing {}", args.repo.display());
    let options = AnalyzeOptions {
        git_ref: args.git_ref,
        history_limit: args.history,
    };
    let outcome = run_analysis(&mut database, &args.repo, &options, config)?;
    if outcome.cached {
        tracing::info!("reused the completed analysis of this commit");
    } else {
        tracing::info!("read {} commits from Git", outcome.commits_read);
    }
    if let Some(reason) = &outcome.graph_skipped {
        eprintln!("warning: no import graph was built: {reason}");
    }
    let overview = load(&database, outcome.repository.id)?;
    print(&overview, args.format)?;
    if matches!(args.format, OutputFormat::Text) && overview.files.tip_typescript_file_count > 0 {
        let repo = safe(&overview.repository.path);
        println!("\nNext: codeatlas serve {repo}");
        println!("  or: codeatlas impact {repo} --file <path from the repository root>");
    }
    Ok(())
}

pub fn overview(args: OverviewArgs, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    print(&load(&database, repository.id)?, args.format)
}

fn print(overview: &OverviewResponse, format: OutputFormat) -> Result<(), AtlasError> {
    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(overview)
                .map_err(|error| AtlasError::InvalidInput(error.to_string()))?;
            println!("{json}");
        }
        OutputFormat::Text => print_text(overview),
    }
    Ok(())
}

/// Paths and subjects come from the repository, so they can hold escape
/// sequences aimed at the terminal that prints them.
pub(crate) fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

fn exclusion_label(reason: ExclusionReason) -> &'static str {
    match reason {
        ExclusionReason::Binary => "binary files",
        ExclusionReason::Oversized => "over the file size limit",
        ExclusionReason::Symlink => "symlinks",
        ExclusionReason::Submodule => "submodules",
        ExclusionReason::Rule => "vendored or generated paths",
    }
}

/// Says so when these results do not reach the repository's current commit.
pub(crate) fn stale_warning(overview: &OverviewResponse) -> Option<String> {
    let head = overview.head_oid.as_ref()?;
    (head != &overview.run.target_oid).then(|| {
        format!(
            "HEAD is {}, but the analysis covers {}. Run `codeatlas analyze` to include it.",
            &head.as_str()[..10],
            &overview.run.target_oid.as_str()[..10]
        )
    })
}

fn counted(count: u32, noun: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{plural}")
}

fn print_text(overview: &OverviewResponse) {
    let OverviewResponse {
        repository,
        run,
        history,
        files,
        exclusions,
        top_churn,
        head_oid: _,
    } = overview;

    println!("{}", safe(&repository.path));
    println!("  analyzed commit  {}", run.target_oid);
    println!(
        "  history          {}, {}",
        counted(history.commit_count, "first-parent commit"),
        counted(history.merge_commit_count, "merge")
    );
    if let (Some(first), Some(last)) = (&history.first_commit, &history.last_commit) {
        println!("  oldest           {}", safe(&first.subject));
        println!("  newest           {}", safe(&last.subject));
    }
    println!(
        "  files at commit  {} ({} TS/TSX)",
        files.tip_file_count, files.tip_typescript_file_count
    );
    println!(
        "  file activity    {} changed, {}",
        counted(files.changed_file_count, "file"),
        counted(files.rename_count, "rename")
    );
    for exclusion in exclusions {
        println!(
            "  excluded         {}: {}",
            counted(exclusion.count, "change"),
            exclusion_label(exclusion.reason)
        );
    }
    if files.unreadable_path_count > 0 {
        println!(
            "  skipped          {} with unreadable paths",
            counted(files.unreadable_path_count, "change")
        );
    }
    if files.tip_typescript_file_count == 0 {
        println!(
            "  warning          no TS or TSX files: CodeAtlas analyzes only TypeScript, so \
             graphs, impact and backtests will be empty"
        );
    }
    if let Some(warning) = stale_warning(overview) {
        println!("  warning          {warning}");
    }
    if history.is_shallow {
        println!("  warning          shallow clone: older history is missing");
    }
    if history.is_truncated {
        println!(
            "  warning          history limit of {} cut off older commits",
            run.history_limit
        );
    }

    if !top_churn.is_empty() {
        println!("\n  commits   +lines   -lines  file");
    }
    for file in top_churn.iter().take(TEXT_CHURN_ROWS) {
        let deleted = if file.exists_at_tip {
            ""
        } else {
            "  (deleted)"
        };
        println!(
            "  {:>7}  {:>7}  {:>7}  {}{deleted}",
            file.commit_count,
            file.added_lines,
            file.deleted_lines,
            safe(file.path.as_str())
        );
    }
}
