import { useCallback, useState } from "react";

import type { NavGroup } from "./registry";

type GroupId = NavGroup["id"];

// Per-browser convenience, like the rail pin: an unscoped key.
const STORAGE_KEY = "iskworks:nav-collapsed-groups";

function readCollapsed(): Set<GroupId> {
  try {
    const parsed: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]");
    return new Set(Array.isArray(parsed) ? parsed.filter((id): id is GroupId => typeof id === "string") : []);
  } catch {
    return new Set();
  }
}

function writeCollapsed(collapsed: Set<GroupId>): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([...collapsed]));
  } catch {
    // Storage unavailable -- collapsing still works for this page load.
  }
}

/** Which rail groups are collapsed, remembered per browser. */
export function useNavCollapsedGroups(): [collapsed: ReadonlySet<GroupId>, toggleGroup: (id: GroupId) => void] {
  const [collapsed, setCollapsed] = useState(readCollapsed);

  const toggleGroup = useCallback((id: GroupId) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      writeCollapsed(next);
      return next;
    });
  }, []);

  return [collapsed, toggleGroup];
}
