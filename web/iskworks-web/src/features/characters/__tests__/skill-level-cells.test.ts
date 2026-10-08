import { describe, expect, test } from "vitest";

import { skillLevelCells, type LevelCellState } from "../skill-queue-summary";

function cells(params: {
  targetLevel: number;
  currentTrainedLevel: number | null;
  /** Sugar: this row IS the active one -> trainingLevel = its own target. */
  isActive?: boolean;
  /** The level of this row's skill training right now (any row of it). */
  trainingLevel?: number | null;
  hasPriorPartialSp?: boolean;
}): LevelCellState[] {
  return skillLevelCells({
    targetLevel: params.targetLevel,
    currentTrainedLevel: params.currentTrainedLevel,
    trainingLevel: params.isActive ? params.targetLevel : params.trainingLevel ?? null,
    hasPriorPartialSp: params.hasPriorPartialSp ?? false,
  });
}

describe("skillLevelCells", () => {
  test("A — untrained skill queued to level I", () => {
    expect(cells({ targetLevel: 1, currentTrainedLevel: 0 })).toEqual([
      "queued",
      "empty",
      "empty",
      "empty",
      "empty",
    ]);
  });

  test("B — untrained skill queued to level III", () => {
    expect(cells({ targetLevel: 3, currentTrainedLevel: 0 })).toEqual([
      "queued",
      "queued",
      "queued",
      "empty",
      "empty",
    ]);
  });

  test("C — skill trained to II, level III queued", () => {
    expect(cells({ targetLevel: 3, currentTrainedLevel: 2 })).toEqual([
      "trained",
      "trained",
      "queued",
      "empty",
      "empty",
    ]);
  });

  test("D — trained to IV, level V queued, no partial SP", () => {
    expect(cells({ targetLevel: 5, currentTrainedLevel: 4 })).toEqual([
      "trained",
      "trained",
      "trained",
      "trained",
      "queued",
    ]);
  });

  test("E — trained to IV, partially trained toward V", () => {
    expect(cells({ targetLevel: 5, currentTrainedLevel: 4, hasPriorPartialSp: true })).toEqual([
      "trained",
      "trained",
      "trained",
      "trained",
      "partial",
    ]);
  });

  test("F — currently training level III, trained to II -> frontier is 'training'", () => {
    expect(cells({ targetLevel: 3, currentTrainedLevel: 2, isActive: true })).toEqual([
      "trained",
      "trained",
      "training",
      "empty",
      "empty",
    ]);
  });

  test("active row wins over pre-accrued partial SP: frontier is 'training', not 'partial'", () => {
    expect(
      cells({ targetLevel: 5, currentTrainedLevel: 4, isActive: true, hasPriorPartialSp: true }),
    ).toEqual(["trained", "trained", "trained", "trained", "training"]);
  });

  test("a later queued row of the currently-training skill animates the in-training level", () => {
    // Supply Chain Management III is training now; the queued "…IV" row must
    // still show level III as 'training', not a plain queued cell.
    expect(cells({ targetLevel: 4, currentTrainedLevel: 2, trainingLevel: 3 })).toEqual([
      "trained",
      "trained",
      "training",
      "queued",
      "empty",
    ]);
    // and an even later "…V" row of the same skill
    expect(cells({ targetLevel: 5, currentTrainedLevel: 2, trainingLevel: 3 })).toEqual([
      "trained",
      "trained",
      "training",
      "queued",
      "queued",
    ]);
  });

  test("a different skill's rows are unaffected by what is training", () => {
    // trainingLevel is null for any row whose skill is not the active one
    expect(cells({ targetLevel: 3, currentTrainedLevel: 0, trainingLevel: null })).toEqual([
      "queued",
      "queued",
      "queued",
      "empty",
      "empty",
    ]);
  });

  test("G — successive queue entries I→V with current trained level 0 do NOT simulate prior entries", () => {
    const rows = [1, 2, 3, 4, 5].map((target) => cells({ targetLevel: target, currentTrainedLevel: 0 }));
    expect(rows).toEqual([
      ["queued", "empty", "empty", "empty", "empty"],
      ["queued", "queued", "empty", "empty", "empty"],
      ["queued", "queued", "queued", "empty", "empty"],
      ["queued", "queued", "queued", "queued", "empty"],
      ["queued", "queued", "queued", "queued", "queued"],
    ]);
    // no row contains a "trained" cell — the character has trained nothing
    expect(rows.flat()).not.toContain("trained");
  });

  test("null currentTrainedLevel (skills array absent) is treated as 0, not a crash", () => {
    expect(cells({ targetLevel: 3, currentTrainedLevel: null })).toEqual([
      "queued",
      "queued",
      "queued",
      "empty",
      "empty",
    ]);
  });

  test("a level already at/above the target (stale entry) still reads trained", () => {
    // char already has V; entry still queued at finished_level 5
    expect(cells({ targetLevel: 5, currentTrainedLevel: 5 })).toEqual([
      "trained",
      "trained",
      "trained",
      "trained",
      "trained",
    ]);
  });

  test("partial only ever lands on the frontier level, never a deeper queued level", () => {
    // trained II, entry targets V: III and IV are plain queued, V is queued
    // (not the frontier — you can't be part-way into V without IV)
    expect(cells({ targetLevel: 5, currentTrainedLevel: 2, hasPriorPartialSp: true })).toEqual([
      "trained",
      "trained",
      "queued",
      "queued",
      "queued",
    ]);
  });
});
