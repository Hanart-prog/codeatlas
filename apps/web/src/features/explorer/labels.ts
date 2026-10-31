import type { DependencyEdgeType } from "@/types/generated/DependencyEdgeType";
import type { ParseStatus } from "@/types/generated/ParseStatus";
import type { TsconfigStatus } from "@/types/generated/TsconfigStatus";
import type { UnresolvedReason } from "@/types/generated/UnresolvedReason";

export const EDGE_LABELS: Record<DependencyEdgeType, string> = {
  static_import: "import",
  type_only_import: "type import",
  re_export: "re-export",
  dynamic_import: "dynamic import",
};

export const REASON_LABELS: Record<UnresolvedReason, string> = {
  not_found: "file not found",
  non_source_target: "not a source file",
  escapes_repository: "leaves the repository",
  alias_target_missing: "alias target missing",
  unknown_alias: "undefined alias",
  unsupported_specifier: "unsupported form",
};

export const PARSE_LABELS: Record<ParseStatus, string> = {
  parsed: "parsed",
  partial: "parsed with syntax errors",
  failed: "could not be parsed",
  oversized: "skipped: over size limit",
  over_total_limit: "skipped: over total limit",
  lfs_pointer: "skipped: Git LFS pointer",
};

export const TSCONFIG_LABELS: Record<TsconfigStatus, string> = {
  absent: "no root tsconfig.json",
  applied: "tsconfig.json applied",
  applied_ignoring_extends: "tsconfig.json applied, extends not followed",
  invalid: "tsconfig.json unreadable, no aliases applied",
};
