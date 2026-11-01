import { useState } from "react";

import { cancelJob } from "@/api/client";
import { Button } from "@/components/ui/button";
import type { JobResponse } from "@/types/generated/JobResponse";

export const isActive = (job: JobResponse) => job.status === "queued" || job.status === "running";

export function progressText(job: JobResponse): string {
  if (job.status === "queued") return "Waiting for a free slot";
  const { progress } = job;
  if (!progress) return "Starting";
  const verb = { commits: "Read", files: "Parsed", cases: "Evaluated" }[progress.unit];
  return `${verb} ${progress.done.toLocaleString("en-US")} of ${progress.total.toLocaleString("en-US")} ${progress.unit}`;
}

/** Progress of a running job, with a way to stop it. */
export function JobActivity({ job }: { job: JobResponse }) {
  const [stopping, setStopping] = useState(false);
  const [failed, setFailed] = useState(false);

  async function stop() {
    setStopping(true);
    setFailed(false);
    try {
      await cancelJob(job.id);
    } catch {
      setStopping(false);
      setFailed(true);
    }
  }

  return (
    <span className="flex items-center gap-3">
      <span role="status" className="font-mono text-xs text-muted-foreground">
        {stopping ? "Stopping…" : progressText(job)}
      </span>
      <Button type="button" variant="ghost" disabled={stopping} onClick={() => void stop()}>
        Cancel
      </Button>
      {failed && (
        <span role="alert" className="text-xs text-destructive">
          Could not cancel. Is the API running?
        </span>
      )}
    </span>
  );
}
