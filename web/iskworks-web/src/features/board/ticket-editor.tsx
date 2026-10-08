import { useEffect, useState } from "react";

import type { CharacterRosterEntry } from "../../api/characters";
import type { Build } from "../../api/industry";
import {
  createTicket,
  previewTicketPlan,
  type CreateTicketInput,
  type OrderSummary,
  type Ticket,
  type TicketKind,
  type TicketPlanPreview,
} from "../../api/industry";
import { searchTypes, type TypeSearchResult } from "../../api/sde";
import { Badge, InlineAlert } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { EveTypeImage } from "../../components/eve-type-image";
import { useDebouncedLookup } from "../../hooks/use-debounced-lookup";
import { apiMessage } from "../industry/shared/api-error";
import { formatDuration } from "../industry/shared/formatting";
import { requirementKindMeta } from "./order-meta";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";

type EditorKind = TicketKind;

const kindOptions: { value: EditorKind; label: string }[] = [
  { value: "generic", label: "Generic" },
  { value: "acquisition", label: "Acquisition" },
  { value: "manufacturing", label: "Manufacturing" },
  { value: "reaction", label: "Reaction" },
];

const PREVIEW_DEBOUNCE_MS = 300;

function Field({ id, label, children }: { id: string; label: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1">
      <label className="block text-[10px] font-semibold uppercase tracking-wide text-muted" htmlFor={id}>
        {label}
      </label>
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between gap-2">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 truncate text-right text-foreground">{children}</dd>
    </div>
  );
}

// The read-only "plan to freeze" panel -- everything here comes from the
// server's own calculation (see `previewTicketPlan`/`POST /api/tickets`),
// never recomputed client-side. Deliberately has no editable ME/TE/
// facility/material controls: those belong to the Build, not this editor.
function PlanToFreezePreview({ preview }: { preview: TicketPlanPreview }) {
  const snapshot = preview.executionSnapshot;
  const blueprint = snapshot.blueprint;
  const facility = snapshot.facility;

  return (
    <div className="space-y-3 rounded-[2px] border border-border bg-panel p-3">
      <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">Plan to freeze</p>

      <div className="space-y-1">
        <p className="text-sm font-semibold text-foreground">{preview.capturedName}</p>
        <p className="text-xs text-muted">
          ×{preview.quantity.toLocaleString()} units from {preview.runs.toLocaleString()} run
          {preview.runs === 1 ? "" : "s"}
        </p>
      </div>

      {blueprint ? (
        <dl className="space-y-1 text-xs">
          <Row label="Blueprint">{blueprint.blueprintName}</Row>
          <Row label="ME / TE">
            {blueprint.materialEfficiency} / {blueprint.timeEfficiency}
          </Row>
        </dl>
      ) : null}

      {facility ? (
        <dl className="space-y-1 text-xs">
          <Row label="Facility">{facility.name}</Row>
        </dl>
      ) : null}

      <dl className="space-y-1 text-xs">
        <Row label="Duration">{formatDuration(snapshot.durationSeconds)}</Row>
        {snapshot.installationCost?.total != null ? (
          <Row label="Installation">
            <MoneyAmount value={snapshot.installationCost.total} />
          </Row>
        ) : null}
        {snapshot.materialValue != null ? (
          <Row label="Material value">
            <MoneyAmount value={snapshot.materialValue} />
          </Row>
        ) : null}
      </dl>

      {preview.prerequisites.length > 0 ? (
        <div className="space-y-1">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted">
            Materials ({preview.prerequisites.length})
          </p>
          <div className="divide-y divide-border/50">
            {preview.prerequisites.map((prerequisite) => (
              <div className="flex items-center gap-2 py-0.5 text-[11px]" key={prerequisite.typeId}>
                <span className="min-w-0 flex-1 truncate text-foreground">{prerequisite.capturedName}</span>
                <span className="w-16 shrink-0 text-right font-mono tabular-nums text-foreground">
                  {prerequisite.requiredQuantity.toLocaleString()}
                </span>
                <span className="w-14 shrink-0 text-right">
                  <Badge square tone={requirementKindMeta[prerequisite.kind].tone}>
                    {requirementKindMeta[prerequisite.kind].label}
                  </Badge>
                </span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

// One canonical Ticket editor, discriminated by `kind` inside a single
// component -- not a separate form per kind. `initialKind`/`initialEpicId`
// are prefills only: every field (including the Epic) stays editable, so
// "Create ticket" from Board, from an Epic Inspector, or (later) from a
// Build/Graph can all reuse this exact component with different defaults.
//
// Manufacturing/Reaction never edit recipe/ME/TE/facility/material/pricing
// -- per the product rule ("Build owns the production plan; Ticket freezes
// that plan as intended work"), this editor only lets a person pick a
// Build and an optional Runs override, then shows the server's own
// read-only preview of what will be frozen.
export function TicketEditor({
  orders,
  characters,
  builds,
  initialKind = "generic",
  initialEpicId = null,
  onClose,
  onCreated,
}: {
  /** Board's already-loaded Epic list -- no extra fetch just to populate
   * this dropdown. */
  orders: OrderSummary[];
  /** Already-loaded/cached connected-character roster -- no ESI call is
   * ever made to open this editor. */
  characters: CharacterRosterEntry[];
  /** Board's already-loaded Build list -- the Manufacturing/Reaction Build
   * picker's data source, filtered client-side by recipe kind. No live
   * recalculation per dropdown row; the backend still validates the
   * kind/recipe match independently at creation time. */
  builds: Build[];
  initialKind?: TicketKind;
  initialEpicId?: string | null;
  onClose: () => void;
  onCreated: (ticket: Ticket) => void;
}) {
  const [kind, setKind] = useState<EditorKind>(initialKind);
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  const [orderId, setOrderId] = useState(initialEpicId ?? "");
  const [assigneeCharacterId, setAssigneeCharacterId] = useState("");
  const [itemQuery, setItemQuery] = useState("");
  const [selectedItem, setSelectedItem] = useState<TypeSearchResult | null>(null);
  const [quantity, setQuantity] = useState("");
  const [buildId, setBuildId] = useState("");
  const [runsInput, setRunsInput] = useState("");
  const [preview, setPreview] = useState<TicketPlanPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const isProductionKind = kind === "manufacturing" || kind === "reaction";
  const compatibleBuilds = builds.filter((build) => build.recipe.kind === kind);

  const lookup = useDebouncedLookup<TypeSearchResult>(itemQuery, searchTypes, () => {}, {
    enabled: kind === "acquisition" && selectedItem === null,
  });

  // Debounced server-side preview: refetches on Build or Runs change, and
  // is the exact calculation `POST /api/tickets` freezes -- never
  // recomputed here, and never mutates the Build.
  useEffect(() => {
    if (!isProductionKind || !buildId) {
      setPreview(null);
      setPreviewError("");
      setPreviewLoading(false);
      return;
    }
    const trimmedRuns = runsInput.trim();
    const parsedRuns = trimmedRuns === "" ? undefined : Number(trimmedRuns);
    if (parsedRuns !== undefined && (!Number.isInteger(parsedRuns) || parsedRuns <= 0)) {
      setPreview(null);
      setPreviewError("");
      setPreviewLoading(false);
      return;
    }

    let cancelled = false;
    setPreviewLoading(true);
    const timer = window.setTimeout(() => {
      void previewTicketPlan(buildId, parsedRuns)
        .then((result) => {
          if (cancelled) return;
          setPreview(result);
          setPreviewError("");
        })
        .catch((requestError) => {
          if (cancelled) return;
          setPreview(null);
          setPreviewError(apiMessage(requestError));
        })
        .finally(() => {
          if (!cancelled) setPreviewLoading(false);
        });
    }, PREVIEW_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [isProductionKind, buildId, runsInput]);

  function changeKind(next: EditorKind) {
    setKind(next);
    setSelectedItem(null);
    setItemQuery("");
    setQuantity("");
    setBuildId("");
    setRunsInput("");
    setPreview(null);
    setPreviewError("");
  }

  function selectBuild(nextBuildId: string) {
    setBuildId(nextBuildId);
    // Defaults Runs to the selected Build's own -- never left blank, and
    // never silently clamped by this editor; a user override is validated
    // server-side against the planner's own bounds at creation time.
    const build = compatibleBuilds.find((candidate) => candidate.id === nextBuildId);
    setRunsInput(build ? String(build.runs) : "");
  }

  async function submit() {
    setError("");

    let input: CreateTicketInput;
    if (kind === "generic") {
      if (!title.trim()) {
        setError("A title is required.");
        return;
      }
      input = {
        kind: "generic",
        capturedName: title.trim(),
        notes: notes.trim() || undefined,
        orderId: orderId || undefined,
        assigneeCharacterId: assigneeCharacterId || undefined,
      };
    } else if (kind === "acquisition") {
      if (!selectedItem) {
        setError("Select an item to acquire.");
        return;
      }
      const parsedQuantity = Number(quantity);
      if (!Number.isInteger(parsedQuantity) || parsedQuantity <= 0) {
        setError("Enter a positive whole-number quantity.");
        return;
      }
      input = {
        kind: "acquisition",
        // The item's own name, not a free-form title -- explicit
        // acquisition recording validates the ticket's captured name
        // against the active SDE's real name for `typeId`, the same
        // invariant every generated Acquisition ticket already relies on.
        capturedName: selectedItem.typeName,
        typeId: selectedItem.typeId,
        quantity: parsedQuantity,
        notes: notes.trim() || undefined,
        orderId: orderId || undefined,
        assigneeCharacterId: assigneeCharacterId || undefined,
      };
    } else {
      if (!buildId) {
        setError("Select a Build.");
        return;
      }
      const trimmedRuns = runsInput.trim();
      const parsedRuns = trimmedRuns === "" ? undefined : Number(trimmedRuns);
      if (parsedRuns !== undefined && (!Number.isInteger(parsedRuns) || parsedRuns <= 0)) {
        setError("Enter a positive whole-number run count.");
        return;
      }
      input = {
        kind,
        buildId,
        runs: parsedRuns,
        notes: notes.trim() || undefined,
        orderId: orderId || undefined,
        assigneeCharacterId: assigneeCharacterId || undefined,
      };
    }

    setBusy(true);
    try {
      const ticket = await createTicket(input);
      onCreated(ticket);
    } catch (requestError) {
      // Keep the editor open with everything the user already entered --
      // never leave a half-configured ticket behind on a failed create.
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <PlannerInspectorShell
      closeLabel="Close ticket editor"
      eyebrow="New ticket"
      onClose={onClose}
      open
      title="Create ticket"
      width="wide"
    >
      <div className="flex-1 space-y-4 px-4 py-3">
        {error ? <InlineAlert title="Ticket was not created">{error}</InlineAlert> : null}

        <Field id="ticket-editor-kind" label="Type">
          <select
            className="iw-input w-full"
            id="ticket-editor-kind"
            onChange={(event) => changeKind(event.target.value as EditorKind)}
            value={kind}
          >
            {kindOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </Field>

        {kind === "generic" ? (
          <Field id="ticket-editor-title" label="Title">
            <input
              className="iw-input w-full"
              id="ticket-editor-title"
              onChange={(event) => setTitle(event.target.value)}
              placeholder="e.g. Move blueprints to C-J6MT"
              value={title}
            />
          </Field>
        ) : null}

        {kind === "acquisition" ? (
          <Field id="ticket-editor-item" label="Item">
            {selectedItem ? (
              <div className="flex items-center gap-2 rounded-[2px] border border-border bg-panel px-2 py-1.5 text-sm">
                <EveTypeImage size={24} typeId={selectedItem.typeId} typeName={selectedItem.typeName} />
                <span className="min-w-0 flex-1 truncate">{selectedItem.typeName}</span>
                <button
                  className="shrink-0 text-xs text-muted hover:text-foreground"
                  onClick={() => {
                    setSelectedItem(null);
                    setItemQuery("");
                  }}
                  type="button"
                >
                  Change
                </button>
              </div>
            ) : (
              <>
                <input
                  aria-label="Search item"
                  className="iw-input w-full"
                  onChange={(event) => setItemQuery(event.target.value)}
                  placeholder="Search item…"
                  value={itemQuery}
                />
                {lookup.results.length > 0 ? (
                  <div className="mt-1 max-h-40 space-y-0.5 overflow-y-auto">
                    {lookup.results.map((result) => (
                      <button
                        className="flex w-full items-center gap-2 rounded-[2px] px-2 py-1 text-left text-xs hover:bg-panel-strong"
                        key={result.typeId}
                        onClick={() => {
                          setSelectedItem(result);
                          setItemQuery("");
                        }}
                        type="button"
                      >
                        <EveTypeImage size={24} typeId={result.typeId} typeName={result.typeName} />
                        <span className="min-w-0 flex-1 truncate">{result.typeName}</span>
                      </button>
                    ))}
                  </div>
                ) : null}
              </>
            )}
          </Field>
        ) : null}

        {kind === "acquisition" ? (
          <Field id="ticket-editor-quantity" label="Quantity">
            <input
              className="iw-input w-full font-mono"
              id="ticket-editor-quantity"
              inputMode="numeric"
              min="1"
              onChange={(event) => setQuantity(event.target.value)}
              step="1"
              type="number"
              value={quantity}
            />
          </Field>
        ) : null}

        {isProductionKind ? (
          <Field id="ticket-editor-build" label="Build">
            <select
              className="iw-input w-full"
              id="ticket-editor-build"
              onChange={(event) => selectBuild(event.target.value)}
              value={buildId}
            >
              <option value="">Select a Build…</option>
              {compatibleBuilds.map((build) => (
                <option key={build.id} value={build.id}>
                  {build.name}
                </option>
              ))}
            </select>
            {compatibleBuilds.length === 0 ? (
              <p className="mt-1 text-xs text-muted">
                No {kind === "manufacturing" ? "manufacturing" : "reaction"} Builds yet -- create one first.
              </p>
            ) : null}
          </Field>
        ) : null}

        {isProductionKind && buildId ? (
          <Field id="ticket-editor-runs" label="Runs">
            <input
              className="iw-input w-full font-mono"
              id="ticket-editor-runs"
              inputMode="numeric"
              min="1"
              onChange={(event) => setRunsInput(event.target.value)}
              step="1"
              type="number"
              value={runsInput}
            />
          </Field>
        ) : null}

        {isProductionKind && buildId ? (
          previewLoading && !preview ? (
            <p className="text-xs text-muted">Calculating plan…</p>
          ) : previewError ? (
            <InlineAlert title="Could not preview this plan">{previewError}</InlineAlert>
          ) : preview ? (
            <PlanToFreezePreview preview={preview} />
          ) : null
        ) : null}

        <Field id="ticket-editor-epic" label="Epic">
          <select
            className="iw-input w-full"
            id="ticket-editor-epic"
            onChange={(event) => setOrderId(event.target.value)}
            value={orderId}
          >
            <option value="">No Epic</option>
            {orders.map((order) => (
              <option key={order.id} value={order.id}>
                {order.displayName}
              </option>
            ))}
          </select>
        </Field>

        <Field id="ticket-editor-assignee" label="Assignee">
          <select
            className="iw-input w-full"
            id="ticket-editor-assignee"
            onChange={(event) => setAssigneeCharacterId(event.target.value)}
            value={assigneeCharacterId}
          >
            <option value="">Unassigned</option>
            {characters.map((character) => (
              <option key={character.connectionId} value={character.connectionId}>
                {character.characterName}
              </option>
            ))}
          </select>
        </Field>

        <Field id="ticket-editor-notes" label="Notes">
          <textarea
            className="iw-input w-full"
            id="ticket-editor-notes"
            onChange={(event) => setNotes(event.target.value)}
            rows={3}
            value={notes}
          />
        </Field>

        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <button className="iw-button-secondary" disabled={busy} onClick={onClose} type="button">
            Cancel
          </button>
          <button className="iw-button-primary" disabled={busy} onClick={() => void submit()} type="button">
            {busy ? "Creating…" : "Create ticket"}
          </button>
        </div>
      </div>
    </PlannerInspectorShell>
  );
}
