# EVE Client Market Export Pricing

## Scope

EVE client market-order exports are imported as workspace-private market evidence that pricing uses alongside ESI order books. Importing never mutates Inventory.

## Why Client Export

Client exports capture the exact station or structure visible to the user, including private structures the connected characters cannot read through ESI. Imported orders land in the same transport-neutral `market_order_observations` table that ESI order books populate.

## Supported Format

The parser accepts UTF-8 `.txt` and `.csv` files with LF or CRLF endings, an optional UTF-8 BOM, decimal-looking whole quantities, and an optional trailing comma. Required columns are:

```text
price,volRemaining,typeID,range,orderID,volEntered,minVolume,bid,issueDate,duration,stationID,regionID,solarSystemID,jumps
```

Column identity comes from the header and item/location identity comes from file contents, never the filename. A single export may contain rows from multiple market locations; the parser splits those rows into location-specific order books for preview, deduplication, and import.

## Parser Behavior

`iskworks-core::market` owns the bounded CSV parser. It validates required and duplicate headers, exact decimal prices, whole quantities, IDs, booleans, issue timestamps, duration, and jumps. Valid rows are grouped by item, location, solar system, and region. One malformed row invalidates the physical upload; no partial rows from that upload are committed.

Errors retain a safe filename, row number, column, stable code, and concise explanation. Uploaded bytes are never executed or logged. Filenames containing path separators, traversal segments, control characters, or more than 180 bytes are rejected.

## Exact Arithmetic

Prices use the existing `Money` and `rust_decimal` types. Quantities are normalized to unsigned integers. Order-line totals are multiplied exactly, totals are summed before division, and weighted unit prices are rounded once to the application's four-decimal money scale. Binary floating point is not used by backend calculations.

## Upload Limits

| Limit | Value |
| --- | ---: |
| Files per batch | 20 |
| Bytes per file | 5 MiB |
| Total batch bytes | 25 MiB |
| Rows per file | 100,000 |
| Rows per batch | 250,000 |
| Filename length | 180 bytes |

Axum's body limit is the batch limit plus multipart framing allowance. The parser enforces limits again independently.

## Preview Workflow

`POST /api/industry/market-imports/preview` accepts multipart files, parses them in memory, resolves type IDs through the active SDE, checks prior file checksums, and returns file and batch summaries. Preview does not create batches, files, observations, or Inventory Events.

The response includes one entry per location-specific order book with item and numeric location identity, order counts, total side volume, best prices, issue-date range, observation timestamp provenance, warnings, errors, and duplicate status.

## Import Workflow And Atomicity

`POST /api/industry/market-imports` reparses submitted files and commits valid files in one batch transaction. Invalid files remain unpersisted and are returned in the result, implementing per-file validation with transactional commit for the accepted set. An all-invalid or all-duplicate request creates no batch.

The transaction creates:

1. `market_import_batches`
2. `market_import_files`
3. append-only `market_order_observations`
4. `market_import_file_observations` provenance links

No temporary upload path is used.

## Batch And File Models

Market import batches are workspace scoped and summarize file, item, location, observation, duplicate, warning, and observation-time ranges. Imported files preserve the safe display filename, raw and normalized SHA-256 checksums, byte and row counts, detected identity, timestamp source, and imported time.

The raw personal export is not committed. The repository fixture is a synthetic equivalent at:

```text
crates/iskworks-core/tests/fixtures/market/Insmother-Tritanium-2026.07.26 192639.txt
```

## Observation Model And Versioning

Each normalized order observation captures transport, observation/import times, order and item identity, side, exact price, remaining and entered volume, minimum volume, range, issue time, duration, location/system/region IDs, jumps, and normalized row checksum.

Observations are never updated. Identical order evidence at the same observation time is deduplicated and linked to each contributing file. A later observation with changed price or volume creates a new row; older evidence is not updated.

## Timestamp Policy

Observation time is selected in this order:

1. recognized `YYYY.MM.DD HHMMSS` EVE export filename suffix;
2. user-supplied RFC 3339 multipart `observedAt`;
3. server import time, marked `ImportTime`.

Order `issueDate` is never used as observation time.

## Location Handling

Numeric `stationID`, `solarSystemID`, and `regionID` are authoritative. Unknown structure IDs do not block import and display as `Structure {id}`. ISK Works can resolve accessible Upwell names through connected characters with `esi-universe.read_structures.v1` (`/api/industry/market-locations/resolve`); resolved names enrich old and new imports without mutating market observations.

## Duplicate Protection

Single-location uploads use their raw SHA-256 file checksum. Multi-location uploads derive a stable checksum for each item/location order book from the raw checksum and authoritative identity fields. A second upload is reported per order book as already imported and creates no observations. Normalized checksums protect equivalent content at an observation time. Observation identity includes workspace, transport, observation time, order ID, and normalized state checksum.

## Use In Pricing

When pricing resolves a market scope (a region and optional location), the latest import batch for the scope's locations is included in the market evidence alongside the scope's ESI order books.

## Pricing Policies

Implemented policies are:

- `LowestSell`
- `HighestBuy`
- `AcquireQuantityFromSellOrders`
- `LiquidateQuantityIntoBuyOrders`

Lowest sell and highest buy select one exact best order. Quantity-aware acquisition walks eligible sell orders by ascending price. Quantity-aware liquidation walks applicable buy demand by descending price and respects minimum volume.

For buy orders, station range (`-1`) requires zero jumps, region range (`32767`) is accepted, and numeric ranges require `jumps <= range`. Taxes, broker fees, and hauling are excluded; liquidation is labeled gross proceeds.

## Quantity-Aware Results

The server returns requested, covered, and uncovered quantities; exact total; weighted average, best, and marginal unit prices; orders used; available depth; coverage state; order IDs; observation IDs; warnings; and formula version `market-depth-v1`.

No-order and partial-depth results are distinct. The coverage policy decides what happens when depth is short: `RequireFullCoverage` fails with `market_volume_insufficient`, and `AllowPartialWithWarning` permits the covered calculation with an explicit warning. Zero is never substituted for a missing market price.

## Inventory Isolation

Market preview and import never write `inventory_events` or `inventory_balances`.

## API

```text
POST /api/industry/market-imports/preview
POST /api/industry/market-imports
GET  /api/industry/market-imports
GET  /api/industry/market-imports/{batch_id}
GET  /api/industry/market-observations/order-book
```

Structured market errors use HTTP 400, 409, 413, or 422 as appropriate and do not expose SQL, stack traces, raw file bodies, or temporary paths.

## Frontend

`/prices/imports` provides file selection, drag and drop, multi-file preview, per-file validation, explicit import, and recent batch history.

## Tests And Known Values

Core tests cover BOM, CRLF/LF, trailing commas, unsafe filenames, fractional volume, missing headers, mixed identity, conflicting order IDs, exact prices, order walking, minimum volume, and insufficient depth.

The sanitized fixture verifies:

```text
Lowest sell:               3.9700 ISK
Highest buy:               3.8100 ISK
Requested acquisition:     250,000,000 units
Exact total:               992,733,582.3100 ISK
Weighted unit price:       3.9709 ISK
Marginal unit price:       3.9800 ISK
Orders consumed:           2
Coverage:                  Full
```

PostgreSQL tests cover duplicate files, observation provenance, and changed-order append behavior. Multipart API tests exercise the same upload path as the UI.

## Security And Transaction Boundaries

Private observations are workspace scoped throughout. Database constraints validate source kinds, sides, IDs, prices, quantities, timestamps, configuration modes, and same-workspace query paths. Import uses an explicit transaction. Database triggers reject observation updates; `ON DELETE RESTRICT` foreign keys keep observations referenced by import files from being deleted.
