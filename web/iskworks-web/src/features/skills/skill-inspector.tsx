import { Check, ChevronDown, ChevronRight, Plus, X } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router";

import {
  getSkillPlan,
  listSkillPlans,
  updateSkillPlan,
  type CatalogueSkill,
  type SkillPlanSummary,
} from "../../api/skills";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { skillLevelNumeral } from "../characters/characters-formatters";
import { AttributeTags, CatalogueLevelBlocks, SkillStateChip } from "./skill-cells";
import { errorMessage } from "./skill-plans";
import { prerequisiteTree, type PrerequisiteNode, type RequiredForEntry } from "./skills-catalogue";

function TreeNodes({ nodes, onSelect }: { nodes: PrerequisiteNode[]; onSelect: (typeId: number) => void }) {
  return (
    <>
      {nodes.map((node) => (
        <li
          aria-expanded={node.children.length > 0 ? true : undefined}
          data-met={String(node.met)}
          key={`${node.skillTypeId}:${node.level}`}
          role="treeitem"
        >
          <button
            className="flex w-full items-center gap-1.5 py-0.5 text-left text-xs hover:text-primary"
            onClick={() => onSelect(node.skillTypeId)}
            type="button"
          >
            {node.met ? (
              <Check aria-label="met" className="h-3 w-3 shrink-0 text-positive" />
            ) : (
              <X aria-label="not met" className="h-3 w-3 shrink-0 text-danger" />
            )}
            <span className={node.met ? "text-muted" : "text-foreground"}>
              {node.name} {skillLevelNumeral(node.level)}
            </span>
          </button>
          {node.children.length > 0 ? (
            <ul className="ml-3 border-l border-border pl-2" role="group">
              <TreeNodes nodes={node.children} onSelect={onSelect} />
            </ul>
          ) : null}
        </li>
      ))}
    </>
  );
}

/** Appends one level of the skill to a chosen plan; the server inserts the
 * prerequisites it needs and the result says how many. */
function AddToPlan({ skill }: { skill: CatalogueSkill }) {
  const [params] = useSearchParams();
  const character = params.get("character");
  const [plans, setPlans] = useState<SkillPlanSummary[] | null>(null);
  const [planId, setPlanId] = useState("");
  const firstLevel = Math.min(5, (skill.progress?.trainedLevel ?? 0) + 1);
  const [level, setLevel] = useState(firstLevel);
  const [message, setMessage] = useState<{ tone: "positive" | "danger"; text: string } | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    listSkillPlans(null, controller.signal)
      .then((result) => {
        if (controller.signal.aborted) return;
        setPlans(result);
        setPlanId(result[0]?.id ?? "");
      })
      .catch(() => {
        if (!controller.signal.aborted) setPlans([]);
      });
    return () => controller.abort();
  }, []);

  async function add() {
    const plan = plans?.find((candidate) => candidate.id === planId);
    if (!plan) return;
    setSaving(true);
    setMessage(null);
    try {
      const detail = await getSkillPlan(plan.id);
      const entries = detail.entries.map(({ skillTypeId, level: entryLevel }) => ({ skillTypeId, level: entryLevel }));
      const result = await updateSkillPlan(plan.id, {
        name: detail.plan.name,
        description: detail.plan.description,
        entries: [...entries, { skillTypeId: skill.typeId, level }],
      });
      const count = result.inserted.length;
      const what = `${skill.name} ${skillLevelNumeral(level)}`;
      setMessage({
        tone: "positive",
        text: count > 0
          ? `Added ${what} and ${count} prerequisite${count === 1 ? "" : "s"} to ${plan.name}`
          : `Added ${what} to ${plan.name}`,
      });
    } catch (reason) {
      setMessage({ tone: "danger", text: errorMessage(reason, "Could not add to the plan.") });
    } finally {
      setSaving(false);
    }
  }

  if (plans === null) return <p className="text-xs text-muted">Loading plans…</p>;
  if (plans.length === 0) {
    return (
      <p className="text-xs text-muted">
        No skill plans yet.{" "}
        <Link className="text-primary underline" to={{ pathname: "/skills/plans", search: character ? `?${new URLSearchParams({ character })}` : "" }}>
          Create a plan
        </Link>
      </p>
    );
  }
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2">
        <label className="min-w-0 flex-1">
          <span className="sr-only">Plan</span>
          <select className="iw-input w-full" onChange={(event) => setPlanId(event.target.value)} value={planId}>
            {plans.map((plan) => <option key={plan.id} value={plan.id}>{plan.name}</option>)}
          </select>
        </label>
        <label>
          <span className="sr-only">Level</span>
          <select className="iw-input" onChange={(event) => setLevel(Number(event.target.value))} value={level}>
            {[1, 2, 3, 4, 5].filter((value) => value >= firstLevel).map((value) => (
              <option key={value} value={value}>{skillLevelNumeral(value)}</option>
            ))}
          </select>
        </label>
        <button className="iw-button-primary" disabled={saving || planId === ""} onClick={() => void add()} type="button">Add</button>
      </div>
      {message ? (
        <p className={`text-xs ${message.tone === "positive" ? "text-positive" : "text-danger"}`} role="status">{message.text}</p>
      ) : null}
    </div>
  );
}

export function SkillInspector({
  onClose,
  onSelect,
  requiredFor,
  skill,
  skills,
}: {
  onClose: () => void;
  onSelect: (typeId: number) => void;
  requiredFor: RequiredForEntry[];
  skill: CatalogueSkill;
  skills: CatalogueSkill[];
}) {
  const [showRequiredFor, setShowRequiredFor] = useState(false);
  const [adding, setAdding] = useState(false);
  const tree = prerequisiteTree(skill, skills);
  const trained = skill.progress?.trainedLevel ?? 0;

  return (
    <PlannerInspectorShell
      closeLabel="Close skill inspector"
      dismissLabel="Dismiss skill inspector"
      eyebrow={`${skill.groupName} · Rank ×${skill.rank}`}
      onClose={onClose}
      open
      returnFocusRowKey={String(skill.typeId)}
      title={skill.name}
      width="wide"
    >
      <div className="space-y-4 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <CatalogueLevelBlocks skill={skill} />
          <AttributeTags primary={skill.primaryAttribute} secondary={skill.secondaryAttribute} />
          {skill.progress ? <SkillStateChip progress={skill.progress} /> : null}
          {skill.omegaOnly ? <span className="text-[0.625rem] text-muted">Omega clones only</span> : null}
        </div>

        <section>
          <h3 className="iw-eyebrow mb-1">SP per level</h3>
          <ol className="divide-y divide-border border border-border">
            {skill.spPerLevel.map((sp, index) => {
              const level = index + 1;
              const done = level <= trained;
              const next = level === trained + 1 && skill.progress !== null;
              return (
                <li
                  className={`flex items-center justify-between px-2 py-1 text-xs ${next ? "bg-primary/10" : ""}`}
                  key={level}
                >
                  <span className="flex items-center gap-1.5">
                    {done ? <Check aria-label="trained" className="h-3 w-3 text-positive" /> : <span className="w-3" />}
                    Level {skillLevelNumeral(level)}
                  </span>
                  <span className="font-mono">{sp.toLocaleString("en-US")} SP</span>
                </li>
              );
            })}
          </ol>
        </section>

        <section>
          <h3 className="iw-eyebrow mb-1">Prerequisites</h3>
          {tree.length === 0 ? (
            <p className="text-xs text-muted">None.</p>
          ) : (
            <ul aria-label="Prerequisites" role="tree">
              <TreeNodes nodes={tree} onSelect={onSelect} />
            </ul>
          )}
        </section>

        {requiredFor.length > 0 ? (
          <section>
            <button
              aria-expanded={showRequiredFor}
              className="flex items-center gap-1 text-xs font-semibold text-muted hover:text-foreground"
              onClick={() => setShowRequiredFor((shown) => !shown)}
              type="button"
            >
              {showRequiredFor ? (
                <ChevronDown aria-hidden="true" className="h-3 w-3" />
              ) : (
                <ChevronRight aria-hidden="true" className="h-3 w-3" />
              )}
              Required for ({requiredFor.length})
            </button>
            {showRequiredFor ? (
              <ul className="mt-1 space-y-0.5">
                {requiredFor.map((entry) => (
                  <li key={entry.skillTypeId}>
                    <button
                      className="text-left text-xs hover:text-primary"
                      onClick={() => onSelect(entry.skillTypeId)}
                      type="button"
                    >
                      {entry.name} <span className="text-muted">needs {skillLevelNumeral(entry.level)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
          </section>
        ) : null}

        {(skill.progress?.trainedLevel ?? 0) < 5 ? (
          <section className="border-t border-border pt-3">
            {adding ? (
              <AddToPlan skill={skill} />
            ) : (
              <button className="iw-button-primary flex w-full items-center justify-center gap-1" onClick={() => setAdding(true)} type="button">
                <Plus aria-hidden="true" className="h-3.5 w-3.5" />
                Add to plan…
              </button>
            )}
          </section>
        ) : null}
      </div>
    </PlannerInspectorShell>
  );
}
