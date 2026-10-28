import { useCallback, useState } from "react";

import { resolveNavEntry } from "./registry";

// Per-browser convenience, like the market scope recents: an unscoped key.
const STORAGE_KEY = "iskworks:nav-recents";
const MAX_RECENTS = 5;

function readRecents(): string[] {
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((path): path is string => typeof path === "string") : [];
  } catch {
    return [];
  }
}

function writeRecents(recents: string[]): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(recents));
  } catch {
    // Storage unavailable -- recents are a convenience, kept in memory only.
  }
}

/**
 * The last five registry pages visited, newest first, for the command
 * palette's empty state. A visit records the most specific page the location
 * belongs to, so /builds/123 counts as Builds.
 */
export function useNavRecents(): { recents: string[]; recordVisit: (pathname: string) => void } {
  const [recents, setRecents] = useState(readRecents);

  const recordVisit = useCallback((pathname: string) => {
    const path = resolveNavEntry(pathname)?.path;
    if (!path) return;
    setRecents((current) => {
      if (current[0] === path) return current;
      const next = [path, ...current.filter((existing) => existing !== path)].slice(0, MAX_RECENTS);
      writeRecents(next);
      return next;
    });
  }, []);

  return { recents, recordVisit };
}
