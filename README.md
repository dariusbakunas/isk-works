<p align="center">
  <img src="web/iskworks-web/src/assets/mascot.png" alt="ISK Works mascot: a beaver in a space suit holding a wrench" width="200">
</p>

<h1 align="center">ISK Works</h1>

<p align="center">
  Industry planning and accounting for EVE Online, costed with what you <em>actually paid</em>.
</p>

---

Most industry tools price every input at today's market. ISK Works keeps an accounting inventory
of what you own and what you paid for it, covers a build from that stock first, and prices only the
shortage. Your profit numbers reflect your real costs, not a hypothetical restock.

> [!NOTE]
> **ISK Works is built with heavy use of AI.** See [Built with AI](#built-with-ai) below.

## Features

- **Builds**: plan a product down to raw materials. The worksheet shows required, covered and
  shortage quantities per material; the plan view orders buy and build steps by stage; the graph
  shows the whole production chain. Costs are marked incomplete, with the reason, until every input
  is priced and every operation has a facility.
- **Inventory**: an append-only ledger with weighted average unit cost. Opening balances, purchases
  from wallet transactions, adjustments, and recorded production (output cost = inputs + installation).
- **Facilities**: stations and Upwell structures with rigs, security, tax and SCC surcharge. System
  cost indices come from ESI.
- **Board**: Epics and tickets for executing a plan, including recording production into inventory.
- **Opportunities**: a profitability scanner across item scopes, manufacturing and reactions.
- **Market and prices**: public ESI order books, player-structure markets, EVE client market exports,
  and manual price overrides.
- **Characters**: read-only ESI sync of skills, blueprints, industry jobs, assets, wallet and planets.
- **Assets, Finance, Calendar, Planetary Interaction**: an asset browser, wallet transactions and
  analytics, a job and extractor calendar, and PI planning.
- **Self-hostable**: one Docker Compose stack, sign-in via EVE SSO (optionally invite-only), encrypted refresh tokens.

## Built with AI

<p align="center">
  <img src="docs/images/built-with-ai.webp" alt="The ISK Works beaver astronaut with a clipboard, supervising three small robots typing code at glowing monitors" width="860">
</p>

ISK Works is one person's project, and almost all of its code, tests and documentation were written
by AI coding agents, mostly [Claude Code](https://claude.com/claude-code) and earlier OpenAI Codex.
The human part is product direction, design decisions, review, and testing in the game.

What that means in practice:

- **Every feature started as a written design and a test-first plan.** An agent drafted both,
  then implemented them task by task with a failing test first. That process is why the test suite
  is large: about 1,000 Rust unit and API tests, about 500 Postgres-backed storage tests, and about
  1,750 web tests, all run in CI.
- **The UI was prototyped with AI design tools** before being rebuilt in the app.
- **The mascot and illustrations were generated with Midjourney.** They are not covered by the
  project license (see [License](#license)).
- **Expect AI fingerprints.** Some files are long, comments can be wordier than a human would
  write, and the architecture shows where features were reworked. Refactoring for readability
  is ongoing, and help is welcome.

AI-assisted contributions are welcome too. You are responsible for what you submit, so understand
it, test it, and say in the PR that an agent helped.

## Architecture

A Rust API with Postgres, and a React single-page app.

| Path | What it is |
| --- | --- |
| `apps/iskworks-api` | Axum HTTP API; routes in `src/routes/<domain>.rs` |
| `apps/iskworks-worker` | Background ESI sync and market refresh |
| `apps/iskworks-sde-import` | CLI that imports EVE's Static Data Export into Postgres |
| `apps/iskworks-admin` | Operator CLI (invites) |
| `crates/iskworks-core` | Domain types and pure business logic, no I/O |
| `crates/iskworks-app` | Application services that orchestrate core and storage |
| `crates/iskworks-storage` | Postgres repositories |
| `crates/iskworks-sde` | SDE read model and import parsing |
| `crates/iskworks-esi` | EVE SSO and ESI client |
| `web/iskworks-web` | Vite + React + TypeScript + Tailwind |
| `migrations/` | sqlx migrations (append-only) |
| `deploy/` | Self-hosting Docker Compose stack |
| `docs/` | Architecture, product and implementation notes |

## Running locally

You need a recent stable Rust toolchain, Node 22, and Docker.

```bash
cp .env.example .env
make db-up          # Postgres in Docker
make api            # API on 127.0.0.1:8080 (runs migrations on start)
make web            # web app on http://127.0.0.1:5173
```

The app needs EVE's static data. Download the **JSONL** export from
<https://developers.eveonline.com/static-data>, then:

```bash
make sde-import SDE_PATH=/path/to/eve-online-static-data-<version>-jsonl.zip
```

Without `EVE_SSO_CLIENT_ID`, the API runs as a single unauthenticated local workspace, which is fine
for development. `ISKWORKS_ESI_MOCK=1` serves fixture ESI data. To use real EVE sign-in, register an
application at <https://developers.eveonline.com> and fill in the EVE SSO settings in `.env`.

Checks, the same ones CI runs:

```bash
make verify         # fmt, clippy, Rust tests, web typecheck, tests, build
```

The Postgres-backed tests are `#[ignore]`d by default. Run them with a database up:

```bash
cargo test --workspace -- --ignored
```

## Self-hosting

[`deploy/README.md`](deploy/README.md) covers the Docker Compose stack: Traefik with Cloudflare
DNS-01 certificates, Postgres, the API, worker and web images, EVE SSO setup, SDE imports, the
admin section, optional session replay, and your instance's community links. Published images are
`dariusbakunas/iskworks-{api,worker,web,sde-import}` on Docker Hub.

Every instance needs its own EVE developer application. Creating one means you accept CCP's
Developer License Agreement as that instance's operator.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security issues: [SECURITY.md](SECURITY.md).

## License

ISK Works is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE). If you
run a modified version as a service, you must offer its source to your users. Point
`ISKWORKS_SOURCE_URL` at it and the app links to it.

The name "ISK Works", the mascot and the illustrations (generated with Midjourney) are **not**
covered by the AGPL. You can use them when running ISK Works, including self-hosted and modified
instances, but not as the branding of a different product or a fork you distribute. See
[BRANDING.md](BRANDING.md).

## CCP notice

© 2014 CCP hf. All rights reserved. "EVE", "EVE Online", "CCP", and all related logos and images are
trademarks or registered trademarks of CCP hf.

ISK Works is a third-party application. It is not affiliated with or endorsed by CCP hf. (Fenris
Creations), and CCP is not responsible for its content or functioning.
