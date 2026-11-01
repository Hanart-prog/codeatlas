import { type FormEvent, useState } from "react";

import { useAnalyzeRepository } from "@/api/queries";
import { JobActivity } from "@/components/JobActivity";
import { Button } from "@/components/ui/button";
import type { JobResponse } from "@/types/generated/JobResponse";

interface Props {
  /** Shown until the user types. Not a remount key: remounting mid-analysis loses its state. */
  initialPath?: string;
  onAnalyzed: (id: number) => void;
}

export function OpenRepositoryForm({ initialPath = "", onAnalyzed }: Props) {
  const [edited, setEdited] = useState<string>();
  const path = edited ?? initialPath;
  const [job, setJob] = useState<JobResponse>();
  const analysis = useAnalyzeRepository(onAnalyzed, setJob);

  function submit(event: FormEvent) {
    event.preventDefault();
    if (path.trim()) analysis.mutate(path.trim());
  }

  return (
    <form onSubmit={submit}>
      <label htmlFor="repository-path" className="text-sm text-muted-foreground">
        Absolute path of a local Git repository
      </label>
      <div className="mt-2 flex gap-2">
        <input
          id="repository-path"
          value={path}
          onChange={(event) => setEdited(event.target.value)}
          placeholder="/path/to/repository"
          spellCheck={false}
          autoComplete="off"
          className="h-9 min-w-0 flex-1 rounded-md border bg-card px-3 font-mono text-sm placeholder:text-muted-foreground"
        />
        <Button type="submit" disabled={analysis.isPending || !path.trim()}>
          {analysis.isPending ? "Analyzing…" : "Analyze"}
        </Button>
      </div>
      {analysis.isPending && job && (
        <div className="mt-2">
          <JobActivity key={job.id} job={job} />
        </div>
      )}
      {analysis.error && (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {analysis.error.message}
        </p>
      )}
    </form>
  );
}
