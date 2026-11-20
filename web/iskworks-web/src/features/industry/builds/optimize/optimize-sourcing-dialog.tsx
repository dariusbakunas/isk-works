import { useEffect, useMemo, useRef, useState } from "react";

import {
  applySourcingOptimization,
  previewSourcingOptimization,
  type CurrentSourcing,
  type SourcingApplyOutcome,
  type SourcingChange,
  type SourcingProposal,
  type SourcingWarningKind,
} from "../../../../api/industry";
import { apiMessage } from "../../../../api/api-error";
import { IskCompact } from "../../../../components/money";
import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "../../../../components/operational-table";
import { InlineAlert } from "../../../../components/primitives";

/** Re-preview this long after the last option change. */
const PREVIEW_DEBOUNCE_MS = 300;
const DEFAULT_MIN_SAVING = "2";

const COLUMNS: OperationalColumn[] = [
  { key: "select", label: "", width: "2rem" },
  { key: "component", label: "Component", width: "minmax(4.5rem, 1fr)" },
  { key: "quantity", label: "Qty", width: "6rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "current", label: "Current", width: "5.5rem", hideBelow: "tablet" },
  { key: "proposed", label: "Proposed", width: "4.75rem" },
  { key: "buy", label: "Buy/unit", width: "6.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "build", label: "Build/unit", width: "6.5rem", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "saving", label: "Saving", width: "5.5rem", align: "right", numeric: true },
];

const WARNING_TEXT: Record<SourcingWarningKind, string> = {
  no_blueprint: "no synced blueprint, so it stays bought",
  no_buy_price: "no market price to compare against",
  produce_cost_incomplete: "its build cost could not be fully priced",
  recipe_unavailable: "its recipe could not be read from the SDE",
};

function currentLabel(from: CurrentSourcing | null): string {
  switch (from) {
    case "buy":
      return "Buy";
    case "produce":
      return "Build";
    case "mixed":
      return "Mixed";
    default:
      // Only reached through a producer this proposal creates.
      return "New";
  }
}

function proposedLabel(change: SourcingChange): string {
  if (change.to.kind === "buy") return "Buy";
  return change.to.recipe.mode === "reaction" ? "React" : "Build";
}

function money(value: string | null) {
  return value ? <IskCompact value={value} /> : <span className="iw-muted">—</span>;
}

type ApplyState =
  | { status: "idle" }
  | { status: "applying" }
  | { status: "done"; outcome: SourcingApplyOutcome }
  | { status: "error"; message: string };

/**
 * Optimize sourcing: previews Buy vs Build for every component of the root
 * plan `buildId` and applies the rows the user keeps. Unticking a row
 * re-previews with it excluded, so the totals always match what Apply
 * writes. `onClose(applied)` -- `applied` when anything was written, so the
 * caller reloads the Build.
 */
export function OptimizeSourcingDialog({
  buildId,
  open,
  onClose,
}: {
  buildId: string;
  open: boolean;
  onClose: (applied: boolean) => void;
}) {
  const [minSaving, setMinSaving] = useState(DEFAULT_MIN_SAVING);
  const [includeWithoutBlueprint, setIncludeWithoutBlueprint] = useState(false);
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  // Last-seen values of excluded rows: they drop out of the proposal, but
  // stay listed (unticked) so they can be ticked again.
  const [excludedRows, setExcludedRows] = useState<Map<number, SourcingChange>>(new Map());
  const [proposal, setProposal] = useState<SourcingProposal | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [apply, setApply] = useState<ApplyState>({ status: "idle" });
  const [previewKey, setPreviewKey] = useState(0);
  const requestId = useRef(0);

  useEffect(() => {
    if (!open) return;
    setMinSaving(DEFAULT_MIN_SAVING);
    setIncludeWithoutBlueprint(false);
    setExcluded(new Set());
    setExcludedRows(new Map());
    setProposal(null);
    setError("");
    setApply({ status: "idle" });
  }, [open]);

  const minSavingValid = /^\d+(\.\d+)?$/.test(minSaving.trim()) && Number(minSaving) < 100;
  const exclude = useMemo(() => [...excluded].sort((a, b) => a - b), [excluded]);

  useEffect(() => {
    if (!open || !minSavingValid) return;
    const id = ++requestId.current;
    setLoading(true);
    const timer = window.setTimeout(() => {
      previewSourcingOptimization(buildId, {
        minSavingPercent: minSaving.trim(),
        includeWithoutBlueprint,
        exclude,
      })
        .then((next) => {
          if (id !== requestId.current) return;
          setProposal(next);
          setError("");
        })
        .catch((requestError) => {
          if (id !== requestId.current) return;
          setError(apiMessage(requestError));
        })
        .finally(() => {
          if (id === requestId.current) setLoading(false);
        });
    }, PREVIEW_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [open, buildId, minSaving, minSavingValid, includeWithoutBlueprint, exclude, previewKey]);

  if (!open) return null;

  const applying = apply.status === "applying";
  const outcome = apply.status === "done" ? apply.outcome : null;
  const anyApplied = (outcome?.applied.length ?? 0) > 0;
  const changes = proposal?.changes ?? [];
  const proposed = new Set(changes.map((change) => change.typeId));
  // Until the re-preview lands, the proposal on screen may still carry a row
  // just unticked (or lack one just ticked again): one row per type, ticked
  // by the user's choice, never both the stale and the remembered copy.
  const rows = [
    ...changes.map((change) => ({ change, checked: !excluded.has(change.typeId) })),
    ...[...excludedRows.values()]
      .filter((change) => !proposed.has(change.typeId) && (excluded.has(change.typeId) || loading))
      .map((change) => ({ change, checked: !excluded.has(change.typeId) })),
  ];
  const selectedCount = rows.filter((row) => row.checked).length;
  const depths = [...new Set(rows.map((row) => row.change.depth))].sort((a, b) => a - b);

  function toggle(change: SourcingChange, checked: boolean) {
    setExcluded((current) => {
      const next = new Set(current);
      if (checked) next.delete(change.typeId);
      else next.add(change.typeId);
      return next;
    });
    if (!checked) setExcludedRows((current) => new Map(current).set(change.typeId, change));
  }

  async function applyChanges() {
    if (!proposal || changes.length === 0) return;
    setApply({ status: "applying" });
    try {
      const result = await applySourcingOptimization(buildId, proposal);
      if (result.failed.length === 0) {
        onClose(true);
        return;
      }
      setApply({ status: "done", outcome: result });
    } catch (requestError) {
      setApply({ status: "error", message: apiMessage(requestError) });
    }
  }

  function rerun() {
    setApply({ status: "idle" });
    setPreviewKey((key) => key + 1);
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="optimize-sourcing-title"
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-4xl p-3 sm:p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="optimize-sourcing-title">
          Optimize sourcing
        </h2>
        <p className="iw-muted mt-1">
          Buy or build each component, whichever gives the higher margin, at the saved plan&apos;s prices and
          inventory. Untick a row to keep its current sourcing.
        </p>

        <div className="iw-dialog-body">
          <div className="mt-3 flex flex-wrap items-end gap-4">
            <label className="grid gap-1 text-xs">
              <span className="iw-muted">Min saving %</span>
              <input
                aria-invalid={!minSavingValid}
                aria-label="Minimum saving percent"
                className="iw-input w-24"
                disabled={applying}
                inputMode="decimal"
                onChange={(event) => setMinSaving(event.target.value)}
                value={minSaving}
              />
            </label>
            <label className="flex items-center gap-2 pb-1.5 text-xs">
              <input
                checked={includeWithoutBlueprint}
                disabled={applying}
                onChange={(event) => setIncludeWithoutBlueprint(event.target.checked)}
                type="checkbox"
              />
              Include components without a blueprint
            </label>
            <div aria-live="polite" className="ml-auto grid gap-0.5 text-right text-xs">
              <span className="iw-muted">Margin</span>
              <span className="font-mono">
                {money(proposal?.currentMargin ?? null)}
                <span className="iw-muted"> → </span>
                {money(proposal?.proposedMargin ?? null)}
                {proposal?.delta && Number(proposal.delta) > 0 ? (
                  <span className="ml-2 text-positive">
                    +<IskCompact value={proposal.delta} />
                  </span>
                ) : null}
              </span>
            </div>
          </div>

          {!minSavingValid ? (
            <p className="mt-2 text-xs text-danger">Enter a percentage from 0 to below 100.</p>
          ) : null}
          {error ? (
            <div className="mt-3">
              <InlineAlert title="Preview failed">{error}</InlineAlert>
            </div>
          ) : null}
          {proposal?.notice ? <p className="iw-muted mt-3 text-xs">{proposal.notice}</p> : null}

          <div className="mt-3 max-h-[55vh] overflow-y-auto">
            {proposal === null ? (
              <p className="iw-muted text-sm" role="status">
                Comparing buy and build costs…
              </p>
            ) : rows.length === 0 ? (
              <p className="iw-muted text-sm" role="status">
                Sourcing is already optimal at this threshold.
              </p>
            ) : (
              <OperationalTable
                ariaLabel="Proposed sourcing changes"
                columns={COLUMNS}
                onSelectRow={() => {}}
                selectedRowKey={null}
              >
                {depths.map((depth) => {
                  const group = rows.filter((row) => row.change.depth === depth);
                  return (
                    <OperationalTableGroup
                      groupKey={`depth-${depth}`}
                      itemCount={group.length}
                      key={depth}
                      label={depth === 1 ? "Plan components" : `Level ${depth}`}
                    >
                      {group.map(({ change, checked }) => (
                        <OperationalTableRow
                          cells={{
                            select: (
                              <input
                                aria-label={`Apply ${change.typeName}`}
                                checked={checked}
                                disabled={applying || outcome !== null}
                                onChange={(event) => toggle(change, event.target.checked)}
                                type="checkbox"
                              />
                            ),
                            component: <span className={checked ? "" : "iw-muted"}>{change.typeName}</span>,
                            quantity: change.quantity.toLocaleString(),
                            current: currentLabel(change.from),
                            proposed: <span className={checked ? "font-medium" : "iw-muted"}>{proposedLabel(change)}</span>,
                            buy: money(change.buyUnit),
                            build: money(change.produceUnit),
                            saving: change.saving ? (
                              <IskCompact className={checked ? "text-positive" : "iw-muted"} value={change.saving} />
                            ) : (
                              <span className="iw-muted">—</span>
                            ),
                          }}
                          interactive={false}
                          key={change.typeId}
                          rowKey={String(change.typeId)}
                        />
                      ))}
                    </OperationalTableGroup>
                  );
                })}
              </OperationalTable>
            )}
          </div>

          {proposal && proposal.warnings.length > 0 ? (
            <ul aria-label="Optimization warnings" className="mt-3 grid gap-0.5 text-xs text-warning">
              {proposal.warnings.map((warning) => (
                <li key={`${warning.typeId}-${warning.kind}`}>
                  {warning.typeName}: {WARNING_TEXT[warning.kind]}
                </li>
              ))}
            </ul>
          ) : null}

          {apply.status === "error" ? (
            <div className="mt-3">
              <InlineAlert title="Sourcing not applied">
                {apply.message} Run the preview again to work from the current plan.
              </InlineAlert>
            </div>
          ) : null}
          {outcome && outcome.failed.length > 0 ? (
            <div className="mt-3">
              <InlineAlert title="Some changes were not applied">
                <ul className="grid gap-0.5">
                  {outcome.failed.map((failure) => (
                    <li key={`${failure.buildId}-${failure.typeId}`}>
                      {changes.find((change) => change.typeId === failure.typeId)?.typeName ?? failure.typeId}:{" "}
                      {failure.error}
                    </li>
                  ))}
                </ul>
              </InlineAlert>
            </div>
          ) : null}
        </div>

        <div className="iw-dialog-footer items-center justify-end gap-2">
          {loading && proposal !== null ? (
            <span className="iw-muted mr-auto text-xs" role="status">
              Updating…
            </span>
          ) : null}
          {outcome || apply.status === "error" ? (
            <>
              <button className="iw-button-secondary" onClick={() => onClose(anyApplied)} type="button">
                Close
              </button>
              <button className="iw-button-primary" onClick={rerun} type="button">
                Re-run
              </button>
            </>
          ) : (
            <>
              <button className="iw-button-secondary" disabled={applying} onClick={() => onClose(false)} type="button">
                Cancel
              </button>
              <button
                className="iw-button-primary"
                disabled={applying || loading || changes.length === 0 || !minSavingValid}
                onClick={() => void applyChanges()}
                type="button"
              >
                {applying ? "Applying…" : `Apply selected (${selectedCount})`}
              </button>
            </>
          )}
        </div>
      </section>
    </div>
  );
}
