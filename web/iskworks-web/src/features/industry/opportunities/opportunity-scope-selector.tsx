import type { ProfitabilityScopeDefinition, ProfitabilityScopeId } from "../../../api/opportunities";

// Groups scopes by their server-owned `family` (Ships / Rigs / Industry
// today, whatever the backend adds tomorrow) in the order families first
// appear in the scopes list -- entirely data-driven, no hardcoded category
// list on the frontend. Scales to many scopes without redesigning this
// control: a flat list would; a grouped dropdown doesn't.
function groupByFamily(scopes: ProfitabilityScopeDefinition[]): [string, ProfitabilityScopeDefinition[]][] {
  const order: string[] = [];
  const byFamily = new Map<string, ProfitabilityScopeDefinition[]>();
  for (const scope of scopes) {
    if (!byFamily.has(scope.family)) {
      byFamily.set(scope.family, []);
      order.push(scope.family);
    }
    byFamily.get(scope.family)?.push(scope);
  }
  return order.map((family) => [family, byFamily.get(family) ?? []]);
}

export function OpportunityScopeSelector({
  scopes,
  selectedScopeId,
  onSelect,
}: {
  scopes: ProfitabilityScopeDefinition[];
  selectedScopeId: ProfitabilityScopeId | null;
  onSelect: (id: ProfitabilityScopeId) => void;
}) {
  const groups = groupByFamily(scopes);
  return (
    <label className="mb-3 flex items-center gap-2" htmlFor="opportunity-scope-select">
      <span className="iw-eyebrow whitespace-nowrap">Category</span>
      <select
        className="iw-input w-auto min-w-[220px]"
        id="opportunity-scope-select"
        onChange={(event) => onSelect(event.target.value)}
        value={selectedScopeId ?? ""}
      >
        {groups.map(([family, familyScopes]) => (
          <optgroup key={family} label={family}>
            {familyScopes.map((scope) => (
              <option key={scope.id} value={scope.id}>
                {scope.label}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
    </label>
  );
}
