# Manual Inventory and Weighted-Average Costing

## 1. Scope

Accounting inventory is an append-only ledger for the configured Workspace and
Owner. Users can record an opening balance, record purchases and adjustments,
inspect exact weighted-average cost and retained history, compare historical cost
with current prices, and reverse the latest event.

## 2. Why Inventory Is a Ledger

Build consumption needs an authoritative answer to both "how many units exist?" and
"what historical value leaves inventory?" Append-only history and exact cost
projections give every workflow one cost model.

## 3. Inventory Event Model

`InventoryEvent` is the immutable source of truth. It captures Workspace, Owner,
stable EVE type ID and name, event kind, exact quantity and cost deltas, cost
quality, provenance, effective time, recorded time, recorded sequence, reversal
relationship, and resulting projection state.

## 4. Event Kinds

- `OpeningBalance` introduces a starting quantity.
- `Purchase` records positive inbound inventory.
- `Adjustment` records a quantity correction in either direction.
- `Consumption` and `ProductionOutput` record material leaving inventory for, and
  output arriving from, production.
- `Reversal` exactly negates the latest economic event.

## 5. Inventory Identity and Costing Pool

The identity is `(workspace_id, owner_id, type_id)`. Cost is pooled owner-wide for
each EVE type; there are no location-level pools.

## 6. Exact Quantity and Money Representation

Quantity is an exact positive integer at command boundaries and a checked signed
delta inside events. Historical values use `rust_decimal::Decimal` at four
fractional ISK digits, PostgreSQL `numeric(24,4)`, and JSON strings. React does not
calculate accounting results.

## 7. Cost-Quality Model

`CostInputQuality` distinguishes `Known`, `Estimated`, and `ZeroCost`. There is
no unknown-cost state: every positive accounted quantity has a defined cost basis,
so average cost is always derivable while quantity is positive. Estimated and
zero-cost provenance is retained in events and surfaced in read-model warnings.

## 8. Opening Balances

An opening balance requires an active-SDE type, positive quantity, effective date,
and an explicit cost quality. Known and estimated costs require a unit cost.
Zero cost requires an explicit acknowledgement. A second opening balance for the
same item is rejected.

## 9. Purchases

Purchases accept unit cost only and require a known or acknowledged zero cost.
Total cost is derived exactly as `quantity * unit_cost`.

## 10. Weighted-Average Formula

For each inbound event:

```text
new quantity = prior quantity + inbound quantity
new total historical cost = prior total cost + inbound total cost
new average = new total historical cost / new quantity
```

Quantity and total historical cost are authoritative. Average is derived and
rounded to four decimal places for display; total cost is never reconstructed from
the rounded average.

## 11. Event-Ordering Policy

Recorded sequence is authoritative for accounting. Effective dates are retained and
displayed but do not reorder history.

## 12. Reversal and Correction Policy

Reversal is latest-event-only. It is safer than implying that
arbitrary dependent history can always be corrected. A reversal appends an exact
negative event, marks the original through its relationship, retains both events,
and cannot itself be reversed. A correction is a reversal followed by a replacement
event.

## 13. Projection Model

`inventory_balances` is a transactionally maintained, rebuildable projection. It
stores quantity, total historical cost, revision, and last activity. Average cost is derived in Rust.

## 14. Projection Rebuild

`InventoryRepository::rebuild` replays each owner/type event stream in sequence,
validates every intermediate projection, compares it with the stored balance, and
repairs mismatches without changing event history. It is covered by core and
PostgreSQL tests.

## 15. Database Schema

Migration `202607250005_create_inventory_ledger.sql` creates:

- `inventory_events`, with append-only economic deltas, result snapshots, sequence,
  and a unique reversal relationship.
- `inventory_balances`, keyed by Workspace, Owner, and type with consistency checks.

Migration `202608240003_require_inventory_cost_basis.sql` removed the earlier
known/unknown quantity split.

## 16. Transaction and Concurrency Behavior

Posting locks the item projection with `FOR UPDATE`, checks `expected_revision`,
validates the item against the active SDE, inserts the event, and upserts the
projection in one transaction. A stale preview returns HTTP `409 Conflict` and does
not append an event.

## 17. API Contract

- `GET /api/inventory`
- `GET /api/inventory/:type_id`
- `POST /api/inventory/opening-balance/preview`
- `POST /api/inventory/opening-balance`
- `POST /api/inventory/purchases/preview`
- `POST /api/inventory/purchases`
- `POST /api/inventory/adjustments/preview`
- `POST /api/inventory/adjustments`
- `POST /api/inventory/:type_id/events/:event_id/reverse`
- `GET /api/inventory/export`
- `POST /api/inventory/import`

Money values are JSON strings. Errors use the existing structured envelope.

## 18. Current-Price Comparison

`GET /api/inventory` prices each item against the workspace's default market
scope, or against an explicitly selected `?priceSourceId=`. Current value is
`quantity * current unit price`; difference is current value minus total
historical cost. It is not labeled as profit. Missing prices are explicit. Price
selection never mutates inventory history.

## 19. Explainability

Summary metrics progressively disclose the exact formula and then the audit stream.
Each event shows quantity, unit and total cost, quality, effective and recorded
times, provenance, resulting quantity, resulting average, and reversal state.

## 20. Warnings and Uncertainty

The UI uses text, not color alone, for estimated opening value,
explicit zero cost, missing current price, reversed history, and revision conflict.

## 21. Tests

Core tests cover exact weighted average, zero-cost acknowledgement, reversal,
negative-balance prevention, overflow boundaries, and deterministic rebuild.
PostgreSQL tests cover transactionality, stale rollback, append-only reversal, and
rebuild equivalence. In-process Axum tests cover empty results, exact previews,
acknowledgement, conflicts, and missing events.
