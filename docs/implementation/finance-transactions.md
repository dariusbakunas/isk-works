# Finance Transactions

The Finance workspace provides a multi-character view of persisted EVE wallet market transactions. It has two pages: `/finance/transactions` and `/finance/analytics`.

## Data boundary

- Finance displays only character wallet transactions synchronized from ESI.
- The Finance sync endpoint rejects fixture transport mode.
- The transaction taxonomy is limited to market buys and market sells. Wallet journal entries are not represented as transactions.
- Application/domain models are defined in `iskworks-core`; storage and HTTP adapters do not expose raw ESI DTOs to React.
- Wallet balances are persisted as observations and the latest observation per connected character contributes to the combined balance.
- Wallet client IDs are resolved in batches through ESI's public universe-name endpoint and cached as public EVE metadata. Resolution is retried during later wallet syncs and never blocks transaction synchronization.

## Server responsibilities

Rust owns filtering, sorting, pagination, summaries, exact decimal money arithmetic, saved-filter validation, CSV generation, and ESI synchronization. The transactions endpoint returns rows, the complete filtered summary, available characters, and pagination metadata in one response.

The CSV endpoint exports the current filtered page and accepts the visible column list. Client uses the cached EVE entity name; Where uses the exact NPC station name from SDE or the known Upwell structure name. Missing enrichment fields are emitted as empty values rather than inferred or replaced with raw IDs.

## Inventory recording

A Market Buy can be recorded into Inventory as a purchase: preview with `POST /api/finance/transactions/:observation_id/inventory-recording/preview`, record with `POST .../inventory-recording`, and undo with `POST .../inventory-recording/:recording_id/revert`. A reverted purchase can be recorded again.

## Analytics

`GET /api/finance/analytics` (`finance_analytics` in `iskworks-core`) aggregates the same market transactions for a date range: KPIs, cash flow, spending and income by category, totals by character and location, top items, a daily net heatmap, and rule-based insights. Income is market sells and expenses are market buys; trading taxes and fees come from the wallet journal. `GET /api/finance/analytics/export` exports the analytics as CSV.

## Client responsibilities

React owns presentation and transient workspace state:

- character, date, direction, type, and debounced text filters;
- sort selection and visible columns;
- saved-filter commands;
- CSV download and synchronization feedback.

Summary values are rendered directly from the server response. React does not recompute balances, income, expenses, net ISK, or daily averages.

## Scrolling

The transaction table owns an independent viewport so the Finance summary, filters, and toolbar remain visible while rows scroll. The client incrementally appends pages from the existing paginated API as a bottom sentinel approaches view. Changing any filter or sort resets accumulation to page one, and responses from an older query generation are ignored.
