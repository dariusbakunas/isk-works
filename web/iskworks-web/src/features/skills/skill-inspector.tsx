import { Check, ChevronDown, ChevronRight, X } from "lucide-react";
import { useState } from "react";

import type { CatalogueSkill } from "../../api/skills";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { skillLevelNumeral } from "../characters/characters-formatters";
import { AttributeTags, CatalogueLevelBlocks, SkillStateChip } from "./skill-cells";
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
      </div>
    </PlannerInspectorShell>
  );
}
