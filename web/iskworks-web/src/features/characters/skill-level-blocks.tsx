import type { CSSProperties } from "react";

import { skillLevelNumeral } from "./characters-formatters";
import type { LevelCellState } from "./skill-queue-summary";

// Five flat level tiles, drawn to read like EVE's in-client skill queue
// (solid tiles, not bordered controls) while using ISKWorks tokens.
// Semantics come from `skillLevelCells` and are independent of the row's
// own status -- the current-training row keeps its green accent elsewhere,
// these tiles still read trained / queued / training / partial / beyond.
//
//   trained  -> bright neutral fill (--color-foreground), reads "owned"
//   queued   -> saturated accent fill (--color-primary)
//   training -> the one level training right now: slow white <-> blue breathe
//   partial  -> crisp diagonal split: bright neutral | saturated accent
//   empty    -> a dark recessed slot (--color-border), no outline
//
// No borders on filled tiles, no rounded corners. `data-level-state` on
// each tile and a single summarised `aria-label` are the semantic surface
// -- colour is never the only signal. Tiles are not individually focusable.

const CELL_WORD: Record<LevelCellState, string> = {
  trained: "trained",
  queued: "queued",
  training: "currently training",
  partial: "partially trained",
  empty: "beyond target",
};

// Trained = the existing bright cool-neutral token (reads near-white on the
// dark panel). Queued = the primary accent lifted toward white so it stays
// an unmistakable saturated blue next to the bright trained tiles, matching
// EVE's cyan-blue queue blocks. `color-mix` on a token is the same idiom
// styles.css already uses for accent tints.
const TRAINED_FILL = "var(--color-foreground)";
const QUEUED_FILL = "color-mix(in srgb, var(--color-primary) 80%, #ffffff)";

// Hard "/" diagonal, bright-neutral top-left over saturated-accent
// bottom-right, matching EVE's partially-trained block. The doubled stops
// at 50% keep the edge crisp with no blended band.
const PARTIAL_STYLE: CSSProperties = {
  backgroundImage: `linear-gradient(135deg, ${TRAINED_FILL} 0 50%, ${QUEUED_FILL} 50% 100%)`,
};

const FILL_STYLE: Partial<Record<LevelCellState, CSSProperties>> = {
  trained: { background: TRAINED_FILL },
  queued: { background: QUEUED_FILL },
  partial: PARTIAL_STYLE,
  // A dark filled slot that barely lifts off the panel -- an unused level,
  // not a disabled button.
  empty: { background: "var(--color-border)" },
};

export function SkillLevelBlocks({ cells, target }: { cells: LevelCellState[]; target: number }) {
  const label =
    `Target level ${skillLevelNumeral(target)}: ` +
    cells.map((state, index) => `${skillLevelNumeral(index + 1)} ${CELL_WORD[state]}`).join(", ");

  return (
    <span aria-label={label} className="inline-flex shrink-0 items-center gap-[2px]" role="img">
      {cells.map((state, index) => (
        <span
          aria-hidden="true"
          className={
            state === "training" ? "block h-[10px] w-[10px] iw-skill-cell-training" : "block h-[10px] w-[10px]"
          }
          data-level={index + 1}
          data-level-state={state}
          key={index}
          style={FILL_STYLE[state]}
        />
      ))}
    </span>
  );
}
