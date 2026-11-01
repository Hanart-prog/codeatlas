import { useRepositories } from "@/api/queries";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

const LISTED = 6;

const folderName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

interface Props {
  current: RepositoryEntry | undefined;
  onOpen: (id: number) => void;
}

/** Repositories with saved results, newest first, which open without analyzing again. */
export function RecentRepositories({ current, onOpen }: Props) {
  const { data } = useRepositories();
  const recent = (data ?? [])
    .filter((entry) => entry.analyzed_at !== null && entry.id !== current?.id)
    .sort((a, b) => (b.analyzed_at ?? 0) - (a.analyzed_at ?? 0))
    .slice(0, LISTED);
  if (recent.length === 0) return null;

  return (
    <div className="mt-4">
      <h3 className="text-xs text-muted-foreground">Recent repositories</h3>
      <ul aria-label="Recent repositories" className="mt-1.5 grid gap-1 sm:grid-cols-2">
        {recent.map((entry) => (
          <li key={entry.id} className="min-w-0">
            <button
              type="button"
              title={entry.path}
              onClick={() => onOpen(entry.id)}
              className="block w-full min-w-0 rounded-md border px-3 py-1.5 text-left hover:bg-muted"
            >
              <span className="block truncate text-sm font-medium">{folderName(entry.path)}</span>
              <span className="block truncate font-mono text-xs text-muted-foreground">{entry.path}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
