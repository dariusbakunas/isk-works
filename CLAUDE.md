# ISK Works

EVE Online industry/accounting tool: build planning, inventory accounting, market pricing.
Rust API + Postgres backend, React/TypeScript frontend.

## Layout

- `apps/iskworks-api` — Axum HTTP API: thin routes in `src/routes/<domain>.rs` or `src/routes/<domain>/` (e.g. `inventory.rs`, `builds/`, `orders/`), `AppState` wiring in `src/state.rs`, and `ApiError`'s HTTP mapping in `src/error.rs` + `src/error/`.
- `apps/iskworks-worker` — background loops: market order books, adjusted prices, system cost indices, character source sync, and expired auth/market data cleanup.
- `apps/iskworks-admin` — operator CLI for invite codes (create, list, disable).
- `apps/iskworks-sde-import` — CLI to import EVE's Static Data Export into Postgres.
- `crates/iskworks-core` — domain types, rules, repository ports (traits) and the services built on those ports. No concrete I/O: no SQL, HTTP or env reads. Modules other crates address by path (`order`, `build_cost`, `production_dependency`, …) are `pub`; the rest are private and re-exported from the crate root.
- `crates/iskworks-app` — application services that orchestrate core with storage/ESI (Build planning coordinators built from `BuildPlanningDeps`, `EsiApplicationService`, `PublicMarketService`, character sync, calendar, planetary).
- `crates/iskworks-storage` — Postgres repository implementations (`Pg*Repository`).
- `crates/iskworks-sde` — SDE read repository trait + import parsing (`SdeReadRepository`).
- `crates/iskworks-esi` — EVE ESI (SSO/API) client.
- Test-only helpers shared across crates (`SecretCipher::for_tests`, `UnusedEsiTransport`, `EsiApplicationService::new_for_tests`, `FakeLinkTransport`) sit behind each crate's `test-support` feature, enabled only from `[dev-dependencies]`, so release builds never contain them.
- `web/iskworks-web` — Vite + React + TypeScript + Tailwind SPA.
  - `src/api/<domain>.ts` — typed fetch client per API domain, mirrors `routes/<domain>.rs`.
  - `src/features/<feature>/` and `src/features/industry/<feature>/` — feature UI, hooks and route pages (`*-page.tsx`, wired in `src/App.tsx`), e.g. `board/`, `industry/builds/` (incl. the planner core in `builds/planner/`), `industry/inventory/`.
  - `src/components/` — shared primitives with no feature imports, incl. `operational-table/` (see below) and the inspector side-panel shell.
  - `src/hooks/` — shared hooks; `src/observability/` — LogRocket integration.
- `migrations/` — sqlx Postgres migrations.
- `docs/implementation/` — how individual features work; `docs/security/` — invite codes and session replay; `docs/esi-usage.md` — what ISK Works asks of ESI, for operators.
- Design docs and TDD plans are not in this repo. Maintainers keep them in the private companion repo, checked out as a sibling: `../isk-works-internal/docs/superpowers/` (`specs/` + `plans/`, dated `YYYY-MM-DD-<slug>`). If you have that checkout, **check it before starting non-trivial work**, since a feature may already be specced and planned, and write new specs/plans there. Never add them to this repo (`docs/superpowers` stays gitignored as a guard).

## Build plans

One planner. Every Build belongs to a plan through `builds.plan_root_build_id` (a root owns itself). Sourcing lives on `production_dependencies` demand edges: one per consumer Build and component, Buy or Produce, with Produce pointing at the plan's shared producer Build. A Build's draft `componentResolutions` / `fulfillmentScopes` are its sourcing intent.

- Change sourcing only through the canonical consumer write (`IndustryService::write_canonical_consumer`, used by `update_draft` and the component-resolution routes). The plain repository `update_draft` refuses sourcing changes (`guard_sourcing_unchanged`), because it writes no edges.
- Plans are projected by `IndustryService::project_build_materials` (canonical projection plus `BuildCostProjection`). There is no tree walk and no parent link; the legacy planner was removed in October 2026.
- Frozen v1/v2 Epics stay readable; new Epics freeze as `planning_snapshot_version = 3`.

## Commands

- `make db-up` / `make db-down` — Postgres via docker compose.
- `make api` — run the API (`cargo run -p iskworks-api`).
- `make web` — run the Vite dev server.
- `make sde-import SDE_PATH=...` — import an SDE export.
- `make fmt` / `make clippy` / `make rust-test` — Rust checks.
- `make web-check` / `make web-test` / `make web-build` — web checks.
- `make verify` — everything above, in order. Run before considering a change done.

Targeted runs during iteration (cheaper than full suite):
- `cargo test -p iskworks-api --test <file> <test_name>`
- `npm --prefix web/iskworks-web test -- --run <path>`

## Testing conventions

- Rust API integration tests (`apps/iskworks-api/tests/*.rs`) prefer lightweight in-memory fakes (fake `WorkspaceRepository`/`InventoryRepository`/`SdeReadRepository` structs implementing the trait) over a real Postgres fixture — no `DATABASE_URL` needed, tests run fast and always. Tests that must exercise a real Postgres pool (via `#[sqlx::test]`) are marked `#[ignore = "requires DATABASE_URL and a PostgreSQL test database"]`.
- Web tests use Vitest + Testing Library, colocated under `__tests__/` next to the code, plus `src/__tests__/App.test.tsx` for cross-page integration (mocked `fetch`, `MemoryRouter`).
- TDD workflow for planned features: write a failing test first, confirm it fails for the right reason, implement, confirm green, then commit — one commit per task in the plan doc.

## UI conventions

- `OperationalTable` / `OperationalTableGroup` / `OperationalTableRow` (`src/components/operational-table/`) is the shared primitive for dense grouped data tables — used by the Build worksheet and Inventory list. Reuse it for new tabular views rather than building ad-hoc grids; columns are declared as `OperationalColumn[]`, cell content is caller-supplied JSX.
- Money values: compact display (`formatIskCompact`) in cells with the exact value in the `title` attribute (`formatIskSummary`), not full text inline — see `src/components/money.tsx`.

## Commit style

Conventional-ish: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, optionally scoped like `fix(web): ...`. One logical change per commit.

## Branch workflow

`main` is protected on GitHub: work on a feature branch, push it, and open a PR with `gh pr create`; PRs are validated by GitHub Actions (`.github/workflows/ci.yml`: Rust fmt/clippy/tests incl. the Postgres suite, web, CI scripts, container image builds). **The user merges** (or explicitly asks for it to be merged) — don't merge your own PRs unless told to.

## Deployment

- `docker/rust.Dockerfile` builds all Rust images (`iskworks-api`, `iskworks-worker`, `iskworks-sde-import`) via `--target`, from one shared builder stage with BuildKit cache mounts (context = repo root, since `sqlx::migrate!` embeds `../../migrations` and the workspace crates are path deps). `web/iskworks-web/Dockerfile` builds the web image (context = `web/iskworks-web`, self-contained).
- Release images are built and pushed by the maintainer's private release pipeline, outside this repo: for a `vX.Y.Z` tag it builds from this repo at that tag and pushes `dariusbakunas/iskworks-{api,worker,web,sde-import}` to Docker Hub tagged with the version, `major.minor`, and `latest`, without re-validating (the tagged commit already passed on its PR). The API image gets `APP_VERSION` baked in (`v1.2.3 (shortsha)`), surfaced via `/api/health` and `/api/workspace` and shown in the web header; local `cargo run` / untagged builds fall back to `"dev"`.
- `deploy/` holds a self-hosted docker-compose stack (standalone Traefik with Cloudflare DNS-01 ACME, Postgres, both app images on one domain — Traefik path-routes `/api/*` to the API and everything else to the web app, so no CORS is needed). See `deploy/README.md` for first-time setup.
