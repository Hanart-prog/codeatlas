import type { Cycle } from "@/types/generated/Cycle";
import type { ParserCoverage } from "@/types/generated/ParserCoverage";

import { REASON_LABELS, TSCONFIG_LABELS } from "./labels";

function Row({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex justify-between gap-4 text-sm">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono">{value.toLocaleString("en-US")}</dd>
    </div>
  );
}

export function CoveragePanel({ coverage }: { coverage: ParserCoverage }) {
  return (
    <>
      <dl className="space-y-1">
        <Row label="Files parsed" value={coverage.parsed_file_count} />
        {coverage.partial_file_count > 0 && <Row label="with syntax errors" value={coverage.partial_file_count} />}
        {coverage.skipped_file_count > 0 && <Row label="Files skipped" value={coverage.skipped_file_count} />}
        <Row label="Excluded by rules" value={coverage.excluded_file_count} />
        <Row label="Other languages, not analyzed" value={coverage.unsupported_file_count} />
        <Row label="Imports resolved" value={coverage.resolved_import_count} />
        <Row label="Package imports" value={coverage.external_import_count} />
        <Row label="Imports unresolved" value={coverage.unresolved_import_count} />
        {coverage.unresolved_by_reason.map((entry) => (
          <Row key={entry.reason} label={REASON_LABELS[entry.reason]} value={entry.count} />
        ))}
        {coverage.nonliteral_dynamic_import_count > 0 && (
          <Row label="Non-literal dynamic imports, not in graph" value={coverage.nonliteral_dynamic_import_count} />
        )}
        {coverage.require_call_count > 0 && <Row label="require calls, not in graph" value={coverage.require_call_count} />}
      </dl>
      <p className="mt-3 text-xs text-muted-foreground">{TSCONFIG_LABELS[coverage.tsconfig]}</p>
    </>
  );
}

export function CycleList({ cycles }: { cycles: Cycle[] }) {
  if (cycles.length === 0) {
    return <p className="text-sm text-muted-foreground">No import cycles at this commit.</p>;
  }
  return (
    <ul className="space-y-3">
      {cycles.map((cycle) => (
        <li key={cycle.members.join("|")}>
          <p className="text-xs text-muted-foreground">
            {cycle.members.length} files{cycle.type_only ? " · types only, gone at run time" : ""}
          </p>
          <ul className="mt-0.5">
            {cycle.members.map((member) => (
              <li key={member} className="truncate font-mono text-xs" title={member}>
                {member}
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
}
