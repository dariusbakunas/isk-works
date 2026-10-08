# Industry Facilities and Structure Modifiers

## Scope

Industry Facility Profiles are reusable, Workspace-owned descriptions of where
a job runs. They provide:

- manual, NPC-station, and Upwell profile identities;
- explicit material, duration, tax, surcharge, and system-index assumptions;
- SDE-identified structures and rigs with security-adjusted manufacturing modifiers;
- blueprint ME 0-10 and TE 0-20 planning inputs;
- deterministic effective material requirements and facility-adjusted duration;
- an explainable planned installation-cost estimate;
- optimistic profile revisions and archival.

Reaction facilities use the same model with the `eve-reaction-facility-v1`
formula version.

## Verified Rules and Sources

The material calculation follows CCP's Material Efficiency description:
blueprint material quantity is multiplied by the reduction factor and the job
result is rounded up. CCP also documents that whole single items are not reduced.
The implementation enforces a minimum of one input item per run.

Source:
<https://support.eveonline.com/hc/en-us/articles/203210542-Material-Efficiency-Research>

CCP's manufacturing documentation identifies blueprint ME/TE and facility
location as job inputs, but does not publish all current structure and fee
constants on that page:
<https://support.eveonline.com/hc/en-us/articles/203210292-Manufacturing>

ESI authentication and compatibility behavior follows the official developer
overview:
<https://developers.eveonline.com/docs/services/esi/overview/>

The active manufacturing formula version is `eve-manufacturing-facility-v1`.

### Materials

For each SDE recipe material:

```text
base extended = quantity per run * runs
ME factor = 1 - blueprint ME / 100
facility factor = product(1 - each material reduction / 100)
effective = ceil(base extended * ME factor * facility factor)
final = max(effective, runs)
```

Calculations use `rust_decimal`, checked integer arithmetic, and ceiling only at
the final documented job-material stage. React and SQL do not reproduce the
formula.

### Duration

```text
base extended seconds = SDE manufacturing seconds * runs
TE factor = 1 - blueprint TE / 100
facility factor = product(1 - each time reduction / 100)
planned seconds = ceil(base extended seconds * TE factor * facility factor)
```

Character skills are intentionally excluded and the API/UI label this policy.

### Planned Installation Cost

The current implementation supports a transparent adjusted-price estimate:

```text
EIV * system cost index * (1 - structure job-cost reduction / 100)
+ EIV * facility tax percentage
+ EIV * SCC surcharge percentage
+ EIV * alliance surcharge percentage
+ fixed supplemental planning cost
```

It is labeled planned, never exact. EIV is calculated from the SDE
recipe's base material quantities multiplied by runs and CCP adjusted prices
from public ESI. Blueprint ME and facility material reductions do not change
this basis. EIV is not a market sell price. If an adjusted price or the system
index is absent, the estimate is incomplete and zero is not substituted; a
Build can supply a manual EIV fallback.

The schema includes reusable ESI observation tables for adjusted prices and
manufacturing system cost indices. `apps/iskworks-worker` refreshes adjusted
prices and system cost indices from ESI, and values are persisted at their
source precision. The
active normalized SDE imports solar-system names and a targeted manufacturing
dogma projection from `typeDogma.jsonl`. Consequently:

- structure role material, time, and job-cost modifiers are derived from SDE
  attributes `2600`, `2602`, and `2601`;
- rig material/time modifiers are derived from attributes `2594` and `2593`,
  adjusted by security multipliers `2355`-`2357`;
- rig compatibility uses structure-group attributes `1298`-`1300` and the
  structure/rig size attribute `1547`;
- users select solar systems by name from the active SDE; the application stores
  the corresponding ID internally;
- structure IDs can be captured explicitly;
- manual cost indices have `Manual` provenance;
- automatic ESI adjusted-price EIV and manufacturing-index refreshes are
  available when ESI is configured.

Derived values are copied into the editable Facility Profile. A later SDE import
affects new selections.

## Persistence

Migration `202607250008_create_industry_facilities.sql` adds:

- `industry_facility_profiles`
- `industry_facility_profile_rigs`
- `industry_system_cost_index_observations`
- `industry_adjusted_price_observations`

Archiving a profile keeps it readable but prevents it from being selected or
used by the calculation service. Deleting a profile removes it and its rigs
permanently; any Build draft planning input that selected it is reset to no
facility.

## API and UI

Facility operations:

```text
GET  /api/industry/facilities
POST /api/industry/facilities
POST /api/industry/facilities/import/preview
POST /api/industry/facilities/import
GET  /api/industry/structures/search
GET  /api/industry/facilities/{facility_id}
PUT  /api/industry/facilities/{facility_id}
DELETE /api/industry/facilities/{facility_id}
POST /api/industry/facilities/{facility_id}/archive
GET  /api/industry/facilities/export
POST /api/industry/structures/resolve
GET  /api/industry/systems/{solar_system_id}/cost-index
POST /api/builds/{build_id}/facility-preview
PATCH /api/builds/{build_id}/facility
```

The structure selector searches names previously resolved through ESI asset,
blueprint, or market-location activity and stores the selected structure ID
internally. Known structure type and solar-system metadata are populated with
the selection.

A Build selects its facility through `PATCH /api/builds/{build_id}/facility`.
The Facilities navigation destination provides profile create, edit,
list, and permanent delete workflows, including Upwell identity, structure-role
material/time/job-cost reductions, and up to three category-applicable rigs.

Facility import is a preview-first workflow. EVE-backed facilities match by
role and EVE location ID; manual facilities match by role, solar system, and a
normalized name. Conflicts default to Skip and can be changed individually or
in bulk to Replace. Import execution rechecks the identity and revision before
writing so stale previews cannot silently overwrite a newer profile.

Structured errors include `facility_not_found`, `facility_archived`,
`facility_revision_conflict`, and `facility_calculation_failed`.
