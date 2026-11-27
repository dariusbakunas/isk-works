import { ClipboardCopy, ClipboardPaste, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { resolveTypes, searchTypes, type TypeSearchResult } from "../../api/sde";
import { CopyValueButton } from "../../components/copy-value-button";
import { EveTypeImage } from "../../components/eve-type-image";
import {
  applyResolved,
  clampQuantity,
  formatM3,
  namesToResolve,
  normalizeShipping,
  parseHold,
  parseItemList,
  planShipments,
  shipmentText,
  toMicro,
  type PendingLine,
  type Shipment,
  type ShippingState,
} from "../../components/shipping/shipping-state";
import type { ToolWindowRenderProps } from "../../components/tool-windows/tool-windows-context";
import { TypeInfoButton } from "../../components/type-info/type-info-button";
import { useDebouncedLookup } from "../../hooks/use-debounced-lookup";

const DIVIDER = "border-[#171f2c]";
const input =
  "h-7 min-w-0 border border-border bg-background px-2 text-[11px] text-foreground placeholder:text-subtle focus:border-primary focus:outline-none";
const sectionTitle = "text-[8px] font-semibold uppercase tracking-[.14em] text-subtle";

/** Looks up the window's pending lines and folds them into its items. */
function usePendingLookup(state: ShippingState, setState: (next: ShippingState) => void) {
  const latest = useRef(state);
  latest.current = state;
  const inFlight = useRef(new Set<PendingLine>());
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    const lines = state.pending.filter((line) => !inFlight.current.has(line));
    if (lines.length === 0) return;
    for (const line of lines) inFlight.current.add(line);
    setFailed(false);
    void resolveTypes(namesToResolve(lines))
      .then((types) => setState(applyResolved(latest.current, lines, types)))
      .catch(() => setFailed(true))
      .finally(() => {
        for (const line of lines) inFlight.current.delete(line);
      });
    // `setState` is the window's own setter; only new lines or a retry rerun this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.pending, attempt]);

  return { failed, retry: () => setAttempt((count) => count + 1) };
}

function AddItem({ onAdd }: { onAdd: (line: PendingLine) => void }) {
  const [query, setQuery] = useState("");
  const [quantity, setQuantity] = useState("1");
  const [searchFailed, setSearchFailed] = useState(false);
  const lookup = useDebouncedLookup<TypeSearchResult>(query, searchTypes, () => setSearchFailed(true));

  function pick(type: TypeSearchResult) {
    onAdd({ name: type.typeName, quantity: Math.max(1, clampQuantity(Number(quantity))) });
    setQuery("");
  }

  return (
    <div className="relative flex gap-1">
      <input
        aria-label="Add an item"
        className={`${input} flex-1`}
        onChange={(event) => {
          setSearchFailed(false);
          setQuery(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" && lookup.results[0]) pick(lookup.results[0]);
        }}
        placeholder="Add an item…"
        value={query}
      />
      <input
        aria-label="Quantity to add"
        className={`${input} w-20 text-right font-mono tabular-nums`}
        inputMode="numeric"
        onChange={(event) => setQuantity(event.target.value.replace(/\D/g, ""))}
        value={quantity}
      />
      {query.trim().length >= 2 ? (
        <ul
          aria-label="Matching items"
          className="absolute left-0 right-0 top-full z-10 mt-px max-h-56 overflow-y-auto border border-border bg-[#151b23] shadow-2xl"
        >
          {lookup.results.map((type) => (
            <li key={type.typeId}>
              <button
                className="flex h-7 w-full items-center gap-2 px-2 text-left text-[10px] hover:bg-[#1a222c] focus-visible:bg-[#1a222c] focus-visible:outline-none"
                onClick={() => pick(type)}
                type="button"
              >
                <EveTypeImage size={18} typeId={type.typeId} typeName={type.typeName} />
                <span className="truncate text-foreground">{type.typeName}</span>
                {type.groupName ? <span className="ml-auto shrink-0 text-[9px] text-subtle">{type.groupName}</span> : null}
              </button>
            </li>
          ))}
          {lookup.results.length === 0 ? (
            <li className="px-2 py-1.5 text-[10px] text-subtle">
              {searchFailed ? "Couldn't search items." : lookup.searching ? "Searching…" : "No items match."}
            </li>
          ) : null}
        </ul>
      ) : null}
    </div>
  );
}

function PasteList({ onAdd, onClose }: { onAdd: (lines: PendingLine[]) => void; onClose: () => void }) {
  const [text, setText] = useState("");
  const lines = useMemo(() => parseItemList(text), [text]);
  return (
    <div className="mt-1 border border-border bg-background p-2">
      <textarea
        aria-label="Item list"
        autoFocus
        className="h-28 w-full resize-y border border-border bg-panel p-2 font-mono text-[10px] text-foreground placeholder:text-subtle focus:border-primary focus:outline-none"
        onChange={(event) => setText(event.target.value)}
        placeholder={"Tritanium 12000\nDatacore - Mechanical Engineering 40\n\nor rows copied from an inventory or multibuy window"}
        value={text}
      />
      <div className="mt-1 flex items-center justify-end gap-1">
        <button className="iw-button-secondary min-h-7 text-[10px]" onClick={onClose} type="button">
          Cancel
        </button>
        <button
          className="iw-button-primary min-h-7 text-[10px]"
          disabled={lines.length === 0}
          onClick={() => {
            onAdd(lines);
            onClose();
          }}
          type="button"
        >
          {lines.length > 0 ? `Add ${lines.length.toLocaleString("en-US")} ${lines.length === 1 ? "line" : "lines"}` : "Add items"}
        </button>
      </div>
    </div>
  );
}

function ShipmentCard({ index, shipment, holdMicro }: { index: number; shipment: Shipment; holdMicro: number }) {
  const [copied, setCopied] = useState(false);
  const text = shipmentText(shipment);
  // A changed shipment is no longer what was copied.
  useEffect(() => setCopied(false), [text]);
  const percent = Math.min(100, (shipment.usedMicro / holdMicro) * 100);
  return (
    <li aria-label={`Shipment ${index + 1}`} className="border border-border">
      <div className="flex items-center gap-2 border-b border-border bg-background px-2 py-1">
        <span className="text-[10px] font-semibold text-bright">Shipment {index + 1}</span>
        <span className="ml-auto font-mono text-[10px] tabular-nums text-foreground">
          {formatM3(shipment.usedMicro)} / {formatM3(holdMicro)} m³
        </span>
        <span className="w-9 text-right font-mono text-[9px] tabular-nums text-muted">{Math.floor(percent)}%</span>
        <button
          aria-label={copied ? `Shipment ${index + 1} copied` : `Copy shipment ${index + 1}`}
          className="grid h-5 w-5 shrink-0 place-items-center text-muted hover:text-foreground"
          onClick={() => void navigator.clipboard?.writeText(text).then(() => setCopied(true))}
          title={copied ? "Copied" : "Copy as a multibuy list"}
          type="button"
        >
          <ClipboardCopy aria-hidden="true" className={`h-3 w-3 ${copied ? "text-positive" : ""}`} />
        </button>
      </div>
      <div aria-hidden="true" className="h-0.5 bg-[#171f2c]">
        <div className="h-full bg-primary" style={{ width: `${percent}%` }} />
      </div>
      <ul>
        {shipment.lines.map((line) => (
          <li className={`group grid min-h-6 grid-cols-[1fr_auto_104px] items-center gap-2 border-b px-2 text-[10px] last:border-b-0 ${DIVIDER}`} key={line.typeId}>
            <span className="truncate text-foreground">{line.name}</span>
            <span className="flex items-center gap-1.5 font-mono tabular-nums text-foreground">
              <CopyValueButton
                label={`Copy ${line.quantity.toLocaleString("en-US")} ${line.name}`}
                revealOnHover
                value={String(line.quantity)}
              />
              {line.quantity.toLocaleString("en-US")}
            </span>
            <span className="whitespace-nowrap text-right font-mono tabular-nums text-muted">{formatM3(line.volumeMicro)} m³</span>
          </li>
        ))}
      </ul>
    </li>
  );
}

/** The Shipping calculator's content: the items, the hold, the shipments. */
export function ShippingWindow({ state: stored, setState }: ToolWindowRenderProps<ShippingState>) {
  const state = useMemo(() => normalizeShipping(stored), [stored]);
  const [pasting, setPasting] = useState(false);
  const lookup = usePendingLookup(state, setState);

  const holdMicro = parseHold(state.holdM3);
  const plan = useMemo(() => planShipments(state.items, holdMicro), [state.items, holdMicro]);
  const queue = (lines: PendingLine[]) => setState({ ...state, pending: [...state.pending, ...lines] });
  const setQuantity = (typeId: number, quantity: number) =>
    setState({ ...state, items: state.items.map((item) => (item.typeId === typeId ? { ...item, quantity } : item)) });

  return (
    <div className="flex flex-col gap-3 p-3">
      <section aria-label="Items">
        <div className="flex gap-1">
          <div className="min-w-0 flex-1">
            <AddItem onAdd={(line) => queue([line])} />
          </div>
          <button
            aria-expanded={pasting}
            className="iw-button-secondary min-h-7 shrink-0 text-[10px]"
            onClick={() => setPasting((open) => !open)}
            type="button"
          >
            <ClipboardPaste aria-hidden="true" className="mr-1 inline h-3 w-3" />
            Paste list
          </button>
        </div>
        {pasting ? <PasteList onAdd={queue} onClose={() => setPasting(false)} /> : null}

        {lookup.failed ? (
          <p className="mt-2 border border-danger/60 bg-danger/10 px-2 py-1 text-[10px] text-danger" role="alert">
            Couldn't look up {state.pending.length.toLocaleString("en-US")} {state.pending.length === 1 ? "item" : "items"}.{" "}
            <button className="underline" onClick={lookup.retry} type="button">
              Try again
            </button>
          </p>
        ) : state.pending.length > 0 ? (
          <p className="mt-2 text-[10px] text-muted" role="status">
            Looking up {state.pending.length.toLocaleString("en-US")} {state.pending.length === 1 ? "item" : "items"}…
          </p>
        ) : null}

        {state.unmatched.length > 0 ? (
          <div className="mt-2 flex items-start gap-2 border border-warning/60 bg-warning/10 px-2 py-1 text-[10px] text-warning" role="status">
            <p className="min-w-0 flex-1">
              No item named: <span className="text-foreground">{state.unmatched.join(", ")}</span>
            </p>
            <button
              aria-label="Dismiss unmatched names"
              className="grid h-4 w-4 shrink-0 place-items-center hover:text-foreground"
              onClick={() => setState({ ...state, unmatched: [] })}
              type="button"
            >
              <X aria-hidden="true" className="h-3 w-3" />
            </button>
          </div>
        ) : null}

        {state.items.length > 0 ? (
          <>
            <div className={`mt-2 grid grid-cols-[1fr_88px_64px_76px_20px] items-center gap-2 border-b pb-1 ${DIVIDER} ${sectionTitle}`}>
              <span>Item</span>
              <span className="text-right">Quantity</span>
              <span className="text-right">Unit m³</span>
              <span className="text-right">Volume m³</span>
              <span />
            </div>
            <ul>
              {state.items.map((item) => {
                const unit = toMicro(item.unitVolumeM3);
                return (
                  <li className={`group grid min-h-8 grid-cols-[1fr_88px_64px_76px_20px] items-center gap-2 border-b text-[10px] ${DIVIDER}`} key={item.typeId}>
                    <span className="flex min-w-0 items-center gap-1.5">
                      <EveTypeImage size={20} typeId={item.typeId} typeName={item.name} />
                      <span className="truncate text-foreground" title={item.name}>
                        {item.name}
                      </span>
                      <TypeInfoButton name={item.name} typeId={item.typeId} />
                    </span>
                    <input
                      aria-label={`Quantity: ${item.name}`}
                      className={`${input} h-6 w-full text-right font-mono tabular-nums`}
                      inputMode="numeric"
                      onChange={(event) => setQuantity(item.typeId, clampQuantity(Number(event.target.value.replace(/\D/g, ""))))}
                      value={item.quantity === 0 ? "" : item.quantity.toLocaleString("en-US")}
                    />
                    <span className="text-right font-mono tabular-nums text-muted">{unit == null ? "—" : formatM3(unit)}</span>
                    <span className="text-right font-mono tabular-nums text-foreground">
                      {unit == null ? "—" : formatM3(unit * item.quantity)}
                    </span>
                    <button
                      aria-label={`Remove ${item.name}`}
                      className="grid h-5 w-5 place-items-center text-subtle hover:text-danger"
                      onClick={() => setState({ ...state, items: state.items.filter((other) => other.typeId !== item.typeId) })}
                      type="button"
                    >
                      <X aria-hidden="true" className="h-3 w-3" />
                    </button>
                  </li>
                );
              })}
            </ul>
            <div className="flex items-center gap-2 pt-1.5 text-[10px]">
              <button
                className="flex items-center gap-1 text-subtle hover:text-danger"
                onClick={() => setState({ ...state, items: [], unmatched: [] })}
                type="button"
              >
                <Trash2 aria-hidden="true" className="h-3 w-3" />
                Clear all
              </button>
              <span className="ml-auto text-muted">Total</span>
              <span className="font-mono font-semibold tabular-nums text-bright">{formatM3(plan.totalMicro)} m³</span>
            </div>
          </>
        ) : state.pending.length === 0 ? (
          <p className="mt-3 text-[10px] text-subtle">
            Add items one at a time, or paste a list: a shopping list copied from ISK Works, or rows copied from an inventory, contract or
            multibuy window in the game.
          </p>
        ) : null}
      </section>

      <section aria-label="Shipments" className="border-t border-border pt-3">
        <label className="flex items-center gap-2 text-[10px] text-muted">
          <span className={sectionTitle}>Cargo hold</span>
          <input
            aria-label="Cargo hold in m³"
            className={`${input} ml-auto w-28 text-right font-mono tabular-nums`}
            inputMode="decimal"
            onChange={(event) => setState({ ...state, holdM3: event.target.value })}
            placeholder="e.g. 62,500"
            value={state.holdM3}
          />
          <span>m³</span>
        </label>

        {state.holdM3.trim() && holdMicro == null ? (
          <p className="mt-2 text-[10px] text-warning">Enter the hold's size in m³ as a number above zero.</p>
        ) : null}
        {plan.unknown.length > 0 ? (
          <p className="mt-2 text-[10px] text-warning" role="status">
            No volume known, not counted: {plan.unknown.map((item) => item.name).join(", ")}
          </p>
        ) : null}
        {plan.oversized.length > 0 ? (
          <p className="mt-2 text-[10px] text-warning" role="status">
            Too big for this hold even alone: {plan.oversized.map((item) => item.name).join(", ")}
          </p>
        ) : null}

        {holdMicro == null ? (
          state.items.length > 0 && !state.holdM3.trim() ? (
            <p className="mt-2 text-[10px] text-subtle">Enter your available cargo space to split the load into shipments.</p>
          ) : null
        ) : plan.tooMany ? (
          <p className="mt-2 text-[10px] text-warning" role="status">
            That is {plan.tooMany.toLocaleString("en-US")} shipments. Check the hold size.
          </p>
        ) : plan.shipments.length > 0 ? (
          <>
            <p className="mt-2 text-[10px] text-foreground" role="status">
              <span className="font-semibold text-bright">
                {plan.shipments.length.toLocaleString("en-US")} {plan.shipments.length === 1 ? "shipment" : "shipments"}
              </span>{" "}
              <span className="text-muted">
                for {formatM3(plan.shipments.reduce((sum, shipment) => sum + shipment.usedMicro, 0))} m³
              </span>
            </p>
            <ol className="mt-2 flex flex-col gap-2">
              {plan.shipments.map((shipment, index) => (
                <ShipmentCard holdMicro={holdMicro} index={index} key={index} shipment={shipment} />
              ))}
            </ol>
          </>
        ) : null}
      </section>
    </div>
  );
}
