import { type FormEvent, useState } from "react";

import { useAnalyzeRepository } from "@/api/queries";
import { Button } from "@/components/ui/button";

interface Props {
  /** Shown until the user types. Not a remount key: remounting mid-analysis loses its state. */
  initialPath?: string;
  onAnalyzed: (id: number) => void;
}

export function OpenRepositoryForm({ initialPath = "", onAnalyzed }: Props) {
  const [edited, setEdited] = useState<string>();
  const path = edited ?? initialPath;
  const analysis = useAnalyzeRepository(onAnalyzed);

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
          className="h-8 min-w-0 flex-1 rounded-md border bg-background px-3 font-mono text-sm placeholder:text-muted-foreground"
        />
        <Button type="submit" disabled={analysis.isPending || !path.trim()}>
          {analysis.isPending ? "Analyzing…" : "Analyze"}
        </Button>
      </div>
      {analysis.error && (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {analysis.error.message}
        </p>
      )}
    </form>
  );
}
