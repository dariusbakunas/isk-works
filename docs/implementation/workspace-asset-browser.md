# Workspace Asset Browser

## Purpose

`/assets` is a read-only, Workspace-wide table of holdings observed through ESI.
It answers what an asset is, where it is, which synchronized character supplied
the observation, and whether the Owner/type total agrees with accounted
Inventory. The flat table is intentionally aligned with the Finance workspace:
compact controls, server-owned filtering and sorting, and a contextual inspector
instead of nested location cards.

The boundary is intentional:

- **Assets** are current physical observations reported by EVE.
- **Inventory** is accepted accounting state used for costing and production.

Browsing, searching, filtering, sorting, and inspecting Assets never creates an
`InventoryEvent` or changes an inventory balance.

## Read Model

`asset_browser_current` projects rows from each connection's active, complete
`esi_asset_snapshots` record. The container hierarchy is resolved once per
snapshot, not on every read: before a snapshot becomes readable,
`refresh_esi_asset_hierarchy` follows `location_type = 'item'` references within
that snapshot and persists each item's effective top-level physical location,
immediate parent, depth, and state (`resolved`, `missing_parent`, or `cycle`) in
`esi_asset_hierarchy`.

The flat projection enriches each stack with its physical location, immediate
container, character, SDE group and packaged volume, blueprint observation, and
Owner/type reconciliation state. It preserves imperfect data:

- Missing parent items remain visible under an unresolved container location.
- Cycles are marked and the walk stops.
- The walk is capped at 32 levels.
- Location IDs remain visible through a descriptive fallback when no cached ESI
  name or SDE solar-system name is available.
- Types absent from the active SDE remain visible as `Unknown EVE type <id>`.

The view is derived. `esi_asset_observations` remains the authoritative source,
and no observation is rewritten during browsing.

## API

- `GET /api/assets` returns the flat page. Pages contain at most 200 rows and
  use an opaque continuation cursor. The workspace summary and filter facets are
  the same on every page, so only the first page (no cursor) carries them.
- `GET /api/assets/export` streams CSV for the current query and selected table
  columns.
- `POST /api/assets/sync` synchronizes selected characters, or all connected
  characters when none are selected. It rejects fixture mode and preserves an
  outcome for every attempted character.
- `GET /api/assets/summary`, `GET /api/assets/filters`, and the
  `/api/assets/locations` endpoints are also available; the `/assets` page does
  not use them.

Search and structured filters execute in PostgreSQL. Search covers item type,
immediate container type, resolved location, character, and group names. Filters
cover character connection, physical location, asset kind, item group,
blueprint kind, and reconciliation state. Multi-select HTTP values are encoded
as comma-delimited lists. All visible columns are sortable by the server.

## Enrichment And Reconciliation

Type, group, and packaged volume come from the active SDE. Blueprint ME, TE,
original/copy state, and licensed runs come from the latest matching
`blueprint_observations` row when available. Packaged stack volume is calculated
in PostgreSQL using decimal arithmetic; TypeScript only formats returned values.

Reconciliation is deliberately scoped to **Owner plus type**, matching the
existing accounted Inventory identity. The badge does not claim location-level
accounting because Inventory does not currently have that identity.

## Known Limitations

- The SDE projection retains NPC station, solar-system, constellation, and
  region geography. The page does not yet expose category or region filters.
- Asset-kind classification is conservative: blueprint observations and
  container relationships are authoritative; material and ship labels depend on
  imported group names.
- Structure names depend on the existing authenticated ESI location-name cache.
- Only currently synchronized character connections appear.
- The browser does not value holdings, move assets, inspect fittings, or accept
  observations into Inventory.

## Verification

Core tests cover query validation and opaque cursor handling. PostgreSQL tests
cover empty pages, nested containers, exact packaged volume, location
enrichment, and missing-SDE type fallbacks. API tests cover validation and
multi-select parsing. Frontend tests cover incremental loading, stale-response
protection, the flat table, the contextual inspector, and debounced server-side
search.
