import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { EDGE_LABELS } from "@/features/explorer/labels";
import type { CompareResponse } from "@/types/generated/CompareResponse";
import type { ImportChange } from "@/types/generated/ImportChange";

import { carryThroughRenames } from "./overlay";

function statusOf(path: string, response: CompareResponse, carry: (path: string) => string): string {
  if (response.added_files.some((file) => file.path === path)) return "Added";
  if (response.removed_files.some((file) => file.path === path)) return "Removed";
  const rename = response.renamed_files.find((item) => item.from === path || item.to === path);
  if (rename) return `Renamed from ${rename.from}`;
  return carry(path) === path ? "At both commits" : "Renamed";
}

function Touching({ title, changes, onSelect }: { title: string; changes: ImportChange[]; onSelect: (path: string) => void }) {
  if (changes.length === 0) return null;
  return (
    <section>
      <h3 className="text-xs text-muted-foreground">
        {title} ({changes.length})
      </h3>
      <ul className="mt-1 space-y-0.5">
        {changes.map(({ edge, uncertain }) => (
          <li key={`${edge.source}|${edge.target}|${edge.edge_type}`} className="text-xs">
            <button
              type="button"
              className="block max-w-full truncate font-mono hover:underline"
              title={`${edge.source} imports ${edge.target}`}
              onClick={() => onSelect(edge.source)}
            >
              {edge.source} → {edge.target}
            </button>
            <span className="text-muted-foreground">
              {EDGE_LABELS[edge.edge_type]}
              {uncertain && ", uncertain: the importing file did not parse cleanly"}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

interface Props {
  path: string;
  response: CompareResponse;
  onClose: () => void;
  onSelect: (path: string) => void;
}

/** One file's part of a comparison. */
export function FileDrawer({ path, response, onClose, onSelect }: Props) {
  const carry = carryThroughRenames(response);
  const current = carry(path);
  const metric = response.metric_changes.find((change) => change.path === current);
  const touches = ({ edge }: ImportChange) => carry(edge.source) === current || carry(edge.target) === current;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="min-w-0 truncate font-mono text-xs" title={current}>
          {current}
        </CardTitle>
        <Button variant="ghost" onClick={onClose}>
          Close
        </Button>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-xs text-muted-foreground">{statusOf(current, response, carry)}</p>
        {metric ? (
          <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-0.5 text-xs">
            <dt className="text-muted-foreground">Imported by</dt>
            <dd className="font-mono">
              {metric.in_degree_before} → {metric.in_degree_after}
            </dd>
            <dt className="text-muted-foreground">Imports</dt>
            <dd className="font-mono">
              {metric.out_degree_before} → {metric.out_degree_after}
            </dd>
            <dt className="text-muted-foreground">All dependents</dt>
            <dd className="font-mono">
              {metric.dependents_before} → {metric.dependents_after}
            </dd>
          </dl>
        ) : (
          <p className="text-xs text-muted-foreground">No before and after metrics for this file.</p>
        )}
        <Touching title="Added imports" changes={response.added_imports.filter(touches)} onSelect={onSelect} />
        <Touching title="Removed imports" changes={response.removed_imports.filter(touches)} onSelect={onSelect} />
      </CardContent>
    </Card>
  );
}
