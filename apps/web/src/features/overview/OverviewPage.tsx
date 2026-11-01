import { ApiRequestError } from "@/api/client";
import { useOverview } from "@/api/queries";
import { useHealth } from "@/api/useHealth";
import type { SelectedRepository } from "@/app/useSelectedRepository";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import type { HealthResponse } from "@/types/generated/HealthResponse";

import { OpenRepositoryForm } from "./OpenRepositoryForm";
import { RecentJobs } from "./RecentJobs";
import { RepositoryOverview } from "./RepositoryOverview";

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="font-mono text-sm">{value}</dd>
    </div>
  );
}

function EngineFacts({ health }: { health: HealthResponse }) {
  const { database } = health;
  const schema =
    database.schema_version === null
      ? "unavailable"
      : `${database.schema_version} of ${database.expected_schema_version}`;
  return (
    <dl className="grid gap-x-10 sm:grid-cols-2">
      <Fact label="Version" value={health.version} />
      <Fact label="API" value={health.api_version} />
      <Fact label="Database" value={database.reachable ? "reachable" : "unreachable"} />
      <Fact label="Schema" value={schema} />
      <Fact label="Parser version" value={String(health.parser_version)} />
      <Fact label="Algorithm version" value={String(health.algorithm_version)} />
    </dl>
  );
}

function EngineCard() {
  const { data, isPending, error } = useHealth();
  const degraded = data?.status === "degraded";

  return (
    <Card>
      <CardHeader>
        <CardTitle>Local engine</CardTitle>
        {data && (
          <span className={cn("font-mono text-xs", degraded ? "text-warning" : "text-success")}>{data.status}</span>
        )}
      </CardHeader>
      <CardContent>
        {data && <EngineFacts health={data} />}
        {isPending && (
          <div aria-label="Loading engine status" className="space-y-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-2/3" />
          </div>
        )}
        {error && !data && (
          <div role="alert">
            <p className="text-sm font-medium text-destructive">{error.message}</p>
            <p className="mt-2 text-sm text-muted-foreground">Start it from the repository root:</p>
            <pre className="mt-2 rounded-md bg-muted px-3 py-2 font-mono text-sm">cargo run -p atlas-cli -- serve</pre>
          </div>
        )}
        {degraded && (
          <p role="alert" className="mt-3 text-sm text-warning">
            The server is running, but its database is not usable at the expected schema version.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function RepositorySection({ repository, select }: SelectedRepository) {
  const overview = useOverview(repository?.id);
  const notAnalyzed = overview.error instanceof ApiRequestError && overview.error.code === "not_found";

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Repository</CardTitle>
        </CardHeader>
        <CardContent>
          <OpenRepositoryForm initialPath={repository?.path ?? ""} onAnalyzed={select} />
          {notAnalyzed && <p className="mt-3 text-sm text-muted-foreground">This repository has not been analyzed yet.</p>}
          {overview.error && !notAnalyzed && (
            <p role="alert" className="mt-3 text-sm text-destructive">
              {overview.error.message}
            </p>
          )}
        </CardContent>
      </Card>
      {overview.isLoading && (
        <div aria-label="Loading overview" className="space-y-3">
          <Skeleton className="h-20 w-full" />
          <Skeleton className="h-48 w-full" />
        </div>
      )}
      {repository && <RecentJobs repositoryId={repository.id} />}
      {overview.data && <RepositoryOverview overview={overview.data} />}
    </>
  );
}

export function OverviewPage({ selection }: { selection: SelectedRepository }) {
  return (
    <div className="mx-auto max-w-4xl space-y-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Overview</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          See which files to inspect when you change one, and the Git and import evidence behind each suggestion.
        </p>
      </div>
      <RepositorySection {...selection} />
      <EngineCard />
    </div>
  );
}
