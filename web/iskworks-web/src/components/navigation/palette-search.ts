import type { LucideIcon } from "lucide-react";

import { adminNavItem, navGroups, type NavItem } from "./registry";

/** Something the command palette can open (a page) or run (an action). */
export interface PaletteEntry {
  id: string;
  kind: "page" | "action";
  /** As shown: "Market Orders", or "Builds › Blueprints" for a sub-page. */
  label: string;
  /** Shown right-aligned: the nav group, or "Actions". */
  group: string;
  icon: LucideIcon;
  keywords: string[];
  path?: string;
  run?: () => void;
}

export interface PaletteMatch {
  entry: PaletteEntry;
  score: number;
  /** Indices into `entry.label` to highlight. */
  matched: number[];
  /** True when a keyword, not the label, is what matched best. */
  viaKeyword: boolean;
}

function itemEntries(item: NavItem, group: string): PaletteEntry[] {
  return [
    { id: `page:${item.path}`, kind: "page", label: item.label, group, icon: item.icon, keywords: item.keywords ?? [], path: item.path },
    ...(item.children ?? []).map((child) => ({
      id: `page:${child.path}`,
      kind: "page" as const,
      label: `${item.label} › ${child.label}`,
      group,
      // A sub-page shares its parent's icon so one icon never means two pages.
      icon: item.icon,
      keywords: child.keywords ?? [],
      path: child.path,
    })),
  ];
}

/** Every page in registry order, sub-pages after their parent; Admin last, for admins. */
export function pageEntries(isAdmin: boolean): PaletteEntry[] {
  const pages = navGroups.flatMap((group) => group.items.flatMap((item) => itemEntries(item, group.label)));
  return isAdmin ? [...pages, ...itemEntries(adminNavItem, "Admin")] : pages;
}

const range = (start: number, length: number) => Array.from({ length }, (_, index) => start + index);

/** How well the label matches, best first: exact, prefix, a word's prefix, word initials, substring, subsequence. */
function matchLabel(label: string, query: string): { score: number; matched: number[] } | null {
  const lower = label.toLowerCase();
  if (lower === query) return { score: 400, matched: range(0, label.length) };
  if (lower.startsWith(query)) return { score: 300, matched: range(0, query.length) };

  const words = [...lower.matchAll(/[a-z0-9]+/g)].map((word) => ({ text: word[0], start: word.index ?? 0 }));
  const word = words.find((candidate) => candidate.text.startsWith(query));
  if (word) return { score: 250, matched: range(word.start, query.length) };

  // Word initials, in order: "mo" -> Market Orders.
  const initials: number[] = [];
  let next = 0;
  for (const char of query) {
    while (next < words.length && words[next].text[0] !== char) next += 1;
    if (next === words.length) break;
    initials.push(words[next].start);
    next += 1;
  }
  if (initials.length === query.length) return { score: 200, matched: initials };

  const at = lower.indexOf(query);
  if (at >= 0) return { score: 150, matched: range(at, query.length) };

  const subsequence: number[] = [];
  let from = 0;
  for (const char of query) {
    const found = lower.indexOf(char, from);
    if (found < 0) return null;
    subsequence.push(found);
    from = found + 1;
  }
  return { score: 100, matched: subsequence };
}

function matchKeywords(keywords: string[], query: string): number {
  if (keywords.includes(query)) return 350;
  return keywords.some((keyword) => keyword.startsWith(query)) ? 275 : 0;
}

/** Entries matching `query`, best first; ties keep the entries' order. */
export function searchPalette(entries: PaletteEntry[], query: string): PaletteMatch[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];

  const matches: PaletteMatch[] = [];
  for (const entry of entries) {
    const label = matchLabel(entry.label, needle);
    const labelScore = label?.score ?? 0;
    const keywordScore = matchKeywords(entry.keywords, needle);
    // A group name lists the group's pages, below anything matched directly.
    const groupScore = entry.group.toLowerCase().startsWith(needle) ? 50 : 0;
    const score = Math.max(labelScore, keywordScore, groupScore);
    if (score === 0) continue;
    matches.push({ entry, score, matched: label?.matched ?? [], viaKeyword: keywordScore > labelScore });
  }
  return matches.sort((a, b) => b.score - a.score);
}
