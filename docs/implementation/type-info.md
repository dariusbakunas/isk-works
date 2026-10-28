# Type info: SDE data

The Type Info window (EVE's "Show Info") reads everything from the active SDE
import. This page covers the data. The API and the window come later.

## Sources

| SDE file | Table | Kept |
|---|---|---|
| `types.jsonl` | `sde_type_details` | English description (CCP markup kept), assembled volume, mass, capacity, portion size, variation parent. Only imported types with at least one of these get a row. The packaged volume stays in `sde_types.packaged_volume_m3`. |
| `dogmaUnits.jsonl` | `sde_dogma_units` | Every unit: name and display name. |
| `dogmaAttributes.jsonl` + `dogmaAttributeCategories.jsonl` | `sde_dogma_attributes` | Published attributes that have an English display name, with their unit, category name, `displayWhenZero` and `highIsGood`. |
| `typeDogma.jsonl` | `sde_type_attributes` | Imported types' values of the kept attributes. |
| `typeMaterials.jsonl` | `sde_type_materials` | Fixed reprocessing outputs per portion whose output type was imported. `randomizedMaterials` (min/max quantities) is skipped. |
| `typeBonus.jsonl` | `sde_type_bonuses` | Ship, subsystem and structure bonuses. `ordinal` runs over the per-skill bonuses (`source_kind = 'skill'`, `skill_type_id` set), then role, then misc. Text keeps CCP markup. |

All files except `types.jsonl` are optional. An export without them still
imports, with those tables empty.

## Notes

- Text keeps CCP's markup: `<a href=showinfo:TYPE_ID>`, `<br>`, `<b>`,
  `<font>`. The API turns it into text and link segments. The web app never
  renders it as HTML.
- Attribute values are stored raw. Unit formatting (resonance → resist %,
  modifier multipliers → signed %, group/type IDs → names) happens when the
  data is read, not at import time.
- Re-importing the same archive redoes the import when the active import
  has no `sde_dogma_attributes` rows (`SdeDatasetRequirements::type_info`).
  After deploying this, run `make sde-import` again.
- Size, SDE 3482594: 1,182 attributes, about 145k attribute values, 44k
  reprocessing lines, 3.9k bonus lines; about 43 MB per import, all tables
  included.
