import { useRepositoryJobs } from "@/api/queries";
import { JobActivity, isActive } from "@/components/JobActivity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

const KIND = { analysis: "Analysis", backtest: "Backtest" } as const;
const OUTCOME = { completed: "Finished", failed: "Failed", cancelled: "Cancelled" } as const;

/** Jobs of this server run, so a job started elsewhere can be watched and stopped. */
export function RecentJobs({ repositoryId }: { repositoryId: number }) {
  const { data } = useRepositoryJobs(repositoryId);
  if (!data || data.length === 0) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent jobs</CardTitle>
      </CardHeader>
      <CardContent>
        <ul aria-label="Recent jobs" className="space-y-1.5">
          {data.map((job) => (
            <li key={job.id} className="flex flex-wrap items-center justify-between gap-3 text-sm">
              <span>{KIND[job.kind]}</span>
              {isActive(job) ? (
                <JobActivity job={job} />
              ) : (
                <span
                  title={job.error ?? undefined}
                  className={job.status === "failed" ? "text-xs text-destructive" : "text-xs text-muted-foreground"}
                >
                  {OUTCOME[job.status as keyof typeof OUTCOME]}
                  {job.error ? `: ${job.error}` : ""}
                </span>
              )}
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
