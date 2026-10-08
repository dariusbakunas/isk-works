import { useEffect, useState } from "react";
import {
  createOrder,
  previewOrder,
  reservationDrift,
  type EpicReusePreview,
  type OrderDetail,
  type ReservationDrift,
  type ReuseChange,
} from "../../../../api/industry/orders";
import type { PreviewBuildPlanCommand } from "../../../../api/industry/builds";
import { apiMessage } from "../../shared/api-error";
import { InlineAlert } from "../../../../components/primitives";

/**
 * Create Epic, with an opt-in (default on) "Reserve inventory" step.
 *
 * Opens on a preview of what the Epic would reuse from free inventory.
 * Confirming with Reserve on sends that preview back as `expectedReuse`;
 * if free stock dropped since, the server refuses with a drift body that
 * already carries the fresh preview, so Refresh needs no extra request.
 * If stock rose, the Epic is created and the increase is shown here before
 * moving on.
 */
export function CreateEpicDialog({
  open,
  buildId,
  command,
  onCancel,
  onCreated,
}: {
  open: boolean;
  buildId: string;
  command: PreviewBuildPlanCommand | null;
  onCancel: () => void;
  onCreated: (order: OrderDetail) => void;
}) {
  const [preview, setPreview] = useState<EpicReusePreview | null>(null);
  const [reserve, setReserve] = useState(true);
  const [drift, setDrift] = useState<ReservationDrift | null>(null);
  const [created, setCreated] = useState<{ order: OrderDetail; increased: ReuseChange[] } | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open || !command) return;
    let cancelled = false;
    setPreview(null);
    setReserve(true);
    setDrift(null);
    setCreated(null);
    setError("");
    previewOrder(buildId, command)
      .then((result) => {
        if (!cancelled) setPreview(result);
      })
      .catch((requestError) => {
        if (!cancelled) setError(apiMessage(requestError));
      });
    return () => {
      cancelled = true;
    };
  }, [open, buildId, command]);

  if (!open) return null;

  const names = new Map<number, string>();
  for (const line of [...(preview?.reuse ?? []), ...(drift?.preview.reuse ?? [])]) {
    names.set(line.typeId, line.typeName);
  }
  const typeName = (typeId: number) => names.get(typeId) ?? `Type ${typeId}`;

  async function submit(withReservation: boolean) {
    if (!command || !preview) return;
    setBusy(true);
    setError("");
    try {
      const order = await createOrder(
        buildId,
        command,
        withReservation
          ? { expectedReuse: preview.reuse.map(({ typeId, quantity }) => ({ typeId, quantity })) }
          : undefined,
      );
      if (order.reuseIncreased?.length) {
        setCreated({ order, increased: order.reuseIncreased });
        setBusy(false);
        return;
      }
      onCreated(order);
    } catch (requestError) {
      const driftBody = reservationDrift(requestError);
      if (driftBody) {
        setDrift(driftBody);
      } else {
        setError(apiMessage(requestError));
      }
      setBusy(false);
    }
  }

  function refresh() {
    if (!drift) return;
    setPreview(drift.preview);
    setDrift(null);
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="create-epic-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-lg p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="create-epic-title">Create Epic</h2>

        {created ? (
          <>
            <p className="iw-muted mt-2">
              Epic created. More inventory was free than the preview showed, so it reserved more:
            </p>
            <ReuseChangeTable
              changes={created.increased.map((change) => ({
                typeId: change.typeId,
                before: change.expected,
                after: change.now,
              }))}
              typeName={typeName}
              beforeLabel="Previewed"
              afterLabel="Reserved"
            />
            <div className="mt-4 flex justify-end gap-2">
              <button className="iw-button-primary" onClick={() => onCreated(created.order)} type="button">
                Open Epic
              </button>
            </div>
          </>
        ) : (
          <>
            {preview === null && !error ? <p className="iw-muted mt-2">Checking free inventory...</p> : null}

            {preview ? (
              preview.reuse.length === 0 ? (
                <p className="iw-muted mt-2">This Epic uses no inventory. Everything will be bought or built.</p>
              ) : (
                <>
                  <p className="iw-muted mt-2">This Epic uses this free inventory:</p>
                  <table className="mt-2 w-full text-sm" aria-label="Inventory this Epic uses">
                    <thead>
                      <tr className="iw-muted text-left text-xs">
                        <th className="py-1 font-medium">Item</th>
                        <th className="py-1 text-right font-medium">Quantity</th>
                      </tr>
                    </thead>
                    <tbody>
                      {preview.reuse.map((line) => (
                        <tr key={line.typeId}>
                          <td className="py-1">{line.typeName}</td>
                          <td className="py-1 text-right tabular-nums">{line.quantity.toLocaleString()}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </>
              )
            ) : null}

            {preview && preview.reuse.length > 0 ? (
              <label className="mt-3 flex items-start gap-2 text-sm">
                <input
                  checked={reserve}
                  className="mt-0.5"
                  disabled={busy || drift !== null}
                  onChange={(event) => setReserve(event.target.checked)}
                  type="checkbox"
                />
                <span>
                  <span className="font-semibold">Reserve inventory</span>
                  <span className="iw-muted block">
                    Hold this stock for the Epic so other plans don&apos;t count it.
                  </span>
                </span>
              </label>
            ) : null}

            {drift ? (
              <div className="mt-3">
                <InlineAlert title="Free inventory changed">
                  {drift.message}
                </InlineAlert>
                <ReuseChangeTable
                  changes={[
                    ...drift.decreased.map((change) => ({
                      typeId: change.typeId,
                      before: change.expected,
                      after: change.now,
                    })),
                    ...drift.shortfalls.map((shortfall) => ({
                      typeId: shortfall.typeId,
                      before: shortfall.wanted,
                      after: shortfall.free,
                    })),
                  ]}
                  typeName={typeName}
                  beforeLabel="Previewed"
                  afterLabel="Free now"
                />
              </div>
            ) : null}

            {error ? <div className="mt-3"><InlineAlert title="Epic not created">{error}</InlineAlert></div> : null}

            <div className="mt-4 flex flex-wrap justify-end gap-2">
              <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
                Cancel
              </button>
              {drift ? (
                <>
                  <button className="iw-button-secondary" disabled={busy} onClick={() => void submit(false)} type="button">
                    Create without reserving
                  </button>
                  <button className="iw-button-primary" disabled={busy} onClick={refresh} type="button">
                    Refresh
                  </button>
                </>
              ) : (
                <button
                  className="iw-button-primary"
                  disabled={busy || preview === null}
                  onClick={() => void submit(reserve && preview !== null && preview.reuse.length > 0)}
                  type="button"
                >
                  {busy ? "Creating Epic..." : "Create Epic"}
                </button>
              )}
            </div>
          </>
        )}
      </section>
    </div>
  );
}

function ReuseChangeTable({
  changes,
  typeName,
  beforeLabel,
  afterLabel,
}: {
  changes: { typeId: number; before: number; after: number }[];
  typeName: (typeId: number) => string;
  beforeLabel: string;
  afterLabel: string;
}) {
  return (
    <table className="mt-2 w-full text-sm" aria-label="Inventory changes">
      <thead>
        <tr className="iw-muted text-left text-xs">
          <th className="py-1 font-medium">Item</th>
          <th className="py-1 text-right font-medium">{beforeLabel}</th>
          <th className="py-1 text-right font-medium">{afterLabel}</th>
        </tr>
      </thead>
      <tbody>
        {changes.map((change) => (
          <tr key={change.typeId}>
            <td className="py-1">{typeName(change.typeId)}</td>
            <td className="py-1 text-right tabular-nums">{change.before.toLocaleString()}</td>
            <td className="py-1 text-right tabular-nums">{change.after.toLocaleString()}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
