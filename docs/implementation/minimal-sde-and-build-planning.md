# SDE Import

## Scope

ISK Works imports CCP's Static Data Export (SDE) from a locally supplied JSONL
ZIP into PostgreSQL. The import is versioned: data is staged under a new import
ID and activated atomically, so readers always see one complete dataset.

## Architecture

```text
CCP JSONL ZIP
      |
      v
iskworks-sde source reader and parser
      |
      v
NormalizedSde records
      |
      v
iskworks-storage staged PostgreSQL importer
      |
      v
active SDE read repository (SdeReadRepository)
```

`iskworks-sde` owns source models, parsing, normalized records, import
orchestration, progress, source version/checksum metadata, SDE errors, and read
repository interfaces. It contains no SQL.

`iskworks-storage` owns SQLx writes and reads. HTTP handlers expose explicit
DTOs rather than raw source or database rows.

## Supported Input

The supported source is a locally supplied official CCP JSONL ZIP. These
archive-root files are required:

- `types.jsonl`
- `groups.jsonl`
- `blueprints.jsonl`

Other files are read when present, including `_sde.jsonl` (build metadata),
`categories.jsonl`, `metaGroups.jsonl`, `marketGroups.jsonl`, `typeDogma.jsonl`,
`industryModifierSources.jsonl`, `industryTargetFilters.jsonl`,
`planetSchematics.jsonl`, `mapPlanets.jsonl`, and the universe files listed
below.

If `_sde.jsonl` is absent, the importer uses a checksum-derived source version.
The source label stores only the filename, never the local absolute path.

The SDE is not downloaded automatically. Download the JSONL archive from CCP and
pass its local path to the command.

## Import Command

```bash
make db-up
make sde-import SDE_PATH=/path/to/eve-online-static-data-<build>-jsonl.zip
```

Equivalent direct command:

```bash
DATABASE_URL=postgres://iskworks:iskworks@127.0.0.1:5432/iskworks \
  cargo run -p iskworks-sde-import -- \
  --path /path/to/eve-online-static-data-<build>-jsonl.zip
```

The command runs pending migrations first. The API server does not need to be
running. `--force` re-imports and activates a fresh dataset even when the
archive's checksum matches the active import.

## Import Safety

Parsing and normalization finish before an import row is created. Database
writes target a new inactive import ID and use batched multi-row statements.
The previous active dataset remains untouched during staging.

Activation uses a short transaction that marks the old version superseded and
the staged version active. A failed write marks the staged import failed. Its
inactive rows may remain for diagnostics, but readers join only the active
version.

This avoids a single transaction covering the entire full-size import while
still making activation atomic.

## Repeatability and Edge Cases

- Same active checksum: idempotent no-op with existing counts, unless the
  archive contains a data category the active import lacks (for example
  facility dogma, NPC stations, reaction formulas, or planetary data), in which
  case an enriched replacement is staged.
- Newer or changed archive: stages a new import and activates only after all
  writes succeed.
- Interrupted import: inactive/importing data is never visible to readers.
- Missing required file: fails before database staging.
- Malformed JSONL: fails with filename and one-based line number.
- Duplicate group, type, or blueprint ID: fails normalization.
- Unresolved legacy type reference: skips only that recipe and counts it as
  skipped.
- Activity without products: skips the unusable recipe.
- Zero/non-positive CCP duration: retained as unknown (`NULL`) rather than an
  invalid duration.
- Removed types and changed recipes: naturally disappear or change when the new
  dataset becomes active.

### Universe Location Projection

The optional universe projection reads `mapRegions.jsonl`,
`mapConstellations.jsonl`, `mapSolarSystems.jsonl`, `mapMoons.jsonl`,
`npcCorporations.jsonl`, `stationOperations.jsonl`, and `npcStations.jsonl`. These records produce versioned region, constellation,
solar-system, and NPC station rows.

NPC station display names are reconstructed from CCP's system, orbit,
corporation, and operation data, matching the EVE naming convention without one
network request per station.

## Progress Model

The importer reports:

1. Validating source.
2. Reading types.
3. Reading blueprints.
4. Normalizing manufacturing recipes.
5. Writing types, with batch counts.
6. Writing blueprint recipes, with batch counts.
7. Activating SDE version.
8. Import complete.

`ProgressReporter` is UI-independent; the `iskworks-sde-import` binary prints
console lines.
