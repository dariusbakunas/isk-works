import { Check, ChevronDown, Users } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { AnalyticsGranularity } from "../../../api/finance-analytics";
import type { FinanceCharacter } from "../../../api/finance";
import { CharacterName, PRIVATE_ATTR } from "../../../observability/private";
import { Segment } from "../../calendar/calendar-filters";
import { FinanceDateField } from "../finance-filter-controls";
import type { AnalyticsUrlState, RangePreset } from "./analytics-url-state";

const PRESET_OPTIONS = [
  ["7d", "7d", "Last 7 days"],
  ["30d", "30d", "Last 30 days"],
  ["90d", "90d", "Last 90 days"],
  ["ytd", "YTD", "Year to date"],
  ["custom", "Custom", "Custom date range"],
] as const;

const GRANULARITY_OPTIONS = [
  ["day", "Day", "Group by day"],
  ["week", "Week", "Group by week"],
  ["month", "Month", "Group by month"],
] as const;

/** Character swatches reuse the category palette, by position in the roster. */
export function characterColor(index: number): string {
  return `var(--color-cat-${(index % 8) + 1})`;
}

export function AnalyticsHeader({
  state,
  range,
  granularity,
  characters,
  onChange,
}: {
  state: AnalyticsUrlState;
  /** The resolved window, used to seed a custom range. */
  range: { dateFrom: string; dateTo: string };
  /** The effective granularity (the chosen one, or the automatic one). */
  granularity: AnalyticsGranularity;
  characters: FinanceCharacter[];
  onChange: (patch: Partial<AnalyticsUrlState>) => void;
}) {
  function choosePreset(preset: RangePreset) {
    if (preset === "custom") {
      onChange({ preset, dateFrom: state.dateFrom ?? range.dateFrom, dateTo: state.dateTo ?? range.dateTo });
    } else {
      onChange({ preset, dateFrom: null, dateTo: null });
    }
  }

  return (
    <header className="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 className="text-xl font-bold leading-6 text-foreground">Analytics</h2>
        <p className="text-xs text-muted">Where your ISK goes</p>
      </div>
      <div className="flex flex-wrap items-end gap-2">
        <Segment label="Date range" onChange={choosePreset} options={PRESET_OPTIONS} value={state.preset} />
        {state.preset === "custom" ? (
          <>
            <FinanceDateField
              id="analytics-date-from"
              label="From"
              onChange={(value) => value && value <= (state.dateTo ?? value) && onChange({ dateFrom: value })}
              value={state.dateFrom ?? range.dateFrom}
            />
            <FinanceDateField
              id="analytics-date-to"
              label="To"
              onChange={(value) => value && value >= (state.dateFrom ?? value) && onChange({ dateTo: value })}
              value={state.dateTo ?? range.dateTo}
            />
          </>
        ) : null}
        <button
          aria-pressed={state.compare}
          className="iw-compare-toggle"
          onClick={() => onChange({ compare: !state.compare })}
          type="button"
        >
          <span aria-hidden="true" className="iw-compare-toggle-track" data-on={state.compare} />
          Compare prev
        </button>
        <button
          aria-pressed={state.excludeInventory}
          className="iw-compare-toggle"
          onClick={() => onChange({ excludeInventory: !state.excludeInventory })}
          title="Leave out purchases already recorded into Inventory; they are build inputs, not spend"
          type="button"
        >
          <span aria-hidden="true" className="iw-compare-toggle-track" data-on={state.excludeInventory} />
          Exclude inventory buys
        </button>
        <CharacterSelect characters={characters} onChange={(selected) => onChange({ characters: selected })} selected={state.characters} />
        <Segment label="Granularity" onChange={(value) => onChange({ granularity: value })} options={GRANULARITY_OPTIONS} value={granularity} />
      </div>
    </header>
  );
}

function CharacterSelect({
  characters,
  selected,
  onChange,
}: {
  characters: FinanceCharacter[];
  selected: string[];
  onChange: (connectionIds: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const single = selected.length === 1 ? characters.find(({ connectionId }) => connectionId === selected[0]) : undefined;
  const summary = selected.length === 0
    ? `All ${characters.length} chars`
    : single?.characterName ?? `${selected.length} characters`;

  function toggle(connectionId: string) {
    // From "all", picking one narrows to just that one; emptying goes back to all.
    const next = selected.length === 0
      ? [connectionId]
      : selected.includes(connectionId)
        ? selected.filter((id) => id !== connectionId)
        : [...selected, connectionId];
    onChange(next.length === characters.length ? [] : next);
  }

  return (
    <div className="relative" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Filter by character: ${summary}`}
        className="iw-calendar-character-trigger"
        disabled={characters.length === 0}
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        <span aria-hidden="true" className="flex -space-x-1.5">
          {characters
            .map((character, index) => ({ character, index }))
            .filter(({ character }) => selected.length === 0 || selected.includes(character.connectionId))
            .slice(0, 4)
            .map(({ character, index }) => (
              <span
                className="grid h-4 w-4 place-items-center rounded-full border border-panel-strong text-[0.5rem] font-bold text-background"
                key={character.connectionId}
                style={{ background: characterColor(index) }}
              >
                {character.characterName.charAt(0)}
              </span>
            ))}
          {characters.length === 0 ? <Users className="h-3 w-3" /> : null}
        </span>
        <span className="max-w-32 truncate" {...PRIVATE_ATTR}>{summary}</span>
        <ChevronDown aria-hidden="true" className="h-3 w-3" />
      </button>
      {open ? (
        <div className="iw-calendar-character-menu" role="menu">
          <button className="iw-calendar-character-option" onClick={() => onChange([])} role="menuitem" type="button">
            <span className="w-4">{selected.length === 0 ? <Check aria-hidden="true" className="h-3 w-3 text-primary" /> : null}</span>
            All characters
          </button>
          {characters.map((character, index) => {
            const checked = selected.length === 0 || selected.includes(character.connectionId);
            return (
              <button
                aria-checked={checked}
                aria-label={character.characterName}
                className="iw-calendar-character-option"
                key={character.connectionId}
                onClick={() => toggle(character.connectionId)}
                role="menuitemcheckbox"
                type="button"
              >
                <span className="w-4">{checked ? <Check aria-hidden="true" className="h-3 w-3 text-primary" /> : null}</span>
                <span aria-hidden="true" className="h-2 w-2 rounded-full" style={{ background: characterColor(index) }} />
                <CharacterName className="truncate" name={character.characterName} />
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
