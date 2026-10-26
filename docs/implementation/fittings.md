# Fittings: stats engine

Fit stats come from EVEShip.fit's dogma engine (`esf-dogma-engine`), run in
the API. ISK Works does not implement EVE's attribute/effect rules itself.

## Pieces

- `iskworks_core::fitting`: the fit (`FitInput`, `FitItem`, `SlotKind`,
  `ModuleState`), the pilot (All V, All 0, or skill levels), the damage
  profile and its presets, the curated `FitStats`, and the `FittingEngine`
  port. No engine types.
- `crates/iskworks-fitting`: `EsfFittingEngine`, the only code that names
  esf types. It maps a fit onto the engine, calculates with validation on,
  and reads `FitStats` back by attribute name. The derived attributes
  (`ehp`, `capacitorDepletesIn`, `damagePerSecondWithoutReload`, ...) come
  from EVEShip.fit's sde-patched and have no stable ids, so names are the
  contract: a renamed attribute fails the load instead of reading zero.
- The API loads the engine at startup from `FITTING_SDE_PATH` (default
  `data/fitting/sde.dat`). Missing or stale data logs a warning and turns
  fit stats off; the rest of the API is unaffected.
  `GET /api/fittings/meta` reports whether stats work and both data builds.

## Engine data

`sde.dat` is versioned apart from our SDE import: names, groups and skill
requirements come from the active SDE import, slots and stats from
`sde.dat`, and the two builds usually differ.

- `fitting-data.lock` pins the `@eveshipfit/sde` npm version and its
  integrity. The version is `<engine major>.<EVE build>.<patch>`.
- `make fitting-data` downloads and verifies it into `data/fitting/`
  (gitignored). The API image does the same in its own build stage and sets
  `FITTING_SDE_PATH`; CI caches it for the engine data tests.
- The crate compiles the lock in and refuses data of another major or
  build, so a stale local file says "run make fitting-data".

### Upgrading

In one PR:

1. Bump `esf-dogma-engine` / `esf-data` (pinned `=x.y.z`) in
   `crates/iskworks-fitting/Cargo.toml`.
2. Set `fitting-data.lock` to a matching `@eveshipfit/sde` version and its
   `dist.integrity` from `https://registry.npmjs.org/@eveshipfit/sde/<version>`.
   The data's major must be one the engine reads.
3. `make fitting-data`, then
   `cargo test -p iskworks-fitting -- --include-ignored`.

## Engine conventions the tests pin

- The ship's `damagePerSecondWithoutReload` already includes drones, so
  weapon DPS is that minus `droneDamagePerSecond`.
- `damageVolley` stays on each weapon; the fit's volley is the sum over
  weapons that reached active.
- `capacitorDepletesIn` is seconds, negative when the capacitor is stable;
  `capacitorStablePercentage` is then the level it settles at.
- Hardpoints: `turretSlotsLeft` / `launcherSlotsLeft` are the totals; used
  counts fitted modules with the `turretFitted` / `launcherFitted` effect,
  as the engine's own validation does.
