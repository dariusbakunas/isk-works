# Invention

Invention turns a run of a Tech I blueprint copy, some datacores and an
optional decryptor into a chance at a limited-run Tech II blueprint copy.
This page covers the SDE data, the rules in `iskworks_core::invention` and
the quote API, and the page built on them.

## SDE data

### Sources

| SDE file | Table | Kept |
|---|---|---|
| `blueprints.jsonl`, `activities.invention` | `sde_blueprint_invention` | One row per source blueprint, with the job time. |
| | `sde_blueprint_invention_materials` | What one attempt consumes (the datacores). |
| | `sde_blueprint_invention_products` | Each blueprint the source can yield: `base_runs` (the product `quantity`) and `probability` (the base chance). |
| | `sde_blueprint_invention_skills` | The skills the job lists, with their level. |
| `blueprints.jsonl`, `activities.copying` | `sde_blueprint_copying` | Copy time of every published blueprint that has one. |
| `typeDogma.jsonl` | `sde_decryptors` | The chance multiplier (attribute 1112) and the ME, TE and run modifiers (1113, 1114, 1124) of each decryptor. |

These tables sit beside `sde_blueprints`, which keeps meaning
"manufacturing". They reference `sde_types`, because a source blueprint is
kept whether or not its manufacturing recipe was.

### What is kept and what is not

- **Blueprints only.** A source must be a published type in the Blueprint
  category. Ancient relics (Tech III) carry an `invention` activity in the
  same shape and are left out.
- **A product needs a chance.** A product without a `probability` in
  `(0, 1]`, or whose blueprint type is unknown, is dropped. A source left
  with no product is dropped.
- **Products keep SDE order.** A source can yield more than one blueprint
  (Rifter Blueprint yields Jaguar and Wolf); the player picks one per job.
- **Only group 1304 (Generic Decryptor) counts.** Ten other published
  "Decryptors - …" groups carry the same four attributes with values the
  game does not use (multipliers up to 5.0), so the import filters on the
  group, not on the attributes.
- A missing decryptor attribute means no change: a multiplier of 1 or a
  modifier of 0.

### Reads

`SdeReadRepository` has four lookups:

- `has_invention_data()`: false for an import made before this data was
  read.
- `invention_recipe(product_blueprint_type_id)`: the recipe that yields a
  given invented blueprint, with every output of its source. A few outputs
  have two or three sources with identical recipes; the lowest source id is
  returned.
- `decryptors()`: by name.
- `search_inventable_products(query, limit)`: published items whose
  blueprint can be invented, matched on the item's name.

### Notes

- Re-importing the same archive redoes the import when the active import
  has no `sde_blueprint_invention` rows
  (`SdeDatasetRequirements::invention`). After deploying this, run
  `make sde-import` again.
- Size, SDE 3482594: 1,003 source blueprints, 1,056 products (931 Tech II,
  125 Structure Tech II), 2,006 material lines, 3,098 skill lines, 3,514
  copy times and 8 decryptors.

## Rules

`iskworks_core::invention` is pure. The formulas were checked against the
in-game industry window (Tristan Blueprint to Nemesis Blueprint, the
fixture in its tests) and against synced invention jobs.

```
chance        = base × (1 + encryption / 40 + Σ other skills / 30) × decryptor
copy          = ME 2 + modifier, TE 4 + modifier, runs base + modifier
job cost base = 0.02 × EIV of the output blueprint × runs
job fee       = base × (cost index + facility tax % + SCC % + alliance %)
attempt cost  = datacores + decryptor + job fee + one run of the T1 copy
cost per copy = attempt cost ÷ chance
cost per run  = cost per copy ÷ runs on the copy
unit cost     = (manufacturing cost per run + cost per run) ÷ output per run
```

- **Skills.** The encryption skill counts at 1/40 per level and every
  other skill the job lists at 1/30. That is usually two science skills.
  Capital module blueprints also list Capital Ship Construction, and it
  counts: synced jobs only match with it included.
- **Chance display.** The game truncates to one decimal (0.4375 reads
  43.7%). `chance_percent_truncated` does the same.
- **EIV** is `Σ base quantity × adjusted price` over the manufacturing
  materials of the blueprint being invented. For the copy fee it is that
  of the blueprint being copied.
- **Cost index.** Invention and copying use their own system cost index,
  not the manufacturing one (`latest_system_cost_index_for`).
- **Not modelled.** Structure role bonuses and rigs reduce the index part
  of the fee and the job time. Without them the fee reads a little high
  (31,976 ISK against 26,682 on the fixture) and the time longer.
- **Manufacturing cost per run** is a share of one job over all the
  copy's runs at the copy's ME, because material rounding depends on the
  run count. Inputs are priced at the best sell order in the workspace's
  market scope. It is a one-level estimate: sub-components are bought.
- **Attempts.** `expected = ceil(copies ÷ chance)`; the confident figure
  is the fewest attempts that yield the copies with 90% probability.
- **Missing data.** A missing price, cost index or EIV makes a figure
  `null` and the option incomplete. Nothing is replaced with zero, and an
  incomplete option is never marked best.

## API

- `GET /api/invention/products?query=`: items whose blueprint can be
  invented.
- `POST /api/invention/quote`: one option for "no decryptor" and one per
  decryptor. A read; `POST` only for its body.

```json
{ "productTypeId": 11377, "characterId": "…", "skillOverrides": { "23121": 5 },
  "inventionFacilityId": "…", "manufacturingFacilityId": "…", "targetRuns": 100 }
```

- Without a character every skill is taken as level 4.
- The invention facility is any facility profile. Only its solar system,
  facility tax and surcharges are used; its manual cost index and job
  cost reduction describe manufacturing. Without one the fee is unknown.
- Without a manufacturing facility the unit cost has materials only
  (`manufacturingInstallationIncluded: false`).
- Both responses carry `inventionDataAvailable`; false means the SDE must
  be imported again.
- The quote registers the facility's solar system for cost indices and
  the priced types for market coverage, so data missing on the first
  request is there on a later one.

## Page

**Invention** (`/invention`, `features/industry/invention/`) is a read-only
calculator: it has no save, record or "add to plan" action.

- Inputs live in the URL (`product`, `char`, `skills`, `inv`, `mfg`, `runs`,
  `option`), so a comparison can be bookmarked. A picker left alone takes a
  default: the character chosen last on this page (else the first), and
  the first manufacturing facility for both facilities. "None" is a choice
  of its own.
- Each skill the job lists has a level stepper. Changing one re-quotes with
  an override and marks the skill; Reset drops every override.
- The table has one row per option, sorted by unit cost with unpriced rows
  last. Chances are truncated to one decimal like the game's.
- Selecting a row opens its details: the attempt cost line by line (the job
  fee opens into its own lines), the outcome, and the attempts and shopping
  list for the target runs. "Copy shopping list" copies the market items as
  `name quantity` lines, which EVE's multibuy accepts.
