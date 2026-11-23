import { AlertTriangle, FlaskConical } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Link, useSearchParams } from "react-router";

import { listCharacters } from "../../../api/characters";
import { getProductFacilityDefaults, listFacilities } from "../../../api/industry";
import {
  quoteInvention,
  searchInventableProducts,
  type InventionOption,
  type InventionQuote,
  type InventionSkill,
} from "../../../api/invention";
import { EveTypeImage } from "../../../components/eve-type-image";
import { IskCompact } from "../../../components/money";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "../../../components/operational-table";
import { Badge } from "../../../components/primitives";
import { useLoad } from "../../../hooks/use-load";
import { useStableCallback } from "../../../hooks/use-stable-callback";
import { PRIVATE_ATTR } from "../../../observability/private";
import {
  copyTone,
  formatChance,
  formatIskDelta,
  formatPercentDelta,
  gapLabel,
  optionKey,
  optionName,
  sortOptions,
  type CopyTone,
  type InventionSort,
  type SortOrder,
} from "./invention-format";
import { InventionInspector } from "./invention-inspector";
import {
  DEFAULT_VIEW,
  MAX_TARGET_RUNS,
  inventionQuoteCommand,
  inventionViewParams,
  parseInventionView,
  resolveChoice,
  type InventionView,
} from "./invention-url-state";
import { ProductSearch } from "./product-search";
import {
  readPreferredCharacter,
  readRecentProducts,
  rememberPreferredCharacter,
  rememberProduct,
  type RecentProduct,
} from "./recent-products";

const COLUMN_DEFS: Array<{
  key: string;
  label: string;
  width: string;
  sort?: InventionSort;
  numeric?: boolean;
  title?: string;
  hideBelow?: "tablet" | "desktop";
}> = [
  { key: "decryptor", label: "Decryptor", width: "minmax(190px,1fr)", sort: "decryptor" },
  { key: "chance", label: "Chance", width: "96px", sort: "chance" },
  { key: "copy", label: "Copy", width: "178px" },
  { key: "attempt", label: "Attempt", width: "84px", sort: "attempt", numeric: true, hideBelow: "desktop", title: "What one attempt costs: datacores, decryptor, job fee and one copy run." },
  { key: "perCopy", label: "Per copy", width: "84px", sort: "perCopy", numeric: true, title: "Attempt cost divided by the chance: the expected cost of one successful copy." },
  { key: "perRun", label: "Per run", width: "84px", sort: "perRun", numeric: true, hideBelow: "tablet", title: "Blueprint cost per manufacturing run." },
  { key: "unitCost", label: "Unit cost", width: "92px", sort: "unitCost", numeric: true, title: "Manufacturing cost of one unit at the copy's ME, plus its share of the blueprint cost." },
  { key: "vsNone", label: "vs none", width: "156px", numeric: true, hideBelow: "desktop", title: "Unit cost against no decryptor." },
  { key: "attempts", label: "Attempts", width: "92px", sort: "attempts", numeric: true, hideBelow: "tablet", title: "Expected attempts · attempts to be 90% sure, for the target runs." },
];

export function InventionPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const view = useMemo(() => parseInventionView(searchParams), [searchParams]);
  const [runsText, setRunsText] = useState(String(view.targetRuns));
  const [sort, setSort] = useState<{ key: InventionSort; order: SortOrder }>({ key: "unitCost", order: "asc" });
  const [recent, setRecent] = useState<RecentProduct[]>(readRecentProducts);
  const [preferredCharacter, setPreferredCharacter] = useState(readPreferredCharacter);

  // An empty search finds nothing but says whether the SDE has the data.
  const availability = useLoad((signal) => searchInventableProducts("", signal), "availability");
  const characters = useLoad(() => listCharacters(), "characters");
  const facilities = useLoad(() => listFacilities(), "facilities");
  // The workspace's facility defaults for this product. They can fail or
  // be unset: the pickers then start on the first facility.
  const defaultsLoad = useLoad(
    (signal) => getProductFacilityDefaults(view.productTypeId, signal),
    `defaults:${view.productTypeId}`,
  );
  // A previous product's defaults stay in `data` while the next load runs.
  const defaults = defaultsLoad.loading ? null : defaultsLoad.data;
  const settled = (characters.data !== null || characters.error !== null)
    && (facilities.data !== null || facilities.error !== null)
    && !defaultsLoad.loading;
  const roster = useMemo(() => characters.data ?? [], [characters.data]);
  const profiles = useMemo(() => (facilities.data ?? []).filter((profile) => profile.archivedAt === null), [facilities.data]);
  const manufacturingProfiles = useMemo(() => profiles.filter((profile) => profile.role === "manufacturing"), [profiles]);
  // Manufacturing profiles first: an invention job runs in the same
  // kind of structure.
  const inventionProfiles = useMemo(
    () => [...manufacturingProfiles, ...profiles.filter((profile) => profile.role !== "manufacturing")],
    [manufacturingProfiles, profiles],
  );
  // The character last chosen here leads, so the default is the inventor.
  const characterIds = useMemo(() => {
    const ids = roster.map((entry) => entry.connectionId);
    return [...ids.filter((id) => id === preferredCharacter), ...ids.filter((id) => id !== preferredCharacter)];
  }, [roster, preferredCharacter]);
  const choices = {
    characterId: resolveChoice(view.characterId, characterIds),
    inventionFacilityId: resolveChoice(
      view.inventionFacilityId,
      inventionProfiles.map((profile) => profile.id),
      defaults?.inventionFacilityId,
    ),
    manufacturingFacilityId: resolveChoice(
      view.manufacturingFacilityId,
      manufacturingProfiles.map((profile) => profile.id),
      defaults?.manufacturingFacilityId,
    ),
  };
  const command = settled ? inventionQuoteCommand(view, choices) : null;
  const quoteLoad = useLoad(command ? (signal) => quoteInvention(command, signal) : null, `quote:${JSON.stringify(command)}`);
  const response = view.productTypeId === null ? null : quoteLoad.data;
  const quote = response?.quote ?? null;
  const dataAvailable = (availability.data?.inventionDataAvailable ?? true) && (response?.inventionDataAvailable ?? true);

  function update(patch: Partial<InventionView>) {
    setSearchParams(inventionViewParams({ ...view, ...patch }), { replace: true });
  }

  // A different product has different skills and rows.
  function pickProduct(productTypeId: number) {
    update({ productTypeId, skillOverrides: {}, selected: null });
  }

  const applyRuns = useStableCallback((text: string) => {
    const runs = Number(text);
    if (Number.isInteger(runs) && runs >= 1 && runs <= MAX_TARGET_RUNS && runs !== view.targetRuns) {
      update({ targetRuns: runs });
    }
  });
  // Only typing schedules a re-quote; URL changes are already applied.
  useEffect(() => {
    const timer = window.setTimeout(() => applyRuns(runsText), 300);
    return () => window.clearTimeout(timer);
  }, [runsText, applyRuns]);

  const quotedProduct = quote ? { typeId: quote.productTypeId, name: quote.productName } : null;
  const quotedKey = quotedProduct ? `${quotedProduct.typeId}:${quotedProduct.name}` : null;
  useEffect(() => {
    if (quotedProduct) setRecent(rememberProduct(quotedProduct));
    // Keyed on the product, not on the object rebuilt each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [quotedKey]);

  const toggleSort = useStableCallback((key: InventionSort) => {
    setSort((current) => current.key === key
      ? { key, order: current.order === "asc" ? "desc" : "asc" }
      : { key, order: key === "chance" ? "desc" : "asc" });
  });
  const columns = useMemo<OperationalColumn[]>(() => COLUMN_DEFS.map((column) => ({
    key: column.key,
    label: column.label,
    title: column.title,
    width: column.width,
    align: column.numeric ? "right" : "left",
    numeric: column.numeric,
    hideBelow: column.hideBelow,
    sort: column.sort
      ? { direction: sort.key === column.sort ? sort.order : null, onToggle: () => toggleSort(column.sort!) }
      : undefined,
  })), [sort, toggleSort]);
  const options = useMemo(() => (quote ? sortOptions(quote.options, sort.key, sort.order) : []), [quote, sort]);
  const baseline = quote?.options.find((option) => option.decryptor === null) ?? null;
  const best = quote?.options.find((option) => option.best) ?? null;
  const selected = quote?.options.find((option) => optionKey(option) === view.selected) ?? null;
  const character = roster.find((entry) => entry.connectionId === choices.characterId) ?? null;
  const loading = view.productTypeId !== null && (!settled || quoteLoad.loading);

  return (
    <section
      className="relative -mx-3 -my-3 flex h-[calc(var(--iw-viewport-h)-var(--iw-header-h)-var(--iw-footer-h,0px))] min-h-0 flex-col overflow-clip bg-background text-xs sm:-mx-4 sm:-my-4"
      data-invention-workspace
    >
      <header className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-3">
        <FlaskConical aria-hidden="true" className="h-4 w-4 text-primary" />
        <h1 className="text-sm font-semibold">Invention</h1>
        <span className="ml-2 hidden text-muted sm:inline">Compare decryptors for a product</span>
      </header>

      <div className="flex flex-wrap items-end gap-x-3 gap-y-2 border-b border-border bg-panel px-3 py-2">
        <Field label="Product">
          <ProductSearch
            onPick={(product) => pickProduct(product.productTypeId)}
            selectedName={quote?.productName ?? null}
            selectedTypeId={view.productTypeId}
          />
        </Field>
        {quote && quote.outputs.length > 1 ? (
          <Field label="Output" note={`${quote.sourceBlueprintName} invents ${quote.outputs.length} blueprints`}>
            <select
              aria-label="Output"
              className="iw-input min-h-7 w-auto py-0.5 text-xs"
              onChange={(event) => pickProduct(Number(event.target.value))}
              value={String(quote.productTypeId)}
            >
              {quote.outputs.filter((output) => output.productTypeId !== null).map((output) => (
                <option key={output.blueprintTypeId} value={String(output.productTypeId)}>{output.productName}</option>
              ))}
            </select>
          </Field>
        ) : null}
        <Field label="Character">
          <select
            {...PRIVATE_ATTR}
            aria-label="Character"
            className="iw-input min-h-7 w-auto py-0.5 text-xs"
            onChange={(event) => {
              if (event.target.value !== "none") setPreferredCharacter(rememberPreferredCharacter(event.target.value));
              update({ characterId: event.target.value, skillOverrides: {} });
            }}
            value={choices.characterId ?? "none"}
          >
            {roster.map((entry) => <option key={entry.connectionId} value={entry.connectionId}>{entry.characterName}</option>)}
            <option value="none">No character (all skills 4)</option>
          </select>
        </Field>
        {quote ? (
          <SkillSteppers
            onChange={(skillOverrides) => update({ skillOverrides })}
            overrides={view.skillOverrides}
            skills={quote.skills}
          />
        ) : null}
        <Field label="Invention facility">
          <FacilitySelect
            label="Invention facility"
            onChange={(inventionFacilityId) => update({ inventionFacilityId })}
            profiles={inventionProfiles}
            value={choices.inventionFacilityId}
          />
        </Field>
        <Field label="Manufacturing facility">
          <FacilitySelect
            label="Manufacturing facility"
            onChange={(manufacturingFacilityId) => update({ manufacturingFacilityId })}
            profiles={manufacturingProfiles}
            value={choices.manufacturingFacilityId}
          />
        </Field>
        <Field label="Target runs">
          <input
            aria-label="Target runs"
            className="iw-input min-h-7 w-24 py-0.5 text-right font-mono text-xs tabular-nums"
            inputMode="numeric"
            max={MAX_TARGET_RUNS}
            min={1}
            onChange={(event) => setRunsText(event.target.value)}
            type="number"
            value={runsText}
          />
        </Field>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {!dataAvailable ? (
          <State icon>
            This SDE import has no invention data. Re-import the SDE to enable invention.
          </State>
        ) : view.productTypeId === null ? (
          <State>
            <p>Pick a product to compare decryptors</p>
            {recent.length > 0 ? (
              <div className="mt-2 flex flex-wrap justify-center gap-1.5">
                {recent.map((product) => (
                  <button
                    className="border border-border bg-panel-strong px-2 py-0.5 text-foreground hover:border-primary"
                    key={product.typeId}
                    onClick={() => pickProduct(product.typeId)}
                    type="button"
                  >
                    {product.name}
                  </button>
                ))}
              </div>
            ) : null}
          </State>
        ) : quoteLoad.errorMessage && !loading ? (
          <div className="m-3 border border-danger/60 bg-danger/10 px-3 py-2 text-danger" role="alert">
            {quoteLoad.error && "status" in quoteLoad.error && quoteLoad.error.status === 404
              ? "This product can't be invented, or the chosen character or facility is no longer available."
              : quoteLoad.errorMessage}{" "}
            <button className="underline" onClick={() => setSearchParams(inventionViewParams(DEFAULT_VIEW), { replace: true })} type="button">
              Start over
            </button>
          </div>
        ) : !quote ? (
          <Skeleton />
        ) : (
          <div aria-busy={loading} className={`transition-opacity ${loading ? "opacity-60" : ""}`}>
            <Summary best={best} quote={quote} />
            <Notices characterName={character?.characterName ?? null} hasInventionFacility={choices.inventionFacilityId !== null} quote={quote} />
            <OperationalTable
              ariaLabel="Decryptor comparison"
              columns={columns}
              onSelectRow={(rowKey) => update({ selected: rowKey })}
              selectedRowKey={view.selected}
            >
              <tbody>
                {options.map((option) => (
                  <OperationalTableRow
                    cells={cells(option, quote, baseline)}
                    key={optionKey(option)}
                    rowKey={optionKey(option)}
                    status={option.best ? "positive" : option.complete ? "neutral" : "warning"}
                  />
                ))}
              </tbody>
            </OperationalTable>
            <p className="px-3 py-2 text-[0.6875rem] text-muted">
              Unit cost assumes every manufacturing input is bought at market
              {quote.manufacturingInstallationIncluded ? "" : " and leaves out the installation cost"}. A{" "}
              <Link className="text-primary hover:underline" to="/builds">Build plan</Link> gives the exact figure.
            </p>
          </div>
        )}
      </div>
      {quote && selected ? (
        <InventionInspector onClose={() => update({ selected: null })} option={selected} quote={quote} />
      ) : null}
    </section>
  );
}

function cells(option: InventionOption, quote: InventionQuote, baseline: InventionOption | null): Record<string, ReactNode> {
  const base = baseline?.copy ?? option.copy;
  const delta = option.unitCostDelta !== null && option.decryptor !== null ? Number(option.unitCostDelta) : null;
  return {
    decryptor: (
      <span className="flex min-w-0 items-center gap-2">
        {option.decryptor ? (
          <EveTypeImage size={32} typeId={option.decryptor.typeId} typeName={option.decryptor.name} />
        ) : (
          <span aria-hidden="true" className="grid h-8 w-8 shrink-0 place-items-center border border-border text-muted">–</span>
        )}
        <span className="min-w-0">
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="truncate font-medium text-foreground" title={optionName(option)}>{optionName(option)}</span>
            {option.best ? <Badge square tone="positive">Best</Badge> : null}
            {!option.complete ? (
              <span title={option.gaps.map((gap) => gapLabel(gap, quote)).join("\n")}>
                <Badge square tone="warning">
                  {option.gaps.some((gap) => gap.kind === "decryptorPrice" || gap.kind === "materialPrice") ? "No price" : "Incomplete"}
                </Badge>
              </span>
            ) : null}
          </span>
          <span className="block text-[0.625rem] text-muted">
            {option.decryptor === null
              ? "no decryptor"
              : option.decryptor.unitPrice === null ? "—" : <IskCompact value={option.decryptor.unitPrice} />}
          </span>
        </span>
      </span>
    ),
    chance: <ChanceBar chance={option.chance} />,
    copy: (
      <span className="flex flex-wrap gap-1">
        <CopyTag tone={copyTone(option.copy.materialEfficiency, base.materialEfficiency)}>ME {option.copy.materialEfficiency}</CopyTag>
        <CopyTag tone={copyTone(option.copy.timeEfficiency, base.timeEfficiency)}>TE {option.copy.timeEfficiency}</CopyTag>
        <CopyTag tone={copyTone(option.copy.runs, base.runs)}>
          {option.copy.runs.toLocaleString()} {option.copy.runs === 1 ? "run" : "runs"}
        </CopyTag>
      </span>
    ),
    attempt: <Money value={option.attemptCost} />,
    perCopy: <Money value={option.costPerCopy} />,
    perRun: <Money value={option.costPerRun} />,
    unitCost: <Money emphasis value={option.unitCost} />,
    vsNone: delta === null || option.unitCostDelta === null ? (
      <span className="text-muted">—</span>
    ) : (
      <span className={`font-mono tabular-nums ${delta > 0 ? "text-danger" : delta < 0 ? "text-positive" : "text-muted"}`}>
        {formatIskDelta(option.unitCostDelta)}
        {option.unitCostDeltaPercent !== null ? ` (${formatPercentDelta(option.unitCostDeltaPercent)})` : ""}
      </span>
    ),
    attempts: option.attempts ? (
      <span className="font-mono tabular-nums">
        {option.attempts.expected.toLocaleString()} · {option.attempts.confident.toLocaleString()}
      </span>
    ) : <span className="text-muted">—</span>,
  };
}

function Money({ emphasis = false, value }: { emphasis?: boolean; value: string | null }) {
  if (value === null) return <span className="text-muted" title="Unknown">—</span>;
  return <IskCompact className={emphasis ? "font-semibold text-foreground" : ""} value={value} />;
}

function ChanceBar({ chance }: { chance: string }) {
  const percent = Math.min(100, Math.max(0, Number(chance) * 100));
  return (
    <span className="relative block h-5 w-full overflow-hidden border border-border bg-panel-strong">
      <span aria-hidden="true" className="absolute inset-y-0 left-0 bg-primary/30" style={{ width: `${percent}%` }} />
      <span className="relative block px-1.5 font-mono text-[0.6875rem] font-semibold leading-5 tabular-nums">{formatChance(chance)}</span>
    </span>
  );
}

const copyToneClasses: Record<CopyTone, string> = {
  better: "border-positive/50 bg-positive/10 text-positive",
  worse: "border-warning/50 bg-warning/10 text-warning",
  same: "border-border text-muted",
};

function CopyTag({ children, tone }: { children: ReactNode; tone: CopyTone }) {
  return (
    <span className={`whitespace-nowrap rounded-[2px] border px-1 py-px font-mono text-[0.625rem] ${copyToneClasses[tone]}`} data-tone={tone}>
      {children}
    </span>
  );
}

function Summary({ best, quote }: { best: InventionOption | null; quote: InventionQuote }) {
  return (
    <section aria-label="Invention summary" className="grid grid-cols-2 border-b border-border bg-panel lg:grid-cols-4">
      <SummaryTile label="Base chance">
        <span className="font-mono text-sm font-semibold tabular-nums">{formatChance(quote.baseChance)}</span>
      </SummaryTile>
      <SummaryTile label="With skills">
        <span className="font-mono text-sm font-semibold tabular-nums">{formatChance(quote.chanceWithSkills)}</span>
        <span className="ml-1.5 text-[0.625rem] text-muted">×{Number(quote.skillFactor).toFixed(3)}</span>
      </SummaryTile>
      <SummaryTile label="Datacores per attempt">
        <span className="block truncate" title={quote.materials.map((material) => `${material.quantity} × ${material.typeName}`).join(", ")}>
          {quote.materials.map((material) => `${material.quantity} × ${material.typeName.replace(/^Datacore - /, "")}`).join(" · ")}
        </span>
        {quote.materialsCost === null ? <span className="text-muted">—</span> : <IskCompact className="text-muted" value={quote.materialsCost} />}
      </SummaryTile>
      <SummaryTile label="Best option">
        {best ? (
          <>
            <span className="block truncate font-semibold text-primary">{optionName(best)}</span>
            {best.unitCost !== null ? (
              <span className="text-primary"><IskCompact value={best.unitCost} /> / unit</span>
            ) : null}
          </>
        ) : (
          <span className="text-muted" title="No option has every cost known.">—</span>
        )}
      </SummaryTile>
    </section>
  );
}

function SummaryTile({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="min-w-0 border-r border-border px-3 py-2">
      <div className="text-[0.625rem] font-semibold uppercase text-muted">{label}</div>
      <div className="mt-0.5 min-w-0 text-xs">{children}</div>
    </div>
  );
}

function Notices({ characterName, hasInventionFacility, quote }: { characterName: string | null; hasInventionFacility: boolean; quote: InventionQuote }) {
  const feeUnknown = quote.jobFee.total === null;
  return (
    <>
      {quote.missingSkills.length > 0 ? (
        <div className="flex items-center gap-2 border-b border-danger/40 bg-danger/10 px-3 py-1.5 text-danger" role="alert">
          <AlertTriangle aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
          <span>
            <span {...PRIVATE_ATTR}>{characterName ?? "This character"}</span> can't start this job:{" "}
            {quote.missingSkills.join(", ")} below the required level.
          </span>
        </div>
      ) : null}
      {quote.skillsDefaulted ? (
        <div className="border-b border-border bg-panel px-3 py-1.5 text-muted" role="status">
          No character chosen: every skill is taken as level 4.
        </div>
      ) : null}
      {feeUnknown ? (
        <div className="border-b border-warning/40 bg-warning/10 px-3 py-1.5 text-warning" role="status">
          {!hasInventionFacility
            ? "Choose an invention facility to price the job fee. Without it no option is complete."
            : quote.jobFee.estimatedItemValue === null
              ? "The invention job fee is unknown: adjusted prices for this blueprint are missing."
              : "The invention cost index for this facility's system hasn't been observed yet. It is fetched within the hour."}
        </div>
      ) : null}
    </>
  );
}

function SkillSteppers({ onChange, overrides, skills }: { onChange: (overrides: Record<number, number>) => void; overrides: Record<number, number>; skills: InventionSkill[] }) {
  const set = (skill: InventionSkill, level: number) => {
    const next = { ...overrides };
    // Back at the character's own level, the override is gone.
    if (skill.characterLevel === level) delete next[skill.skillTypeId];
    else next[skill.skillTypeId] = level;
    onChange(next);
  };
  const overridden = Object.keys(overrides).length > 0;
  return (
    <div aria-label="Skills" className="flex flex-wrap items-center gap-1.5" role="group">
      {skills.map((skill) => (
        <span
          className="flex min-h-7 items-center gap-1 border border-border bg-panel-strong px-1.5"
          key={skill.skillTypeId}
          title={`${skill.skillName}: ${skill.role === "encryption" ? "1/40" : "1/30"} of the base chance per level`}
        >
          <span className="max-w-32 truncate text-muted">{skill.skillName}</span>
          {skill.overridden ? <span aria-label="Overridden" className="h-1.5 w-1.5 rounded-full bg-warning" role="img" /> : null}
          <button
            aria-label={`Lower ${skill.skillName}`}
            className="grid h-5 w-5 place-items-center border border-border text-muted hover:text-foreground disabled:opacity-40"
            disabled={skill.level <= 0}
            onClick={() => set(skill, skill.level - 1)}
            type="button"
          >
            −
          </button>
          <span aria-label={`${skill.skillName} level`} className="w-3 text-center font-mono font-semibold tabular-nums">{skill.level}</span>
          <button
            aria-label={`Raise ${skill.skillName}`}
            className="grid h-5 w-5 place-items-center border border-border text-muted hover:text-foreground disabled:opacity-40"
            disabled={skill.level >= 5}
            onClick={() => set(skill, skill.level + 1)}
            type="button"
          >
            +
          </button>
        </span>
      ))}
      {overridden ? (
        <button className="text-[0.6875rem] text-primary hover:underline" onClick={() => onChange({})} type="button">Reset</button>
      ) : null}
    </div>
  );
}

function FacilitySelect({ label, onChange, profiles, value }: {
  label: string;
  onChange: (value: string) => void;
  profiles: Array<{ id: string; name: string; solarSystemName: string }>;
  value: string | null;
}) {
  return (
    <select
      aria-label={label}
      className="iw-input min-h-7 w-auto max-w-56 py-0.5 text-xs"
      onChange={(event) => onChange(event.target.value)}
      value={value ?? "none"}
    >
      {profiles.map((profile) => (
        <option key={profile.id} value={profile.id}>
          {profile.solarSystemName ? `${profile.solarSystemName} · ${profile.name}` : profile.name}
        </option>
      ))}
      <option value="none">None</option>
    </select>
  );
}

function Field({ children, label, note }: { children: ReactNode; label: string; note?: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <span className="text-[0.625rem] font-semibold uppercase text-muted">{label}</span>
      {children}
      {note ? <span className="text-[0.625rem] text-muted">{note}</span> : null}
    </div>
  );
}

function State({ children, icon = false }: { children: ReactNode; icon?: boolean }) {
  return (
    <div className="flex h-full min-h-40 flex-col items-center justify-center p-4 text-center text-muted">
      {icon ? <AlertTriangle aria-hidden="true" className="mb-2 h-5 w-5 text-warning" /> : null}
      {children}
    </div>
  );
}

function Skeleton() {
  const block = "animate-pulse rounded-[2px] bg-panel-strong";
  return (
    <div aria-busy="true" aria-label="Loading the comparison" role="status">
      <div className="grid grid-cols-2 border-b border-border bg-panel lg:grid-cols-4">
        {[0, 1, 2, 3].map((tile) => (
          <div className="space-y-1.5 border-r border-border px-3 py-2" key={tile}>
            <div className={`${block} h-2 w-20`} />
            <div className={`${block} h-3 w-28`} />
          </div>
        ))}
      </div>
      {Array.from({ length: 9 }, (_, row) => (
        <div className="flex h-10 items-center gap-6 border-b border-border px-3" key={row}>
          <div className={`${block} h-3 w-48`} />
          <div className={`${block} h-3 w-20`} />
          <div className={`${block} h-3 flex-1`} />
        </div>
      ))}
    </div>
  );
}
