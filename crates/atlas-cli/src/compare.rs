//! `codeatlas compare`.

use std::path::Path;

use atlas_analysis::{compare, find_repository};
use atlas_model::{AtlasConfig, AtlasError, CompareRequest, CompareResponse, Cycle, ImportChange};
use atlas_storage::Database;

use crate::cli::{CompareArgs, OutputFormat};
use crate::history::safe;

pub fn run(args: CompareArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let request = CompareRequest {
        base: args.base,
        target: args.target,
        limit: Some(args.limit),
    };
    let response = compare(&database, &repository, &request, config)?;
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

fn print_text(response: &CompareResponse) {
    let (base, target, counts) = (&response.base, &response.target, &response.counts);
    println!("base   {}  {}", base.commit.oid, safe(&base.commit.subject));
    println!(
        "target {}  {}",
        target.commit.oid,
        safe(&target.commit.subject)
    );
    println!("These are changes in static imports, not in what runs.\n");

    println!("  files    {} -> {}", base.file_count, target.file_count);
    println!(
        "  imports  {} -> {}",
        base.import_count, target.import_count
    );
    println!(
        "  cycles   {} -> {}\n",
        base.cycle_count, target.cycle_count
    );

    for file in &response.added_files {
        println!("  + file   {}", safe(file.path.as_str()));
    }
    for file in &response.removed_files {
        println!("  - file   {}", safe(file.path.as_str()));
    }
    for rename in &response.renamed_files {
        println!(
            "  ~ file   {} -> {}",
            safe(rename.from.as_str()),
            safe(rename.to.as_str())
        );
    }
    print_imports('+', &response.added_imports);
    print_imports('-', &response.removed_imports);
    print_cycles('+', &response.new_cycles);
    print_cycles('-', &response.resolved_cycles);
    for cycle in &response.changed_cycles {
        let list = |paths: &[atlas_model::RepoPath]| {
            let paths: Vec<_> = paths.iter().map(|p| safe(p.as_str())).collect();
            paths.join(", ")
        };
        println!(
            "  ~ cycle  {} members -> {}: joined [{}], left [{}]",
            cycle.before.members.len(),
            cycle.after.members.len(),
            list(&cycle.joined),
            list(&cycle.left)
        );
    }
    for change in &response.metric_changes {
        println!(
            "  metric   {}: in {} -> {}, out {} -> {}, dependents {} -> {}",
            safe(change.path.as_str()),
            change.in_degree_before,
            change.in_degree_after,
            change.out_degree_before,
            change.out_degree_after,
            change.dependents_before,
            change.dependents_after
        );
    }
    println!(
        "\n{} added and {} removed imports, {} new and {} resolved cycles",
        counts.added_imports, counts.removed_imports, counts.new_cycles, counts.resolved_cycles
    );
    for note in &response.notes {
        println!("note: {}", safe(note));
    }
}

fn print_imports(sign: char, changes: &[ImportChange]) {
    for change in changes {
        println!(
            "  {sign} import {} -> {}{}",
            safe(change.edge.source.as_str()),
            safe(change.edge.target.as_str()),
            if change.uncertain { " (uncertain)" } else { "" }
        );
    }
}

fn print_cycles(sign: char, cycles: &[Cycle]) {
    for cycle in cycles {
        let members: Vec<_> = cycle.members.iter().map(|m| safe(m.as_str())).collect();
        println!("  {sign} cycle  {}", members.join(", "));
    }
}
