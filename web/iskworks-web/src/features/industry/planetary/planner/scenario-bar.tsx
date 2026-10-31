import { useEffect, useState } from "react";
import { Minus, Plus, Search, X } from "lucide-react";

import { searchSolarSystems, type SolarSystemSearchResult } from "../../../../api/industry";
import { getPlannerCharacters, INTERVAL_DAYS, MAX_RADIUS, type PlannerSystem } from "../../../../api/pi-planner";
import { useDebouncedLookup } from "../../../../hooks/use-debounced-lookup";
import { apiMessage } from "../../inventory/shared";
import { SecurityValue } from "./planets-panel";
import {
  clampLevel,
  colonies,
  MAX_SKILL_LEVEL,
  percentRate,
  ratePercent,
  type ScenarioCharacter,
  type ScenarioSettings,
} from "./planner-format";

function Segmented<T extends number>({
  label,
  onChange,
  options,
  title,
  value,
  format = String,
}: {
  label: string;
  onChange: (value: T) => void;
  options: readonly T[];
  title?: string;
  value: T;
  format?: (value: T) => string;
}) {
  return (
    <div className="min-w-0" title={title}>
      <span className="iw-eyebrow mb-1 block text-muted">{label}</span>
      <div aria-label={label} className="flex max-w-full overflow-x-auto" role="radiogroup">
        {options.map((option) => (
          <button
            aria-checked={option === value}
            className={`h-8 min-w-8 shrink-0 border px-2 text-xs ${
              option === value
                ? "border-primary/60 bg-primary/15 text-primary"
                : "border-border bg-background text-muted hover:text-foreground"
            }`}
            key={option}
            onClick={() => onChange(option)}
            role="radio"
            type="button"
          >
            {format(option)}
          </button>
        ))}
      </div>
    </div>
  );
}

function Stepper({ label, onChange, value }: { label: string; onChange: (value: number) => void; value: number }) {
  return (
    <span className="inline-flex items-center gap-0.5" title={label}>
      <span className="text-[0.65rem] text-muted">{label.split(" ").map((word) => word[0]).join("")}</span>
      <button
        aria-label={`Lower ${label}`}
        className="grid h-6 w-5 place-items-center rounded-sm border border-border text-muted hover:text-foreground disabled:opacity-40"
        disabled={value <= 0}
        onClick={() => onChange(clampLevel(value - 1))}
        type="button"
      >
        <Minus aria-hidden="true" className="h-3 w-3" />
      </button>
      <span aria-label={label} className="w-4 text-center font-mono text-xs" role="status">
        {value}
      </span>
      <button
        aria-label={`Raise ${label}`}
        className="grid h-6 w-5 place-items-center rounded-sm border border-border text-muted hover:text-foreground disabled:opacity-40"
        disabled={value >= MAX_SKILL_LEVEL}
        onClick={() => onChange(clampLevel(value + 1))}
        type="button"
      >
        <Plus aria-hidden="true" className="h-3 w-3" />
      </button>
    </span>
  );
}

function SystemPicker({
  home,
  onPick,
}: {
  home: PlannerSystem | null;
  onPick: (system: SolarSystemSearchResult) => void;
}) {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const lookup = useDebouncedLookup(query, searchSolarSystems, (requestError) => setError(apiMessage(requestError)), {
    enabled: open,
  });
  return (
    <div className="relative">
      <span className="iw-eyebrow mb-1 block text-muted">Home system</span>
      <label className="relative block">
        <Search aria-hidden="true" className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
        <input
          aria-label="Home system"
          className="iw-input min-h-8 w-64 pl-7 text-sm"
          onBlur={() => window.setTimeout(() => setOpen(false), 150)}
          onChange={(event) => {
            setQuery(event.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          placeholder={home ? home.name : "Search a solar system"}
          value={query}
        />
        {home && !query ? (
          <span className="pointer-events-none absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-1.5 text-xs">
            <SecurityValue value={home.securityStatus} />
            <span className="text-muted">{home.regionName}</span>
          </span>
        ) : null}
      </label>
      {open && lookup.results.length > 0 ? (
        <ul className="absolute z-30 mt-1 max-h-64 w-64 overflow-auto rounded-md border border-border bg-panel-strong shadow-xl" role="listbox">
          {lookup.results.map((system) => (
            <li key={system.solarSystemId}>
              <button
                className="flex w-full items-center justify-between px-2 py-1.5 text-left text-sm hover:bg-primary/10"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  onPick(system);
                  setQuery("");
                  setOpen(false);
                }}
                role="option"
                aria-selected={home?.solarSystemId === system.solarSystemId}
                type="button"
              >
                <span>{system.solarSystemName}</span>
                <span className="text-xs text-muted">{system.securityClass}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? <p className="mt-1 text-xs text-destructive">{error}</p> : null}
    </div>
  );
}

/** The scenario: where, who, and how often the player tends colonies. */
export function ScenarioBar({
  characters,
  counts,
  home,
  onCharactersChange,
  onSettingsChange,
  settings,
}: {
  characters: ScenarioCharacter[];
  /** In range of the home system, once loaded. */
  counts: { systems: number; planets: number } | null;
  home: PlannerSystem | null;
  onCharactersChange: (characters: ScenarioCharacter[]) => void;
  onSettingsChange: (settings: ScenarioSettings) => void;
  settings: ScenarioSettings;
}) {
  const [taxText, setTaxText] = useState(ratePercent(settings.defaultCustomsRate));
  useEffect(() => setTaxText(ratePercent(settings.defaultCustomsRate)), [settings.defaultCustomsRate]);
  const [prefillMessage, setPrefillMessage] = useState<string | null>(null);
  const set = (patch: Partial<ScenarioSettings>) => onSettingsChange({ ...settings, ...patch });
  const setCharacter = (index: number, patch: Partial<ScenarioCharacter>) =>
    onCharactersChange(characters.map((character, i) => (i === index ? { ...character, ...patch } : character)));

  const prefill = async () => {
    try {
      const connected = await getPlannerCharacters();
      if (connected.length === 0) {
        setPrefillMessage("No connected characters.");
        return;
      }
      setPrefillMessage(null);
      onCharactersChange(
        connected.map((character) => ({
          name: character.name,
          interplanetaryConsolidation: clampLevel(character.interplanetaryConsolidation ?? 0),
          commandCenterUpgrades: clampLevel(character.commandCenterUpgrades ?? 0),
        })),
      );
    } catch (requestError) {
      setPrefillMessage(apiMessage(requestError));
    }
  };

  return (
    <section aria-label="Scenario" className="iw-panel mb-3 p-3">
      <div className="flex flex-wrap items-end gap-3">
        <SystemPicker home={home} onPick={(system) => set({ homeSystemId: system.solarSystemId })} />
        <Segmented
          label="Radius"
          onChange={(radius) => set({ radius })}
          options={Array.from({ length: MAX_RADIUS + 1 }, (_, n) => n)}
          title="Gate jumps from the home system"
          value={settings.radius}
        />
        {counts ? (
          <span className="pb-2 text-xs text-muted">
            {counts.systems} {counts.systems === 1 ? "system" : "systems"} · {counts.planets}{" "}
            {counts.planets === 1 ? "planet" : "planets"}
          </span>
        ) : null}
        <label className="block">
          <span className="iw-eyebrow mb-1 block text-muted">Customs tax</span>
          <span className="relative block">
            <input
              aria-label="Default customs tax percent"
              className="iw-input min-h-8 w-16 pr-5 text-right font-mono text-sm"
              inputMode="decimal"
              onBlur={() => {
                const rate = percentRate(taxText);
                if (rate === null) setTaxText(ratePercent(settings.defaultCustomsRate));
                else set({ defaultCustomsRate: rate });
              }}
              onChange={(event) => setTaxText(event.target.value.replace(/[^\d.]/g, ""))}
              value={taxText}
            />
            <span className="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-xs text-faint">%</span>
          </span>
        </label>
        <Segmented
          format={(days) => `${days}d`}
          label="Reset every"
          onChange={(resetDays) => set({ resetDays })}
          options={INTERVAL_DAYS}
          title="How often you restart extractor programs. Longer programs yield less per hour."
          value={settings.resetDays as (typeof INTERVAL_DAYS)[number]}
        />
        <Segmented
          format={(days) => `${days}d`}
          label="Haul every"
          onChange={(haulDays) => set({ haulDays })}
          options={INTERVAL_DAYS}
          title="How often you move goods between planets and to market. Longer gaps need more storage."
          value={settings.haulDays as (typeof INTERVAL_DAYS)[number]}
        />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-3 text-xs">
        <label
          className="flex items-center gap-1.5"
          title="Move output to the customs office or skyhook at every login and haul from there."
        >
          <input
            checked={settings.upliftAtLogin}
            className="h-3.5 w-3.5"
            onChange={(event) => set({ upliftAtLogin: event.target.checked })}
            type="checkbox"
          />
          Uplift to skyhook at each login
        </label>
        <label className="flex items-center gap-1.5" title="Buy P1 that no planet in range yields at best sell">
          <input
            checked={settings.buyIntermediates}
            className="h-3.5 w-3.5"
            onChange={(event) => set({ buyIntermediates: event.target.checked })}
            type="checkbox"
          />
          Buy intermediates
        </label>
        <label className="flex items-center gap-1.5">
          Max colonies
          <input
            aria-label="Max colonies"
            className="iw-input min-h-7 w-12 text-center font-mono text-xs"
            inputMode="numeric"
            onChange={(event) => {
              const value = Number(event.target.value.replace(/\D/g, ""));
              set({ maxColonies: value > 0 ? value : null });
            }}
            placeholder="∞"
            value={settings.maxColonies ?? ""}
          />
        </label>
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        {characters.map((character, index) => (
          <div
            className="flex h-9 items-center gap-2 rounded-md border border-border bg-background px-2"
            data-testid="scenario-character"
            key={index}
          >
            <span className="min-w-20 truncate text-xs" data-private="">
              {character.name}
            </span>
            <Stepper
              label="Interplanetary Consolidation"
              onChange={(level) => setCharacter(index, { interplanetaryConsolidation: level })}
              value={character.interplanetaryConsolidation}
            />
            <Stepper
              label="Command Center Upgrades"
              onChange={(level) => setCharacter(index, { commandCenterUpgrades: level })}
              value={character.commandCenterUpgrades}
            />
            <span className="text-xs text-muted">
              → {colonies(character)} {colonies(character) === 1 ? "planet" : "planets"}
            </span>
            <button
              aria-label={`Remove ${character.name}`}
              className="text-muted hover:text-foreground disabled:opacity-40"
              disabled={characters.length === 1}
              onClick={() => onCharactersChange(characters.filter((_, i) => i !== index))}
              type="button"
            >
              <X aria-hidden="true" className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
        <button
          className="iw-button-secondary"
          onClick={() =>
            onCharactersChange([
              ...characters,
              { name: `Character ${characters.length + 1}`, interplanetaryConsolidation: 4, commandCenterUpgrades: 4 },
            ])
          }
          type="button"
        >
          <Plus aria-hidden="true" className="h-3.5 w-3.5" />
          <span className="ml-1">Add character</span>
        </button>
        <button className="text-xs text-primary hover:underline" onClick={() => void prefill()} type="button">
          Prefill from connected characters
        </button>
        {prefillMessage ? <span className="text-xs text-muted">{prefillMessage}</span> : null}
      </div>
    </section>
  );
}
