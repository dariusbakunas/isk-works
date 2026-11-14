import { Bookmark, Check } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";

import type { PlannerScenario, PlannerSolution } from "../../../../api/pi-planner";
import { savePlan, type SavedPlanScenario } from "../../../../api/pi-plans";
import { PlanNameDialog } from "../plans/plan-name-dialog";
import type { ScenarioCharacter } from "./planner-format";

/** The solve request as a snapshot: each character's name, and its EVE id when prefilled. */
export function savedScenario(scenario: PlannerScenario, characters: ScenarioCharacter[]): SavedPlanScenario {
  return {
    ...scenario,
    characterNames: characters.map((character) => character.name),
    eveCharacterIds: characters.map((character) => character.eveCharacterId ?? null),
  };
}

/** What the plan sells, by its top tier: trickles under 1 unit/h don't count. */
function mainProduct(solution: PlannerSolution): string | null {
  const sold = solution.plan.flows.filter((flow) => flow.to === null && Number(flow.unitsPerHour) >= 1);
  sold.sort((a, b) => b.tier - a.tier || Number(b.unitsPerHour) - Number(a.unitsPerHour));
  const products = new Set(sold.map((flow) => flow.name));
  if (sold.length === 0) return null;
  return products.size > 1 ? `${sold[0].name} + ${products.size - 1} more` : sold[0].name;
}

export function defaultPlanName(homeName: string | null, solution: PlannerSolution, now = new Date()): string {
  const date = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric" }).format(now);
  return [homeName, mainProduct(solution), date].filter(Boolean).join(" · ");
}

/** Saves the plan on screen as a snapshot to come back to while building it. */
export function SavePlanButton({
  characters,
  disabled,
  homeName,
  scenario,
  solution,
}: {
  characters: ScenarioCharacter[];
  disabled: boolean;
  homeName: string | null;
  scenario: PlannerScenario;
  solution: PlannerSolution;
}) {
  const [naming, setNaming] = useState(false);
  const [savedId, setSavedId] = useState<string | null>(null);
  const empty = solution.plan.colonies.length === 0;

  async function save(name: string) {
    const saved = await savePlan({ name, scenario: savedScenario(scenario, characters), solution });
    setSavedId(saved.id);
    setNaming(false);
  }

  return (
    <div className="flex items-center gap-2">
      {savedId ? (
        <Link className="flex items-center gap-1 text-xs text-success hover:underline" to={`/planetary/plans/${savedId}`}>
          <Check aria-hidden="true" className="h-3.5 w-3.5" />
          <span className="sr-only">Open saved plan</span>
          <span aria-hidden="true">Saved</span>
        </Link>
      ) : null}
      <button
        className="iw-button-secondary flex items-center gap-1.5"
        disabled={disabled || empty}
        onClick={() => setNaming(true)}
        title={empty ? "Nothing to save: the plan has no colonies." : "Keep this plan to come back to while you build it."}
        type="button"
      >
        <Bookmark aria-hidden="true" className="h-3.5 w-3.5" />
        Save plan
      </button>
      {naming ? (
        <PlanNameDialog
          initialName={defaultPlanName(homeName, solution)}
          onCancel={() => setNaming(false)}
          onSubmit={save}
          submitLabel="Save"
          title="Save plan"
        />
      ) : null}
    </div>
  );
}
