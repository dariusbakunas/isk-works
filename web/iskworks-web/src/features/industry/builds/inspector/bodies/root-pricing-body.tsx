import { MarketScopeSelector } from "../../../../../components/market-scope-selector";
import type {
  InspectorActions,
  RootPricingSlice,
} from "../inspector-model";

export function RootPricingBody({
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
