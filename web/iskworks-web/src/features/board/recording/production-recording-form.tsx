import { useMemo, useRef, useState } from "react";

import {
  insufficientAvailable,
  proposedTakes,
  recordTicketProduction,
  type AvailabilityShortage,
  type TicketSummary,
} from "../../../api/industry";
import { MoneyInput } from "../../../components/money-input/money-input";
import type { MoneyInputResult } from "../../../components/money-input/parse-money-input";
import { parseMoneyInput } from "../../../components/money-input/parse-money-input";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "../../industry/shared/api-error";
import { newIdempotencyKey } from "./recording-meta";

function isPositiveInteger(value: string): boolean {
  const n = Number(value);
  return value.trim() !== "" && Number.isFinite(n) && Number.isInteger(n) && n > 0;
}

function isNonNegativeInteger(value: string): boolean {
  const n = Number(value);
  return value.trim() !== "" && Number.isFinite(n) && Number.isInteger(n) && n >= 0;
}

// The type-specific editor behind RecordingSection for a Manufacturing /
// Reaction ticket. Every default comes from `ticket.executionSnapshot` --
// the frozen *intended* plan -- never from a live Build preview: the
// snapshot is what was planned, the form is the actual execution being
// recorded. Posts exactly one explicit `record-production`; never a
// workflow-status write, never `/complete`.
export function ProductionRecordingForm({
  ticket,
  onCancel,
  onRecorded,
}: {
  ticket: TicketSummary;
  onCancel: () => void;
  onRecorded: () => void;
}) {
  const snapshot = ticket.executionSnapshot;
  const legacyNoSnapshot = snapshot === null;
  const plannedRuns = snapshot?.runs ?? 0;
  const plannedInstallTotal = snapshot?.installationCost?.total ?? null;
  const facility = snapshot?.facility ?? null;

  // The production summary counts runs; default the next recording to the
  // outstanding runs, or blank once the plan is met (0 can't be submitted,
  // and recording surplus runs is a deliberate explicit choice).
  const remainingRuns = ticket.recording?.remainingQuantity ?? 0;
  const seedRuns = remainingRuns > 0 ? remainingRuns : 0;

  // Planned output identity comes straight from the ticket DTO -- the user
  // never types a type id, and no Build lookup happens.
  // Always present -- this form only ever renders for a Manufacturing/
  // Reaction ticket, both of which require type_id/quantity.
  const outputTypeId = ticket.typeId ?? 0;
  const plannedQuantity = ticket.quantity ?? 0;
  const outputName = ticket.capturedName;

  const defaultOutput = (runs: number): string => {
    if (legacyNoSnapshot || plannedRuns <= 0) return "";
    // `ticket.quantity` is the frozen planned output for `plannedRuns` runs
    // (the drawer shows it as "N units from M runs"); scale it down.
    return String(Math.round((plannedQuantity / plannedRuns) * runs));
  };
  const defaultInput = (requiredQuantity: number, runs: number): string => {
    // Frozen prerequisites are totals for `plannedRuns` runs (frozen at
    // ticket creation). Scale by runs; round up so a partial recording is
    // never short the material it needs.
    if (plannedRuns <= 0) return String(requiredQuantity);
    return String(Math.ceil((requiredQuantity * runs) / plannedRuns));
  };
  const defaultInstall = (runs: number): string => {
    if (!plannedInstallTotal || plannedRuns <= 0) return "0";
    const scaled = (Number(plannedInstallTotal) * runs) / plannedRuns;
    if (!Number.isFinite(scaled)) return "0";
    return scaled.toFixed(2);
  };

  const [runs, setRuns] = useState(seedRuns > 0 ? String(seedRuns) : "");
  const [output, setOutput] = useState(() => defaultOutput(seedRuns));
  const [inputs, setInputs] = useState<Record<string, string>>(() =>
    Object.fromEntries(
      ticket.prerequisites.map((prerequisite) => [
        prerequisite.id,
        defaultInput(prerequisite.requiredQuantity, seedRuns),
      ]),
    ),
  );
  const [installCost, setInstallCost] = useState(() => defaultInstall(seedRuns));
  const [installCostResult, setInstallCostResult] = useState<MoneyInputResult>(() =>
    parseMoneyInput(defaultInstall(seedRuns), { final: true }),
  );
  const [locationNote, setLocationNote] = useState("");
  const [note, setNote] = useState("");
  // Which dependent fields the user has hand-edited -- an edited field is
  // never silently overwritten when runs changes again.
  const [touched, setTouched] = useState<ReadonlySet<string>>(() => new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [shortages, setShortages] = useState<AvailabilityShortage[] | null>(null);

  const idempotencyKeyRef = useRef<string>(newIdempotencyKey());

  function markTouched(key: string) {
    setTouched((current) => (current.has(key) ? current : new Set(current).add(key)));
  }

  function changeRuns(next: string) {
    setRuns(next);
    const runsNumber = Number(next);
    if (!isPositiveInteger(next)) return;
    if (!touched.has("output")) setOutput(defaultOutput(runsNumber));
    if (!touched.has("install")) setInstallCost(defaultInstall(runsNumber));
    setInputs((current) => {
      const nextInputs = { ...current };
      for (const prerequisite of ticket.prerequisites) {
        if (!touched.has(`input:${prerequisite.id}`)) {
          nextInputs[prerequisite.id] = defaultInput(prerequisite.requiredQuantity, runsNumber);
        }
      }
      return nextInputs;
    });
  }

  const runsValid = isPositiveInteger(runs);
  const outputValid = isNonNegativeInteger(output);
  const inputsValid = ticket.prerequisites.every((prerequisite) =>
    isPositiveInteger(inputs[prerequisite.id] ?? ""),
  );
  const installValid =
    installCostResult.status === "empty" || installCostResult.status === "valid";
  const canSubmit = runsValid && outputValid && inputsValid && installValid && !busy;

  const validationHint = useMemo(() => {
    if (!runsValid) return "Enter a positive whole number of runs completed.";
    if (!inputsValid) return "Every material quantity must be a positive whole number.";
    if (!outputValid) return "Enter the actual output quantity (0 or more).";
    if (!installValid) return "Enter a valid installation cost, or leave it blank for zero.";
    return "";
  }, [runsValid, inputsValid, outputValid, installValid]);

  async function submit(takeFrom?: { orderId: string; typeId: number }[]) {
    if (!canSubmit) {
      setError(validationHint || "Check the entered values.");
      return;
    }
    setBusy(true);
    setError("");
    setShortages(null);
    try {
      await recordTicketProduction(ticket.id, {
        idempotencyKey: idempotencyKeyRef.current,
        runsCompleted: Number(runs),
        output: { typeId: outputTypeId, quantity: Number(output) },
        inputs: ticket.prerequisites.map((prerequisite) => ({
          typeId: prerequisite.typeId,
          quantity: Number(inputs[prerequisite.id]),
        })),
        installationCost:
          installCostResult.status === "valid" ? (installCostResult.canonical as string) : "0",
        locationNote: locationNote.trim() || undefined,
        note: note.trim() || undefined,
        takeFrom,
      });
      idempotencyKeyRef.current = newIdempotencyKey();
      onRecorded();
    } catch (requestError) {
      // Other Epics reserved stock this recording needs: nothing was
      // recorded, so offer to take it (same idempotency key on retry).
      const shortage = insufficientAvailable(requestError);
      if (shortage) {
        setShortages(shortage.shortages);
      } else {
        setError(apiMessage(requestError));
      }
    } finally {
      setBusy(false);
    }
  }

  const actionLabel = ticket.kind === "reaction" ? "Record reaction" : "Record production";

  return (
    <form
      aria-label={actionLabel}
      className="space-y-2 rounded-[2px] border border-border bg-panel-strong/40 p-2"
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">{actionLabel}</p>

      {legacyNoSnapshot ? (
        <InlineAlert title="No frozen plan" tone="info">
          This ticket predates frozen production planning. Enter the actual production values
          manually.
        </InlineAlert>
      ) : null}

      {facility ? (
        <dl className="space-y-0.5 rounded-[2px] border border-border/70 bg-panel px-2 py-1 text-[11px]">
          <div className="flex justify-between gap-2">
            <dt className="text-muted">Facility</dt>
            <dd className="min-w-0 flex-1 truncate text-right text-foreground">{facility.name}</dd>
          </div>
          {facility.solarSystemName ? (
            <div className="flex justify-between gap-2">
              <dt className="text-muted">Solar system</dt>
              <dd className="min-w-0 flex-1 truncate text-right text-foreground">
                {facility.solarSystemName}
              </dd>
            </div>
          ) : null}
          {facility.structureTypeName ? (
            <div className="flex justify-between gap-2">
              <dt className="text-muted">Structure</dt>
              <dd className="min-w-0 flex-1 truncate text-right text-foreground">
                {facility.structureTypeName}
              </dd>
            </div>
          ) : null}
        </dl>
      ) : null}

      <label className="block text-xs font-medium text-muted">
        Runs completed
        <input
          className="iw-input mt-0.5 font-mono"
          inputMode="numeric"
          min="1"
          onChange={(event) => changeRuns(event.target.value)}
          step="1"
          type="number"
          value={runs}
        />
      </label>

      <div className="space-y-0.5">
        <span className="block text-xs font-medium text-muted">Output — {outputName}</span>
        <input
          aria-label={`Output quantity for ${outputName}`}
          className="iw-input font-mono"
          inputMode="numeric"
          min="0"
          onChange={(event) => {
            setOutput(event.target.value);
            markTouched("output");
          }}
          step="1"
          type="number"
          value={output}
        />
      </div>

      {ticket.prerequisites.length > 0 ? (
        <div className="space-y-1">
          <span className="block text-xs font-medium text-muted">Materials consumed</span>
          {ticket.prerequisites.map((prerequisite) => (
            <label className="block text-[11px] text-muted" key={prerequisite.id}>
              {prerequisite.capturedName}
              <input
                aria-label={`Consumed quantity for ${prerequisite.capturedName}`}
                className="iw-input mt-0.5 font-mono"
                inputMode="numeric"
                min="1"
                onChange={(event) => {
                  const value = event.target.value;
                  setInputs((current) => ({ ...current, [prerequisite.id]: value }));
                  markTouched(`input:${prerequisite.id}`);
                }}
                step="1"
                type="number"
                value={inputs[prerequisite.id] ?? ""}
              />
            </label>
          ))}
        </div>
      ) : null}

      <label className="block text-xs font-medium text-muted">
        Installation cost <span className="font-normal">(ISK — actual for this recording)</span>
        <MoneyInput
          aria-label="Installation cost (ISK)"
          className="mt-0.5"
          onClear={() => {
            setInstallCost("");
            markTouched("install");
          }}
          onCommit={(canonical) => {
            setInstallCost(canonical);
            markTouched("install");
          }}
          onValueChange={setInstallCostResult}
          value={installCost}
        />
      </label>

      <label className="block text-xs font-medium text-muted">
        Location / reference <span className="font-normal">(optional)</span>
        <input
          className="iw-input mt-0.5"
          onChange={(event) => setLocationNote(event.target.value)}
          value={locationNote}
        />
      </label>

      <label className="block text-xs font-medium text-muted">
        Note <span className="font-normal">(optional)</span>
        <textarea
          className="iw-input mt-0.5 min-h-14"
          onChange={(event) => setNote(event.target.value)}
          value={note}
        />
      </label>

      {error ? <InlineAlert title="Recording did not finish">{error}</InlineAlert> : null}
      {shortages ? (
        <ReservedByOtherEpics
          busy={busy}
          onCancel={() => setShortages(null)}
          onTake={(takeFrom) => void submit(takeFrom)}
          shortages={shortages}
        />
      ) : null}

      <div className="flex justify-end gap-2">
        <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
          Cancel
        </button>
        <button className="iw-button-primary" disabled={!canSubmit} type="submit">
          {busy ? "Recording…" : "Record"}
        </button>
      </div>
    </form>
  );
}

/** Recording would use stock other Epics reserved: say who holds what and
 * offer to take just the missing amount from them. */
function ReservedByOtherEpics({
  shortages,
  busy,
  onTake,
  onCancel,
}: {
  shortages: AvailabilityShortage[];
  busy: boolean;
  onTake: (takeFrom: { orderId: string; typeId: number }[]) => void;
  onCancel: () => void;
}) {
  const takes = proposedTakes(shortages);
  const takeLabel = takes
    .map((take) => `${take.quantity.toLocaleString()} ${take.typeName} from ${take.displayName}`)
    .join(", ");
  return (
    <div className="space-y-1.5">
      <InlineAlert title="Reserved by other Epics" tone="warning">
        {shortages.map((shortage) => (
          <span className="block" key={shortage.typeId}>
            {shortage.typeName}: needs {shortage.needed.toLocaleString()}, this Epic holds{" "}
            {shortage.own.toLocaleString()} and {shortage.free.toLocaleString()} is free.{" "}
            {shortage.holders
              .map((holder) => `Reserved by ${holder.displayName} (${holder.quantity.toLocaleString()})`)
              .join(", ")}
            .
          </span>
        ))}
      </InlineAlert>
      <div className="flex flex-wrap gap-2">
        <button
          className="iw-button-primary"
          disabled={busy}
          onClick={() => onTake(takes.map(({ orderId, typeId }) => ({ orderId, typeId })))}
          type="button"
        >
          Take {takeLabel} and record
        </button>
        <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
          Keep their reservations
        </button>
      </div>
    </div>
  );
}
