//! `codeatlas graph`.

use std::path::Path;

use atlas_analysis::{GraphQuery, directory_map, find_repository, graph, overview};
use atlas_model::{AtlasConfig, AtlasError, DirectoryGroupKind, DirectoryMap, GraphResponse};
use atlas_storage::Database;

use crate::cli::{GraphArgs, OutputFormat};
use crate::history::{safe, stale_warning};

pub fn run(args: GraphArgs, config: &AtlasConfig, data_dir: &Path) -> Result<(), AtlasError> {
    let database = Database::open(&crate::database_path(data_dir)?)?;
    let repository = find_repository(&database, &args.repo)?;
    if args.by_directory {
        let map = directory_map(
            &database,
            &repository,
            args.git_ref.as_deref(),
            args.directory.as_deref().unwrap_or_default(),
            args.limit.clamp(1, config.limits.max_graph_nodes),
            config,
        )?;
        match args.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&map)
                    .map_err(|error| AtlasError::InvalidInput(error.to_string()))?;
                println!("{json}");
            }
            OutputFormat::Text => print_directories(&map),
        }
        return Ok(());
    }
    let query = GraphQuery {
        git_ref: args.git_ref,
        directory: args.directory,
        search: None,
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

fn print_directories(map: &DirectoryMap) {
    println!("commit {}", map.commit);
    println!(
        "  {} files under {:?}",
        map.matching_node_count,
        safe(&map.directory)
    );
    for group in &map.groups {
        let suffix = match group.kind {
            DirectoryGroupKind::Directory => "/",
            DirectoryGroupKind::LooseFiles => " (files directly here)",
        };
        println!(
            "  {:>6} files  {:>6} inner imports  {}{suffix}",
            group.file_count,
            group.internal_import_count,
            safe(&group.path)
        );
    }
    if map.hidden_group_count > 0 {
        println!(
            "  {} more groups with {} files",
            map.hidden_group_count, map.hidden_file_count
        );
    }
    println!();
    for link in &map.links {
        println!(
            "  {} -> {}: {} imports",
            safe(&map.groups[link.source as usize].path),
            safe(&map.groups[link.target as usize].path),
            link.import_count
        );
    }
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
