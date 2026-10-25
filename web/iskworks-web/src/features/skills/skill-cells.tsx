import type { CatalogueSkill, SkillAttribute, SkillProgress, SkillState } from "../../api/skills";
import { SkillLevelBlocks } from "../characters/skill-level-blocks";
import { skillLevelCells } from "../characters/skill-queue-summary";
import { ATTRIBUTE_ABBREVIATIONS, stateLabel } from "./skills-catalogue";

const CHIP_CLASSES: Record<SkillState, string> = {
  max: "border-positive/50 text-positive",
  training: "border-primary/60 bg-primary/10 text-primary",
  queued: "border-primary/60 bg-primary/10 text-primary",
  canTrain: "border-border text-foreground/80",
  canInject: "border-warning/50 text-warning",
  locked: "border-border text-muted",
  omegaOnly: "border-[#8a6cc4]/50 text-[#a58ad8]",
};

export function SkillStateChip({ progress }: { progress: SkillProgress }) {
  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-[2px] border px-1.5 py-px text-[0.625rem] font-medium ${CHIP_CLASSES[progress.state]}`}
    >
      {stateLabel(progress)}
    </span>
  );
}

export function AttributeTags({ primary, secondary }: { primary: SkillAttribute; secondary: SkillAttribute }) {
  return (
    <span className="inline-flex gap-px" title={`Trains from ${primary} and ${secondary}`}>
      {[primary, secondary].map((attribute) => (
        <span
          className="rounded-[1px] border border-border px-1 py-px font-mono text-[0.5625rem] text-muted"
          key={attribute}
        >
          {ATTRIBUTE_ABBREVIATIONS[attribute]}
        </span>
      ))}
    </span>
  );
}

/** Level tiles for a catalogue row: trained, then the queued levels, with
 * the one in training animated and a partly trained next level split. */
export function CatalogueLevelBlocks({ skill }: { skill: CatalogueSkill }) {
  const progress = skill.progress;
  const trained = progress?.trainedLevel ?? 0;
  const levelStartSp = trained > 0 ? skill.spPerLevel[trained - 1] : 0;
  const partial = progress !== null && trained < 5 && progress.skillpoints > levelStartSp;
  const training = progress?.state === "training";
  const target = Math.max(trained, progress?.queuedToLevel ?? 0, partial || training ? trained + 1 : 0);
  return (
    <SkillLevelBlocks
      cells={skillLevelCells({
        targetLevel: target,
        currentTrainedLevel: trained,
        trainingLevel: training ? trained + 1 : null,
        hasPriorPartialSp: partial,
      })}
      target={Math.max(1, target)}
    />
  );
}
