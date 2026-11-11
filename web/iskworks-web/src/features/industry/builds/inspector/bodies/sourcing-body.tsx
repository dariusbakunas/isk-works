import type {
  InspectorActions,
  SourcingSlice,
} from "../inspector-model";
import { strategyActive, strategyBox, strategyIdle } from "./body-styles";

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

export function SourcingBody({
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
