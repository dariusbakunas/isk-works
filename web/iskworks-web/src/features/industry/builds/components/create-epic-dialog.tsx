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
 * Create Epic. Every Epic reserves the stock its plan reuses, so other
 * plans don't count it.
 *
 * Opens on a preview of what the Epic will reserve from free inventory.
 * Confirming sends that preview back as `expectedReuse`;
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
  const [drift, setDrift] = useState<ReservationDrift | null>(null);
  const [created, setCreated] = useState<{ order: OrderDetail; increased: ReuseChange[] } | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    // Reset on close as well as on open: the dialog stays mounted, and a
    // reopen must not show the last preview (with a live Create button)
    // for even a frame.
    setPreview(null);
    setDrift(null);
    setCreated(null);
    setError("");
    setBusy(false);
    if (!open || !command) return;
    let cancelled = false;
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

  async function submit() {
    if (!command || !preview) return;
    setBusy(true);
    setError("");
    try {
      const order = await createOrder(buildId, command, {
        expectedReuse: preview.reuse.map(({ typeId, quantity }) => ({ typeId, quantity })),
      });
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
    <div
      className="fixed inset-0 z-50 grid place-items-center overscroll-contain bg-black/70 p-4"
      role="presentation"
    >
      {/* Capped to the viewport: a long reuse list scrolls inside the
          dialog, and the buttons stay in view below it. */}
      <section
        aria-labelledby="create-epic-title"
        aria-modal="true"
        className="iw-dialog flex max-h-[calc(100dvh-2rem)] w-full max-w-lg flex-col p-4"
        role="dialog"
      >
        <h2 className="shrink-0 text-base font-semibold" id="create-epic-title">Create Epic</h2>

        {created ? (
          <>
            <p className="iw-muted mt-2">
              Epic created. More inventory was free than the preview showed, so it reserved more:
            </p>
            <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
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
            </div>
            <div className="mt-4 flex shrink-0 justify-end gap-2">
              <button className="iw-button-primary" onClick={() => onCreated(created.order)} type="button">
                Open Epic
              </button>
            </div>
          </>
        ) : (
          <>
            <div
              className="min-h-0 flex-1 overflow-y-auto overscroll-contain"
              data-testid="create-epic-scroll"
            >
              {preview === null && !error ? <p className="iw-muted mt-2">Checking free inventory...</p> : null}

              {preview ? (
                preview.reuse.length === 0 ? (
                  <p className="iw-muted mt-2">This Epic uses no inventory. Everything will be bought or built.</p>
                ) : (
                  <>
                    <p className="iw-muted mt-2">
                      This Epic reserves this free inventory, so other plans don&apos;t count it:
                    </p>
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
            </div>

            <div className="mt-4 flex shrink-0 flex-wrap justify-end gap-2">
              <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
                Cancel
              </button>
              {drift ? (
                <button className="iw-button-primary" disabled={busy} onClick={refresh} type="button">
                  Refresh
                </button>
              ) : (
                <button
                  className="iw-button-primary"
                  disabled={busy || preview === null}
                  onClick={() => void submit()}
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
