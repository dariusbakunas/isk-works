import { X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

import type { PlannerPricingSelection } from "../../../api/industry";
import { EveTypeImage } from "../../../components/eve-type-image";
import { MarketScopeSelector } from "../../../components/market-scope-selector";
import { MoneyInput } from "../../../components/money-input";
import { Badge } from "../../../components/primitives";
import { BlueprintSelectionFields } from "../builds/planner/blueprint-selection-section";
import { FacilityEivControl } from "../builds/components/planner-panels";
import { formatDuration } from "../shared/formatting";

import {
  INSPECTOR_SECTION_ORDER,
  type BlueprintSlice,
  type CostSlice,
  type CoverageSlice,
  type FacilitySlice,
  type InspectorActions,
  type InspectorModel,
  type RootPricingSlice,
  type RowPricingSlice,
  type ProvenanceSlice,
  type QuantitiesSlice,
  type RelatedBuildsSlice,
  type SourcingSlice,
} from "./inspector-model";
import {
  InspectorMetricGrid,
  InspectorRow,
  InspectorSection,
} from "./inspector-section";

/**
 * The single presentation component for a selected planning object. Both the
 * Worksheet item inspector and the Graph node inspector build an
 * `InspectorModel` + `InspectorActions` from their own selection and render
 * THIS -- there is no separate Worksheet or Graph inspector body.
 *
 * A section renders only when its model slice is present, so a raw material
 * and a linked Build expose different sections from the same component.
 * Collapse state is keyed by semantic section id (`InspectorCollapseProvider`
 * above), so it survives selection changes and Worksheet <-> Graph switches.
 */
export function UnifiedItemInspector({
  model,
  actions,
}: {
  model: InspectorModel;
  actions: InspectorActions;
}) {
  const sections: Partial<Record<(typeof INSPECTOR_SECTION_ORDER)[number], ReactNode>> = {
    quantities: model.quantities ? (
      <InspectorSection defaultExpanded id="quantities" label="Quantities" summary={model.quantities.summary}>
        <QuantitiesBody slice={model.quantities} />
      </InspectorSection>
    ) : null,
    coverage: model.coverage ? (
      <InspectorSection defaultExpanded id="coverage" label="Coverage" summary={model.coverage.summary}>
        <CoverageBody slice={model.coverage} />
      </InspectorSection>
    ) : null,
    sourcing: model.sourcing ? (
      <InspectorSection defaultExpanded id="sourcing" label="Sourcing" summary={model.sourcing.summary}>
        <SourcingBody name={model.identity.name} slice={model.sourcing} actions={actions.sourcing} />
      </InspectorSection>
    ) : null,
    blueprint: model.blueprint ? (
      <InspectorSection defaultExpanded id="blueprint" label="Blueprint" summary={model.blueprint.summary}>
        <BlueprintBody slice={model.blueprint} actions={actions.blueprint} />
      </InspectorSection>
    ) : null,
    recipe: model.recipe ? (
      <InspectorSection defaultExpanded id="recipe" label="Recipe" summary={model.recipe.summary}>
        <InspectorRow label="Reaction formula" value={model.recipe.name ?? "—"} />
      </InspectorSection>
    ) : null,
    facility: model.facility ? (
      <InspectorSection defaultExpanded id="facility" label="Facility" summary={model.facility.summary}>
        <FacilityBody slice={model.facility} actions={actions.facility} />
      </InspectorSection>
    ) : null,
    cost: model.cost ? (
      <InspectorSection defaultExpanded id="cost" label="Cost" summary={model.cost.summary}>
        <CostBody slice={model.cost} />
      </InspectorSection>
    ) : null,
    pricing: model.pricing ? (
      <InspectorSection defaultExpanded id="pricing" label="Pricing" summary={model.pricing.summary}>
        {model.pricing.kind === "root" ? (
          <RootPricingBody slice={model.pricing} actions={actions.pricing} />
        ) : (
          <PricingBody slice={model.pricing} onChange={actions.pricing?.onChange} />
        )}
      </InspectorSection>
    ) : null,
    duration:
      model.durationSeconds != null ? (
        <InspectorSection defaultExpanded id="duration" label="Duration">
          <InspectorRow label="Planned duration" value={formatDuration(model.durationSeconds)} />
        </InspectorSection>
      ) : null,
    inputs: model.inputs ? (
      <InspectorSection defaultExpanded id="inputs" label={model.inputs.label}>
        <RelatedBuildsBody slice={model.inputs} />
      </InspectorSection>
    ) : null,
    usedBy: model.usedBy ? (
      <InspectorSection defaultExpanded id="usedBy" label={model.usedBy.label}>
        <RelatedBuildsBody slice={model.usedBy} />
      </InspectorSection>
    ) : null,
    value: model.value ? (
      <InspectorSection defaultExpanded id="value" label="Value">
        {model.value.metrics.map((metric) => (
          <InspectorRow key={metric.label} label={metric.label} tone={metric.tone} value={metric.value} />
        ))}
      </InspectorSection>
    ) : null,
    provenance: model.provenance ? (
      <InspectorSection defaultExpanded id="provenance" label="Provenance" summary={model.provenance.summary}>
        <ProvenanceBody slice={model.provenance} onCopyBuildId={actions.copyBuildId} />
      </InspectorSection>
    ) : null,
  };

  return (
    <div className="pb-2">
      <IdentityHeader model={model} onClose={actions.onClose} />
      {model.statusLine ? (
        <p
          className={`border-b border-border px-3 py-1.5 text-[11px] ${
            model.statusLine.tone === "blocking" ? "text-danger" : "text-muted"
          }`}
          role="status"
        >
          {model.statusLine.text}
        </p>
      ) : null}
      <WarningBadges warnings={model.warnings} />
      {INSPECTOR_SECTION_ORDER.map((id) => (sections[id] ? <div key={id}>{sections[id]}</div> : null))}
      {actions.openLinkedBuild ? (
        <div className="px-3 py-2">
          <button
            aria-label={`Open linked build for ${model.identity.name}`}
            className="iw-button-secondary w-full"
            onClick={actions.openLinkedBuild}
            type="button"
          >
            Open linked build
          </button>
        </div>
      ) : null}
      {actions.calculationEvidence ? (
        <details className="border-t border-border px-3 pt-3">
          <summary className="cursor-pointer text-xs font-semibold">Exact calculation evidence</summary>
          <div className="mt-3">{actions.calculationEvidence}</div>
        </details>
      ) : null}
      {actions.footer}
    </div>
  );
}

// ─── header + warnings ─────────────────────────────────────────────────

function IdentityHeader({ model, onClose }: { model: InspectorModel; onClose?: () => void }) {
  const { identity } = model;
  return (
    <div className="sticky top-0 z-10 flex items-start gap-2 border-b border-border bg-panel px-3 py-2">
      {identity.showImage && identity.typeId != null ? (
        <EveTypeImage size={32} typeId={identity.typeId} typeName={identity.name} />
      ) : null}
      <div className="min-w-0 flex-1">
        <span className="block text-[10px] font-semibold uppercase tracking-wide text-muted">
          {identity.kindLabel}
        </span>
        <div className="truncate text-sm font-semibold">{identity.name}</div>
        {identity.subtitle ? (
          <div className="truncate text-[11px] text-muted">{identity.subtitle}</div>
        ) : null}
        {identity.summary ? (
          <div className="mt-0.5 text-[11px] text-foreground/80">{identity.summary}</div>
        ) : null}
      </div>
      {onClose ? (
        <button
          aria-label="Close inspector"
          className="grid h-7 w-7 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
          onClick={onClose}
          type="button"
        >
          <X aria-hidden="true" className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}

function WarningBadges({ warnings }: { warnings: InspectorModel["warnings"] }) {
  if (warnings.length === 0) return null;
  return (
    <div
      aria-label="Warnings"
      className="flex flex-wrap gap-1 border-b border-border px-3 py-2"
      role="group"
    >
      {warnings.map((warning) => (
        <Badge key={warning.label} square tone={warning.tone === "blocking" ? "danger" : "warning"}>
          <span title={warning.detail}>{warning.label}</span>
        </Badge>
      ))}
    </div>
  );
}

// ─── section bodies ────────────────────────────────────────────────────

function CoverageBar({ percentage, hasShortage }: { percentage: number; hasShortage: boolean }) {
  return (
    <div className="mt-1.5 flex items-center gap-2">
      <span className="h-1 flex-1 overflow-hidden bg-border">
        <span
          className={`block h-full ${hasShortage ? "bg-warning" : "bg-positive"}`}
          style={{ width: `${percentage}%` }}
        />
      </span>
      <strong className={`font-mono text-xs ${hasShortage ? "text-warning" : "text-positive"}`}>
        {percentage}%
      </strong>
    </div>
  );
}

function QuantitiesBody({ slice }: { slice: QuantitiesSlice }) {
  return (
    <>
      <InspectorMetricGrid metrics={slice.metrics} />
      {slice.percentage != null ? (
        <CoverageBar hasShortage={Boolean(slice.hasShortage)} percentage={slice.percentage} />
      ) : null}
    </>
  );
}

function CoverageBody({ slice }: { slice: CoverageSlice }) {
  return (
    <>
      <InspectorMetricGrid metrics={slice.metrics} />
      <div className="mt-1.5 flex items-center gap-2">
        <span className="h-1 flex-1 overflow-hidden bg-border">
          <span
            className={`block h-full ${slice.hasShortage ? "bg-warning" : "bg-positive"}`}
            style={{ width: `${slice.percentage}%` }}
          />
        </span>
        <strong className={`font-mono text-xs ${slice.hasShortage ? "text-warning" : "text-positive"}`}>
          {slice.percentage}%
        </strong>
      </div>
    </>
  );
}

const strategyBox =
  "flex cursor-pointer items-start gap-2 border px-2 py-1.5 text-xs";
const strategyActive = "border-primary bg-primary/10";
const strategyIdle = "border-border";

/** The "Buy" option's sub-text, honest about inventory coverage. Never
 * implies inventory is itself a persisted sourcing strategy. */
function buyDescription(slice: SourcingSlice): string {
  if (slice.usingInventory) return "Use existing inventory for this requirement.";
  const covered = Math.max(
    0,
    Math.min(slice.requiredQuantity - slice.missingQuantity, slice.requiredQuantity),
  );
  if (covered > 0 && slice.missingQuantity > 0) {
    return "Use available inventory and acquire the remaining shortage.";
  }
  return "Acquire this requirement externally.";
}

function SourcingBody({
  name,
  slice,
  actions,
}: {
  name: string;
  slice: SourcingSlice;
  actions: InspectorActions["sourcing"];
}) {
  const group = `sourcing-${name}`;
  const disabled = actions?.pending;
  return (
    <fieldset className="grid gap-1" disabled={disabled}>
      <legend className="sr-only">How to source {name}</legend>
      {(slice.fullyCoveredByInventory && actions?.onUseInventory) || slice.usingInventory ? (
        <label className={`${strategyBox} ${slice.usingInventory ? strategyActive : strategyIdle}`}>
          <input
            aria-label="Use Inventory"
            checked={slice.usingInventory}
            name={group}
            onChange={() => actions?.onUseInventory?.()}
            type="radio"
          />
          <span>
            <strong className="block">Use Inventory</strong>
            <span className="text-muted">
              {slice.availableQuantity > 0
                ? `${slice.availableQuantity.toLocaleString()} available — fully covers this row.`
                : "Fully covered by existing inventory."}
            </span>
          </span>
        </label>
      ) : null}
      <label
        className={`${strategyBox} ${slice.mode === "buy" && !slice.usingInventory ? strategyActive : strategyIdle}`}
      >
        <input
          aria-label="Buy"
          checked={slice.mode === "buy" && !slice.usingInventory}
          name={group}
          onChange={() => actions?.onBuy?.()}
          type="radio"
        />
        <span>
          <strong className="block">Buy</strong>
          <span className="text-muted">{buyDescription(slice)}</span>
        </span>
      </label>
      {slice.buildable ? (
        <label className={`${strategyBox} ${slice.mode === "build" ? strategyActive : strategyIdle}`}>
          <input
            aria-label="Build"
            checked={slice.mode === "build"}
            name={group}
            onChange={() => actions?.onBuild?.()}
            type="radio"
          />
          <span>
            <strong className="block">Build</strong>
            <span className="text-muted">Produce it from its own recipe.</span>
          </span>
        </label>
      ) : null}
      {!slice.usingInventory && slice.hasShortfall && actions?.onScope ? (
        <fieldset className="mt-1 grid gap-1 border-t border-border pt-1.5">
          <legend className="mb-1 text-[10px] font-semibold uppercase text-muted">Quantity</legend>
          <label className={`${strategyBox} ${slice.scope === "missing" ? strategyActive : strategyIdle}`}>
            <input
              aria-label="Shortage only"
              checked={slice.scope === "missing"}
              name={`${group}-scope`}
              onChange={() => actions.onScope?.("missing")}
              type="radio"
            />
            <span>
              <strong className="block">Shortage only</strong>
              <span className="text-muted">{slice.missingQuantity.toLocaleString()}</span>
            </span>
          </label>
          <label className={`${strategyBox} ${slice.scope === "full" ? strategyActive : strategyIdle}`}>
            <input
              aria-label="Full requirement"
              checked={slice.scope === "full"}
              name={`${group}-scope`}
              onChange={() => actions.onScope?.("full")}
              type="radio"
            />
            <span>
              <strong className="block">Full requirement</strong>
              <span className="text-muted">{slice.requiredQuantity.toLocaleString()}</span>
            </span>
          </label>
        </fieldset>
      ) : null}
      {slice.fulfillmentSentence ? (
        <p className="mt-1 text-[11px] leading-4 text-muted/75">{slice.fulfillmentSentence}</p>
      ) : null}
    </fieldset>
  );
}

function BlueprintBody({
  slice,
  actions,
}: {
  slice: BlueprintSlice;
  actions: InspectorActions["blueprint"];
}) {
  const canEdit = slice.editable && Boolean(actions);
  const hasObservations = slice.observations.length > 0;
  const startExisting =
    slice.mode === "existing" ? true : slice.mode === "manual" ? false : hasObservations;
  const [mode, setMode] = useState<"manual" | "observedAsset">(
    startExisting ? "observedAsset" : "manual",
  );
  const [kind, setKind] = useState<"original" | "copy">(slice.origin === "BPC" ? "copy" : "original");
  const [me, setMe] = useState(String(slice.me ?? 0));
  const [te, setTe] = useState(String(slice.te ?? 0));
  const [runs, setRuns] = useState(slice.licensedRuns != null ? String(slice.licensedRuns) : "");
  const [notes, setNotes] = useState(slice.notes);

  // Re-seed only when the underlying selection identity changes -- not on
  // every preview-derived me/te tick, so an in-progress edit isn't clobbered.
  useEffect(() => {
    setMode(startExisting ? "observedAsset" : "manual");
    setKind(slice.origin === "BPC" ? "copy" : "original");
    setMe(String(slice.me ?? 0));
    setTe(String(slice.te ?? 0));
    setRuns(slice.licensedRuns != null ? String(slice.licensedRuns) : "");
    setNotes(slice.notes);
  }, [slice.blueprintTypeId, slice.mode, slice.selectedObservationId]);

  function commitManual(next: Partial<{ kind: "original" | "copy"; me: string; te: string; runs: string; notes: string }> = {}) {
    const k = next.kind ?? kind;
    const meValue = Number(next.me ?? me);
    const teValue = Number(next.te ?? te);
    const runsValue = next.runs ?? runs;
    if (!Number.isInteger(meValue) || meValue < 0 || meValue > 10) return;
    if (!Number.isInteger(teValue) || teValue < 0 || teValue > 20) return;
    const licensedRuns = k === "copy" ? (runsValue ? Number(runsValue) : null) : null;
    if (k === "copy" && (licensedRuns == null || !Number.isInteger(licensedRuns) || licensedRuns < 1)) return;
    actions?.onModelManually?.({
      kind: k,
      materialEfficiency: meValue,
      timeEfficiency: teValue,
      licensedRuns,
      notes: next.notes ?? notes,
    });
  }

  // The observation currently backing an `observedAsset` selection -- its own
  // ME/TE/origin/runs are the authoritative read-only values.
  const selectedObservation = slice.selectedObservationId
    ? slice.observations.find((o) => o.id === slice.selectedObservationId) ?? null
    : null;
  const derivedOrigin =
    selectedObservation != null
      ? selectedObservation.kind === "copy"
        ? "BPC"
        : "BPO"
      : slice.origin;
  const derivedMe = selectedObservation?.materialEfficiency ?? slice.me;
  const derivedTe = selectedObservation?.timeEfficiency ?? slice.te;
  const derivedRuns =
    selectedObservation != null
      ? selectedObservation.kind === "copy"
        ? selectedObservation.licensedRuns
        : null
      : slice.licensedRuns;

  if (!canEdit) {
    return (
      <>
        <InspectorRow
          label="Blueprint"
          value={
            slice.name != null ? (
              <span>
                {slice.name}
                {derivedOrigin ? <span className="ml-1.5 text-muted">· {derivedOrigin}</span> : null}
              </span>
            ) : (
              "—"
            )
          }
        />
        {derivedMe != null && derivedTe != null ? (
          <InspectorRow label="ME / TE" value={`ME ${derivedMe} · TE ${derivedTe}`} />
        ) : slice.computing ? (
          <InspectorRow label="ME / TE" value={<span className="text-muted">Computing…</span>} />
        ) : null}
        {derivedRuns != null ? (
          <InspectorRow label="Runs remaining" value={derivedRuns.toLocaleString()} />
        ) : null}
      </>
    );
  }

  // Editable: the same "Enter manually" / "Use available blueprint" control
  // as the standalone Choose Blueprint dialog, so both offer one selection UX.
  return (
    <div className="space-y-2">
      <InspectorRow label="Blueprint" value={slice.name ?? "—"} />
      <BlueprintSelectionFields
        kind={kind}
        licensedRuns={runs}
        me={me}
        mode={mode}
        notes={notes}
        observations={slice.observations}
        onCommit={() => commitManual()}
        onKind={(next) => {
          setKind(next);
          commitManual({ kind: next });
        }}
        onLicensedRuns={setRuns}
        onMe={setMe}
        onMode={setMode}
        onNotes={setNotes}
        onObservation={(id) => actions?.onSelectObservation?.(id)}
        onTe={setTe}
        requiredRuns={slice.requiredRuns}
        selectedObservationId={slice.selectedObservationId ?? ""}
        te={te}
      />
      {mode === "observedAsset" && slice.selectedObservationId && selectedObservation == null ? (
        // The chosen blueprint isn't in the loaded list -- still state the
        // ME/TE the plan resolved from it.
        <>
          {derivedMe != null && derivedTe != null ? (
            <InspectorRow label="ME / TE" value={`ME ${derivedMe} · TE ${derivedTe}`} />
          ) : null}
          {derivedRuns != null ? (
            <InspectorRow label="Runs remaining" value={derivedRuns.toLocaleString()} />
          ) : null}
        </>
      ) : null}
      {actions?.error ? <p className="text-[11px] text-danger">{actions.error}</p> : null}
    </div>
  );
}

function FacilityBody({
  slice,
  actions,
}: {
  slice: FacilitySlice;
  actions: InspectorActions["facility"];
}) {
  if (!slice.editable || !actions) {
    return (
      <>
        <InspectorRow
          label="Facility"
          value={slice.name ?? <span className="text-muted">{slice.summary}</span>}
        />
        {slice.location ? <InspectorRow label="System" value={slice.location} /> : null}
        {slice.bonuses ? <InspectorRow label="Bonuses" value={slice.bonuses} /> : null}
        {slice.rigCount > 0 ? <InspectorRow label="Rigs" value={`${slice.rigCount} installed`} /> : null}
      </>
    );
  }
  const isRoot = slice.eiv !== undefined;
  const facilityLabel = isRoot
    ? slice.role === "reaction"
      ? "Reaction Facility"
      : "Manufacturing Facility"
    : "Facility";
  return (
    <div className="space-y-1.5">
      {slice.state !== "set" && !isRoot ? (
        <InspectorRow label="Resolved facility" value={<span className="text-muted">{slice.summary}</span>} />
      ) : null}
      <label className={`block text-xs font-semibold ${slice.state !== "set" && isRoot ? "text-warning" : ""}`}>
        {facilityLabel}
        <select
          aria-label={facilityLabel}
          className="iw-input mt-1"
          disabled={actions.pending}
          onChange={(event) => actions.onSelect?.(event.target.value || null)}
          value={slice.selectedFacilityId ?? ""}
        >
          <option value="">{isRoot ? "Not selected" : "Use build facility"}</option>
          {actions.options
            .filter((profile) => !profile.archivedAt && profile.role === slice.role)
            .map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
        </select>
      </label>
      {slice.location ? <InspectorRow label="System" value={slice.location} /> : null}
      {slice.bonuses ? <InspectorRow label="Bonuses" value={slice.bonuses} /> : null}
      {slice.rigCount > 0 ? <InspectorRow label="Rigs" value={`${slice.rigCount} installed`} /> : null}
      {slice.eiv ? (
        <FacilityEivControl
          automaticEiv={slice.eiv.automaticEiv}
          error={slice.eiv.error}
          loading={slice.eiv.loading}
          manual={slice.eiv.manual}
          onClearValue={() => actions.onEivClear?.()}
          onCommitValue={(canonical) => actions.onEivCommit?.(canonical)}
          onManual={(manual) => actions.onEivManual?.(manual)}
          value={slice.eiv.value}
        />
      ) : null}
      {actions.error ? <p className="text-[11px] text-danger">{actions.error}</p> : null}
    </div>
  );
}

function RootPricingBody({
  slice,
  actions,
}: {
  slice: RootPricingSlice;
  actions: InspectorActions["pricing"];
}) {
  return (
    <div className="space-y-3">
      <section aria-label="Material acquisition" className="space-y-1.5">
        <p className="iw-eyebrow">Material acquisition</p>
        <div>
          <span className="iw-eyebrow block">Market / location</span>
          <div className="mt-1">
            <MarketScopeSelector onChange={(scope) => actions?.onMaterialScope?.(scope)} scope={slice.materialScope} />
          </div>
        </div>
        <label className="block text-xs font-semibold">
          Material pricing
          <select
            className="iw-input mt-1"
            onChange={(event) => actions?.onMaterialPolicy?.(event.target.value as RootPricingSlice["materialPolicy"])}
            value={slice.materialPolicy}
          >
            <option value="highestBuy">Use highest buy order (placed-order value)</option>
            <option value="acquireQuantityFromSellOrders">Buy immediately</option>
          </select>
        </label>
      </section>
      <section aria-label="Output valuation" className="space-y-1.5 border-t border-border pt-3">
        <p className="iw-eyebrow">Output valuation</p>
        <div>
          <span className="iw-eyebrow block">Market / location</span>
          <div className="mt-1">
            <MarketScopeSelector onChange={(scope) => actions?.onOutputScope?.(scope)} scope={slice.outputScope} />
          </div>
        </div>
        <label className="block text-xs font-semibold">
          Output pricing
          <select
            className="iw-input mt-1"
            onChange={(event) => actions?.onOutputPolicy?.(event.target.value as RootPricingSlice["outputPolicy"])}
            value={slice.outputPolicy}
          >
            <option value="lowestSell">Use lowest sell order (listing value)</option>
            <option value="liquidateQuantityIntoBuyOrders">Sell immediately</option>
          </select>
        </label>
      </section>
      <label className="block border-t border-border pt-3 text-xs font-semibold">
        Price Override (fallback)
        <select
          className="iw-input mt-1"
          onChange={(event) => actions?.onPriceSource?.(event.target.value)}
          value={slice.priceSourceId}
        >
          <option value="">None -- market price only</option>
          {slice.priceSources.map((item) => (
            <option key={item.id} value={item.id}>
              {item.name}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

function CostBody({ slice }: { slice: CostSlice }) {
  const unknown = slice.computing
    ? "Computing…"
    : slice.state === "incomplete"
      ? "Incomplete"
      : slice.state === "stale"
        ? "Stale"
        : slice.state === "unresolved"
          ? "Unresolved"
          : "Not computed";
  return (
    <>
      <InspectorRow label="Material / Component Cost" value={slice.material ?? unknown} />
      <InspectorRow
        label="Installation"
        value={
          slice.installation ?? (
            <span className="text-muted">{slice.computing ? "Computing…" : "Not included"}</span>
          )
        }
      />
      <InspectorRow
        label="Total Production Cost"
        value={
          slice.total ?? (
            <span className="text-muted">{slice.computing ? "Computing…" : "Incomplete"}</span>
          )
        }
      />
    </>
  );
}

const pricingBox = "flex cursor-pointer items-start gap-2 border px-2 py-1.5 text-xs";

/** Row pricing controls (default / policy override / manual price) --
 * shared by the unified inspector and the Plan input inspector, so a
 * row-level price exception has one UI and one write
 * path (`applyPricingSelection`). */
export function PricingBody({
  slice,
  onChange,
}: {
  slice: RowPricingSlice;
  onChange: ((selection: PlannerPricingSelection) => void) | undefined;
}) {
  const [mode, setMode] = useState(slice.mode);
  const [policy, setPolicy] = useState(slice.policy);
  useEffect(() => {
    setMode(slice.mode);
    setPolicy(slice.policy);
  }, [slice.typeId, slice.role, slice.mode, slice.policy]);

  if (slice.readOnly || !onChange) {
    return (
      <InspectorRow
        label="Captured setting"
        value={
          slice.mode === "manual"
            ? "Manual price"
            : slice.mode === "policy"
              ? "Pricing policy override"
              : "Price Source default"
        }
      />
    );
  }
  const group = `pricing-${slice.role}-${slice.typeId}`;
  return (
    <>
      <fieldset className="grid gap-1">
        <legend className="sr-only">Pricing mode</legend>
        <label className={`${pricingBox} ${mode === "default" ? strategyActive : strategyIdle}`}>
          <input
            aria-label="Price Source default"
            checked={mode === "default"}
            name={group}
            onChange={() => {
              setMode("default");
              onChange({ typeId: slice.typeId, role: slice.role, selection: { kind: "default" } });
            }}
            type="radio"
          />
          <span>
            <strong className="block">Price Source default</strong>
            <span className="text-muted">Use the captured policy and observation.</span>
          </span>
        </label>
        {slice.allowPolicyOverride ? (
          <label className={`${pricingBox} ${mode === "policy" ? strategyActive : strategyIdle}`}>
            <input
              aria-label="Pricing policy override"
              checked={mode === "policy"}
              name={group}
              onChange={() => {
                setMode("policy");
                onChange({ typeId: slice.typeId, role: slice.role, selection: { kind: "market_policy", policy } });
              }}
              type="radio"
            />
            <span>
              <strong className="block">Pricing policy override</strong>
              <span className="text-muted">Override the material default for this row.</span>
            </span>
          </label>
        ) : null}
        <label className={`${pricingBox} ${mode === "manual" ? strategyActive : strategyIdle}`}>
          <input
            aria-label="Manual price"
            checked={mode === "manual"}
            name={group}
            onChange={() => setMode("manual")}
            type="radio"
          />
          <span>
            <strong className="block">Manual price</strong>
            <span className="text-muted">Override this row only.</span>
          </span>
        </label>
      </fieldset>
      {mode === "manual" ? (
        <label className="mt-2 block text-xs font-semibold">
          Unit price
          <MoneyInput
            aria-label="Unit price"
            className="mt-1"
            commitDebounceMs={250}
            onClear={() =>
              onChange({ typeId: slice.typeId, role: slice.role, selection: { kind: "default" } })
            }
            onCommit={(canonical) =>
              onChange({
                typeId: slice.typeId,
                role: slice.role,
                selection: { kind: "manual", unit_price: canonical },
              })
            }
            value={slice.manualUnitPrice ?? ""}
          />
        </label>
      ) : mode === "policy" ? (
        <label className="mt-2 block text-xs font-semibold">
          Material pricing policy
          <select
            className="iw-input mt-1"
            onChange={(event) => {
              const value = event.target.value as RowPricingSlice["policy"];
              setPolicy(value);
              onChange({
                typeId: slice.typeId,
                role: slice.role,
                selection: { kind: "market_policy", policy: value },
              });
            }}
            value={policy}
          >
            <option value="highestBuy">Highest buy order</option>
            <option value="acquireQuantityFromSellOrders">Buy immediately</option>
          </select>
        </label>
      ) : null}
    </>
  );
}

function RelatedBuildsBody({ slice }: { slice: RelatedBuildsSlice }) {
  return (
    <div className="grid gap-1.5">
      {slice.entries.map((entry) => (
        <div
          className="flex items-center justify-between gap-3 py-0.5 text-xs"
          key={entry.typeId ?? "root"}
        >
          <span className="flex min-w-0 items-center gap-1.5 text-muted">
            <EveTypeImage
              size={24}
              typeId={entry.typeId ?? slice.fallbackTypeId ?? 0}
              typeName={entry.name}
            />
            <span className="truncate">{entry.name}</span>
          </span>
          <strong className="shrink-0 text-right font-mono">{entry.quantity.toLocaleString()}</strong>
        </div>
      ))}
    </div>
  );
}

function ProvenanceBody({
  slice,
  onCopyBuildId,
}: {
  slice: ProvenanceSlice;
  onCopyBuildId: (() => void) | undefined;
}) {
  return (
    <div className="space-y-1.5">
      {slice.recipeCurrency ? (
        <div className="space-y-1">
          <Badge square tone={slice.recipeCurrency.tone === "blocking" ? "danger" : "warning"}>
            {slice.recipeCurrency.label}
          </Badge>
          {slice.recipeCurrency.explanation ? (
            <p className="text-[11px] leading-snug text-muted">{slice.recipeCurrency.explanation}</p>
          ) : null}
        </div>
      ) : null}
      <details>
        <summary className="cursor-pointer text-xs font-semibold">{slice.summary}</summary>
        <div className="mt-2 space-y-1">
          {slice.lines.map((line) => (
            <InspectorRow key={line.label} label={line.label} value={line.value} />
          ))}
          {slice.note ? <p className="break-words text-[11px] text-muted" data-private="">{slice.note}</p> : null}
          {slice.buildId ? (
            <div className="flex items-center justify-between gap-2 pt-1 text-[11px] text-muted">
              <span className="truncate font-mono">Build {slice.buildId}</span>
              {onCopyBuildId ? (
                <button className="iw-button-secondary shrink-0 px-1.5 py-0.5" onClick={onCopyBuildId} type="button">
                  Copy ID
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </details>
    </div>
  );
}

