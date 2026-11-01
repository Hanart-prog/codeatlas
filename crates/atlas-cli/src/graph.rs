//! `codeatlas graph`.

use std::path::Path;

use atlas_analysis::{GraphQuery, find_repository, graph, overview};
use atlas_model::{AtlasConfig, AtlasError, GraphResponse};
use atlas_storage::Database;

use crate::cli::{GraphArgs, OutputFormat};
use crate::history::{safe, stale_warning};

pub fn run(args: GraphArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    let query = GraphQuery {
        git_ref: args.git_ref,
        directory: args.directory,
        limit: args.limit.clamp(1, config.limits.max_graph_nodes),
        include_cochange: false,
    };
    let response = graph(&database, &repository, &query, config)?;
    if query.git_ref.is_none()
        && let Some(warning) = stale_warning(&overview(&database, repository.id)?)
    {
        eprintln!("warning: {warning}");
    }
    if let Some(directory) = &query.directory
        && response.matching_node_count == 0
    {
        eprintln!(
            "warning: no source files under {:?}. Directories are relative to the repository root.",
            safe(directory)
        );
    }
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

fn print_text(response: &GraphResponse) {
    let coverage = &response.coverage;
    println!("commit {}", response.commit);
    println!(
        "  files    {} shown of {} ({} in total)",
        response.nodes.len(),
        response.matching_node_count,
        response.total_node_count
    );
    println!(
        "  imports  {} statements resolved to {} edges, {} external, {} unresolved",
        coverage.resolved_import_count,
        response.total_edge_count,
        coverage.external_import_count,
        coverage.unresolved_import_count
    );
    for cycle in &response.cycles {
        let members: Vec<_> = cycle.members.iter().map(|m| safe(m.as_str())).collect();
        let label = if cycle.type_only {
            "type cycle"
        } else {
            "cycle"
        };
        println!("  {label}    {}", members.join(", "));
    }
    println!();
    for edge in &response.edges {
        println!(
            "  {} -> {}",
            safe(edge.source.as_str()),
            safe(edge.target.as_str())
        );
    }
}
