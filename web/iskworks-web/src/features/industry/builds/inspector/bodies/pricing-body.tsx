import { useEffect, useState } from "react";

import type { PlannerPricingSelection } from "../../../../../api/industry";
import { MoneyInput } from "../../../../../components/money-input";
import type {
  RowPricingSlice,
} from "../inspector-model";
import { InspectorRow } from "../inspector-section";
import { pricingBox, strategyActive, strategyIdle } from "./body-styles";

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
