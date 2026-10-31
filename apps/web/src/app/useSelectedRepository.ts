import { useCallback, useState } from "react";

import { useRepositories } from "@/api/queries";
import type { RepositoryEntry } from "@/types/generated/RepositoryEntry";

const STORAGE_KEY = "codeatlas-repository";

export interface SelectedRepository {
  repository: RepositoryEntry | undefined;
  select: (id: number) => void;
}

export function useSelectedRepository(): SelectedRepository {
  const { data } = useRepositories();
  const [id, setId] = useState(() => Number(localStorage.getItem(STORAGE_KEY)));

  const select = useCallback((next: number) => {
    localStorage.setItem(STORAGE_KEY, String(next));
    setId(next);
  }, []);

  return { repository: data?.find((entry) => entry.id === id) ?? data?.[0], select };
}
