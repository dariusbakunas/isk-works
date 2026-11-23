# Invention: SDE data

Invention turns a run of a Tech I blueprint copy, some datacores and an
optional decryptor into a chance at a limited-run Tech II blueprint copy.
This page covers the data the SDE import keeps for it. The calculator, API
and page come later.

## Sources

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

## What is kept and what is not

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

## Reads

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

## Notes

- Re-importing the same archive redoes the import when the active import
  has no `sde_blueprint_invention` rows
  (`SdeDatasetRequirements::invention`). After deploying this, run
  `make sde-import` again.
- Size, SDE 3482594: 1,003 source blueprints, 1,056 products (931 Tech II,
  125 Structure Tech II), 2,006 material lines, 3,098 skill lines, 3,514
  copy times and 8 decryptors.
