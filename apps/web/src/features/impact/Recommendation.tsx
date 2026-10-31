import type { ImpactRecommendation } from "@/types/generated/ImpactRecommendation";

function formatShare(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function ScoreRow({ label, value, weight }: { label: string; value: number; weight: string }) {
  return (
    <div className="flex justify-between gap-4 text-xs">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono">
        {value.toFixed(2)} <span className="text-muted-foreground">{weight}</span>
      </dd>
    </div>
  );
}

export function Recommendation({ item }: { item: ImpactRecommendation }) {
  const { static_evidence: imports, historical_evidence: history } = item;
  return (
    <li className="rounded-lg border bg-card p-4">
      <div className="flex items-baseline gap-3">
        <span className="font-mono text-xs text-muted-foreground">#{item.rank}</span>
        <span className="min-w-0 flex-1 break-all font-mono text-sm">{item.path}</span>
        <span className="font-mono text-sm" title="Ranking score, not a probability">
          {item.score.toFixed(3)}
        </span>
      </div>

      <dl className="mt-3 space-y-0.5">
        <ScoreRow label="Import evidence" value={item.static_score} weight="1 / hops" />
        <ScoreRow label="History evidence" value={item.historical_score} weight="P(this | selected)" />
      </dl>

      {imports && (
        <section className="mt-3">
          <h3 className="text-xs font-medium">
            Imports, {imports.distance} {imports.distance === 1 ? "hop" : "hops"}
          </h3>
          <p className="mt-0.5 break-all font-mono text-xs text-muted-foreground">{imports.path.join(" → ")}</p>
        </section>
      )}

      {history && (
        <section className="mt-3">
          <h3 className="text-xs font-medium">Changed together with {history.selected}</h3>
          <p className="mt-0.5 text-xs text-muted-foreground">
            <span>
              {history.shared_commit_count} of {history.selected_commit_count} commits
            </span>
            {` · changes in ${formatShare(history.base_rate)} of all commits · lift ${history.lift.toFixed(1)} · Jaccard ${history.jaccard.toFixed(2)}`}
          </p>
          <ul className="mt-1 space-y-0.5">
            {history.commits.map((commit) => (
              <li key={commit.oid} className="flex gap-2 text-xs">
                <span className="shrink-0 font-mono text-muted-foreground">{commit.oid.slice(0, 10)}</span>
                <span className="min-w-0 truncate" title={commit.subject}>
                  {commit.subject}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {item.caveats.length > 0 && (
        <ul className="mt-3 space-y-0.5 text-xs text-muted-foreground">
          {item.caveats.map((caveat) => (
            <li key={caveat}>{caveat}</li>
          ))}
        </ul>
      )}
    </li>
  );
}
