import type { ArchitectureSummary } from "@/types/generated/ArchitectureSummary";
import type { CompareResponse } from "@/types/generated/CompareResponse";
import type { GraphNode } from "@/types/generated/GraphNode";
import type { ImportChange } from "@/types/generated/ImportChange";

function side(oid: string, subject: string, file_count: number): ArchitectureSummary {
  return {
    commit: { oid: oid.repeat(40), committed_at: 1, subject },
    file_count,
    import_count: 4,
    cycle_count: 0,
    cyclic_file_count: 0,
    largest_cycle_size: 0,
    coverage: {
      parsed_file_count: file_count,
      partial_file_count: 0,
      skipped_file_count: 0,
      unsupported_file_count: 0,
      excluded_file_count: 0,
      resolved_import_count: 4,
      external_import_count: 0,
      unresolved_import_count: 0,
      unresolved_by_reason: [],
      nonliteral_dynamic_import_count: 0,
      require_call_count: 0,
      tsconfig: "absent",
    },
  };
}

export function graphNode(path: string, out_degree: number): GraphNode {
  return {
    path,
    language: "typescript",
    loc: 1,
    parse_status: "parsed",
    in_degree: 0,
    out_degree,
    in_cycle: false,
  };
}

export function importChange(source: string, target: string, uncertain = false): ImportChange {
  return {
    edge: { source, target, edge_type: "static_import", resolution: "relative", specifier: "./x", line: 1 },
    uncertain,
  };
}

/** A comparison with nothing in it but what a test supplies. */
export function comparison(parts: Partial<CompareResponse> = {}): CompareResponse {
  const response: CompareResponse = {
    base: side("a", "Before", 6),
    target: side("b", "After", 7),
    relation: "forward",
    counts: {
      added_files: 0,
      removed_files: 0,
      renamed_files: 0,
      added_imports: 0,
      removed_imports: 0,
      uncertain_imports: 0,
      imports_following_renames: 0,
      new_cycles: 0,
      resolved_cycles: 0,
      changed_cycles: 0,
      unchanged_cycles: 0,
      metric_changes: 0,
    },
    added_files: [],
    removed_files: [],
    renamed_files: [],
    added_imports: [],
    removed_imports: [],
    new_cycles: [],
    resolved_cycles: [],
    changed_cycles: [],
    metric_changes: [],
    notes: [],
    ...parts,
  };
  return {
    ...response,
    counts: {
      ...response.counts,
      added_files: response.added_files.length,
      removed_files: response.removed_files.length,
      renamed_files: response.renamed_files.length,
      added_imports: response.added_imports.length,
      removed_imports: response.removed_imports.length,
      new_cycles: response.new_cycles.length,
      resolved_cycles: response.resolved_cycles.length,
      changed_cycles: response.changed_cycles.length,
      metric_changes: response.metric_changes.length,
      ...parts.counts,
    },
  };
}
