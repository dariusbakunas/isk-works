import { Search } from "lucide-react";
import { Fragment, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { useLocation, useNavigate } from "react-router";

import { pageEntries, searchPalette, type PaletteEntry, type PaletteMatch } from "./palette-search";
import { resolveNavEntry } from "./registry";

interface Section {
  heading: string;
  /** Printed once above this section ("All pages" above its group sections). */
  overline?: string;
  /** A rule above the heading, between top-level sections. */
  divider?: boolean;
  matches: PaletteMatch[];
}

const plain = (entry: PaletteEntry): PaletteMatch => ({ entry, score: 0, matched: [], viaKeyword: false });

function emptySections(pages: PaletteEntry[], recents: string[], actions: PaletteEntry[], currentPath?: string): Section[] {
  const byPath = new Map(pages.map((entry) => [entry.path, entry]));
  // The page you're on is never somewhere to go back to.
  const recent = recents.filter((path) => path !== currentPath).flatMap((path) => byPath.get(path) ?? []);
  // The sitemap lists top-level pages; sub-pages are a search away.
  const topLevel = pages.filter((entry) => !entry.label.includes(" › "));
  const groups = [...new Set(topLevel.map((entry) => entry.group))];
  return [
    ...(recent.length ? [{ heading: "Recent", matches: recent.map(plain) }] : []),
    ...(actions.length ? [{ heading: "Actions", divider: recent.length > 0, matches: actions.map(plain) }] : []),
    ...groups.map((group, index) => ({
      heading: group,
      overline: index === 0 ? "All pages" : undefined,
      matches: topLevel.filter((entry) => entry.group === group).map(plain),
    })),
  ];
}

function Highlighted({ label, matched }: { label: string; matched: number[] }) {
  if (!matched.length) return <>{label}</>;
  const hits = new Set(matched);
  return (
    <>
      {[...label].map((char, index) => (
        <span className={hits.has(index) ? "font-semibold text-bright" : "text-muted"} key={index}>
          {char}
        </span>
      ))}
    </>
  );
}

const headingClass = "flex h-6 items-center px-3 text-[8px] uppercase tracking-[.12em] text-subtle";

/**
 * ⌘K: jump to any page or run an action by typing. Empty, it lists recent
 * pages, actions and every page by group, so it doubles as a sitemap.
 */
export function CommandPalette({
  isAdmin,
  recents,
  actions,
  onClose,
}: {
  isAdmin: boolean;
  recents: string[];
  actions: PaletteEntry[];
  onClose: () => void;
}) {
  const navigate = useNavigate();
  const currentPath = resolveNavEntry(useLocation().pathname)?.path;
  const listId = useId();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  const pages = useMemo(() => pageEntries(isAdmin), [isAdmin]);
  const sections = useMemo<Section[]>(() => {
    if (!query.trim()) return emptySections(pages, recents, actions, currentPath);
    const matches = searchPalette([...pages, ...actions], query);
    if (!matches.length) return [];
    return [{ heading: matches.every((match) => match.entry.kind === "action") ? "Actions" : "Results", matches }];
  }, [actions, currentPath, pages, query, recents]);
  const flat = sections.flatMap((section) => section.matches);

  // Give focus back to whatever opened the palette (a button, or the page for ⌘K).
  useLayoutEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    input.current?.focus();
    return () => previous?.focus?.();
  }, []);

  useEffect(() => setActive(0), [query]);

  useEffect(() => {
    document.getElementById(`${listId}-${active}`)?.scrollIntoView?.({ block: "nearest" });
  }, [active, listId]);

  function choose(match: PaletteMatch | undefined, newTab = false) {
    if (!match) return;
    const { entry } = match;
    if (entry.kind === "page" && entry.path) {
      if (newTab) window.open(entry.path, "_blank", "noopener");
      else navigate(entry.path);
    }
    onClose();
    if (entry.kind === "action") entry.run?.();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    const last = flat.length - 1;
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setActive((current) => (current >= last ? 0 : current + 1));
        break;
      case "ArrowUp":
        event.preventDefault();
        setActive((current) => (current <= 0 ? last : current - 1));
        break;
      case "Home":
        event.preventDefault();
        setActive(0);
        break;
      case "End":
        event.preventDefault();
        setActive(last);
        break;
      case "Enter":
        event.preventDefault();
        choose(flat[active], event.metaKey || event.ctrlKey);
        break;
      case "Escape":
        event.preventDefault();
        onClose();
        break;
    }
  }

  let optionIndex = -1;

  return createPortal(
    <div
      className="fixed inset-0 z-[80] flex items-start justify-center bg-[rgba(2,4,7,.76)] pt-[12vh] max-md:pt-0"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        aria-label="Command palette"
        aria-modal="true"
        className="flex max-h-[74vh] w-[560px] flex-col border border-border bg-[#11161d] shadow-2xl max-md:h-full max-md:max-h-none max-md:w-full"
        role="dialog"
      >
        <div className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
          <Search aria-hidden="true" className="h-[17px] w-[17px] text-muted" />
          <input
            aria-activedescendant={flat.length ? `${listId}-${active}` : undefined}
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded="true"
            aria-label="Go to page or run a command"
            className="flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-subtle"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Go to page or run a command…"
            ref={input}
            role="combobox"
            value={query}
          />
          <kbd className="rounded-[2px] border border-border bg-background px-1 font-mono text-[8px] text-subtle">Esc</kbd>
        </div>
        <div className="min-h-0 flex-1 overflow-auto py-2" id={listId} role="listbox">
          {flat.length === 0 ? (
            <div className="grid h-40 place-items-center text-[10px] text-subtle">No page matches “{query.trim()}”</div>
          ) : (
            sections.map((section) => (
              <Fragment key={`${section.overline ?? ""}${section.heading}`}>
                {section.overline ? (
                  <div className={`${headingClass} mt-1 border-t border-border`} role="presentation">
                    {section.overline}
                  </div>
                ) : null}
                <div className={`${headingClass} ${section.divider ? "mt-1 border-t border-border" : ""}`} role="presentation">
                  {section.heading}
                </div>
                {section.matches.map((match) => {
                  optionIndex += 1;
                  const index = optionIndex;
                  const selected = index === active;
                  const Icon = match.entry.icon;
                  return (
                    <div
                      aria-selected={selected}
                      className={`relative flex h-8 cursor-pointer items-center gap-2 px-3 text-[10px] ${selected ? "bg-hover" : "hover:bg-hover"}`}
                      id={`${listId}-${index}`}
                      key={match.entry.id}
                      onClick={() => choose(match)}
                      onMouseMove={() => setActive(index)}
                      role="option"
                    >
                      {selected ? <span aria-hidden="true" className="absolute inset-y-0 left-0 w-0.5 bg-primary" /> : null}
                      <Icon aria-hidden="true" className={`h-3.5 w-3.5 shrink-0 ${selected ? "text-primary" : "text-muted"}`} />
                      <span className="truncate text-foreground">
                        <Highlighted label={match.entry.label} matched={match.matched} />
                      </span>
                      {match.viaKeyword ? (
                        <span className="shrink-0 border border-border px-1 text-[7px] text-subtle">matches: {query.trim().toLowerCase()}</span>
                      ) : null}
                      <span className="ml-auto shrink-0 text-[8px] text-subtle">{match.entry.group}</span>
                    </div>
                  );
                })}
              </Fragment>
            ))
          )}
        </div>
        <div className="flex h-8 shrink-0 items-center border-t border-border px-3 font-mono text-[8px] text-subtle">
          ↑↓ navigate · ↵ open · ⌘↵ new tab · esc close
        </div>
      </div>
    </div>,
    document.body,
  );
}
