# Category-scoped profitability evaluation

The Opportunities API evaluates one run for every product in a server-owned category scope without
creating Builds or scan records.

## Endpoints

`GET /api/opportunities/scopes` returns the supported scope catalog
(`supported_profitability_scopes`):

- `t1-frigates`, `t1-destroyers`, `t1-cruisers`, `t1-battleships`, `t1-industrial-ships`:
  published Tech I hulls (category 6, one group each, meta group 1);
- `t1-battlecruisers`: Tech I battlecruisers across groups 419 and 1201, filtered by the
  Standard Battlecruisers market group because the attack battlecruisers carry no meta group;
- `t1-rigs`: published Tech I rigs (category 7, meta group 1) under the Rigs market group;
- `reactions`: published reaction formulas producing category 4 materials.

`POST /api/opportunities/evaluate` accepts a scope, a facility, blueprint ME/TE, and a market
scope:

```json
{
  "scopeId": "t1-frigates",
  "facilityProfileId": "<facility-profile-id>",
  "materialEfficiency": 10,
  "timeEfficiency": 20,
  "marketScope": { "regionId": 10000002, "locationId": 60003760 }
}
```

ME/TE are required for manufacturing scopes and rejected for `reactions`. The facility is resolved
against its current settings on every evaluation.

The response reports the fixed context (`runs: 1`, materials priced with
`acquireQuantityFromSellOrders`, output valued with `lowestSell`), complete/incomplete counts,
evidence readiness, ranked candidates, warnings, assumptions, exclusions, and elapsed milliseconds.

`POST /api/opportunities/refresh` takes the same body and prioritizes refresh of the market,
adjusted-price, and system-cost-index evidence the evaluation needs.

## Calculation and completeness

Every candidate uses the normal transient Build calculator with the selected facility. Immediate
materials are adjusted for blueprint and facility ME, priced across sufficient sell-order depth,
and never satisfied from inventory or recursive component Builds. Installation cost uses the same
adjusted-price EIV and facility calculation as Build preview.

A candidate is incomplete rather than zero-filled when a material/output order book is absent,
material depth is insufficient, installation cost evidence is incomplete, or the recipe has
multiple outputs. Stale market observations stay usable and are reported as warnings. Complete
candidates sort by estimated gross profit per manufacturing hour, followed by stable
product-name/type-ID ties; incomplete candidates sort last.

Gross profitability excludes broker fees, sales tax, hauling, blueprint acquisition, invention,
copying, liquidity/sale time, and slot concurrency.

## Verification coverage

- Core tests cover strict catalog predicates, fixed one-run policies, exact derived metrics,
  deterministic ranking, quantity-aware market depth, partial-depth evidence, and recipe capture.
- PostgreSQL integration fixtures prove the SDE candidate predicates exclude unpublished and
  Tech II types.
- A schema assertion proves no `opportunity`, `profitability_candidate`, or `scan_run` table is
  introduced.
