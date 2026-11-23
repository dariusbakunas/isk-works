import type { InventionQuoteCommand } from "../../../api/invention";

/** A picker left at its default, or explicitly set to nothing. */
export type Choice = string | "none" | null;

/** Everything the page's inputs hold; lives in the URL so a comparison can
 * be bookmarked. */
export interface InventionView {
  productTypeId: number | null;
  /** `null` picks the first character; `"none"` uses level 4 throughout. */
  characterId: Choice;
  /** Levels used in place of the character's, by skill type id. */
  skillOverrides: Record<number, number>;
  /** `null` picks the first facility; `"none"` leaves the fee unknown. */
  inventionFacilityId: Choice;
  manufacturingFacilityId: Choice;
  targetRuns: number;
  /** The open row: a decryptor type id, or `"none"`. */
  selected: string | null;
}

export const DEFAULT_TARGET_RUNS = 100;
export const MAX_TARGET_RUNS = 1_000_000;

export const DEFAULT_VIEW: InventionView = {
  productTypeId: null,
  characterId: null,
  skillOverrides: {},
  inventionFacilityId: null,
  manufacturingFacilityId: null,
  targetRuns: DEFAULT_TARGET_RUNS,
  selected: null,
};

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function positiveInteger(value: string | null): number | null {
  if (value === null || !/^\d+$/.test(value)) return null;
  const number = Number(value);
  return Number.isSafeInteger(number) && number > 0 ? number : null;
}

function choice(value: string | null): Choice {
  if (value === "none") return "none";
  return value !== null && UUID_PATTERN.test(value) ? value : null;
}

function overrides(value: string | null): Record<number, number> {
  const result: Record<number, number> = {};
  for (const entry of (value ?? "").split(",")) {
    const match = /^(\d+):([0-5])$/.exec(entry);
    const skillTypeId = match ? positiveInteger(match[1]) : null;
    if (match && skillTypeId !== null) result[skillTypeId] = Number(match[2]);
  }
  return result;
}

export function parseInventionView(params: URLSearchParams): InventionView {
  const runs = positiveInteger(params.get("runs"));
  const selected = params.get("option");
  return {
    productTypeId: positiveInteger(params.get("product")),
    characterId: choice(params.get("char")),
    skillOverrides: overrides(params.get("skills")),
    inventionFacilityId: choice(params.get("inv")),
    manufacturingFacilityId: choice(params.get("mfg")),
    targetRuns: runs !== null && runs <= MAX_TARGET_RUNS ? runs : DEFAULT_TARGET_RUNS,
    selected: selected === "none" || positiveInteger(selected) !== null ? selected : null,
  };
}

/** The view as query parameters, with defaults left out. */
export function inventionViewParams(view: InventionView): URLSearchParams {
  const params = new URLSearchParams();
  if (view.productTypeId !== null) params.set("product", String(view.productTypeId));
  if (view.characterId !== null) params.set("char", view.characterId);
  const skills = Object.entries(view.skillOverrides)
    .sort(([left], [right]) => Number(left) - Number(right))
    .map(([skillTypeId, level]) => `${skillTypeId}:${level}`);
  if (skills.length > 0) params.set("skills", skills.join(","));
  if (view.inventionFacilityId !== null) params.set("inv", view.inventionFacilityId);
  if (view.manufacturingFacilityId !== null) params.set("mfg", view.manufacturingFacilityId);
  if (view.targetRuns !== DEFAULT_TARGET_RUNS) params.set("runs", String(view.targetRuns));
  if (view.selected !== null) params.set("option", view.selected);
  return params;
}

/** What the pickers resolve to once the page knows what exists. */
export interface ResolvedChoices {
  characterId: string | null;
  inventionFacilityId: string | null;
  manufacturingFacilityId: string | null;
}

/** A default (`null`) becomes the first candidate; `"none"` stays nothing. */
export function resolveChoice(value: Choice, candidates: string[]): string | null {
  if (value === "none") return null;
  if (value !== null && candidates.includes(value)) return value;
  return candidates[0] ?? null;
}

/** The quote request for a view, or `null` while no product is chosen. */
export function inventionQuoteCommand(view: InventionView, choices: ResolvedChoices): InventionQuoteCommand | null {
  if (view.productTypeId === null) return null;
  const command: InventionQuoteCommand = { productTypeId: view.productTypeId, targetRuns: view.targetRuns };
  if (choices.characterId) command.characterId = choices.characterId;
  if (choices.inventionFacilityId) command.inventionFacilityId = choices.inventionFacilityId;
  if (choices.manufacturingFacilityId) command.manufacturingFacilityId = choices.manufacturingFacilityId;
  if (Object.keys(view.skillOverrides).length > 0) {
    command.skillOverrides = Object.fromEntries(
      Object.entries(view.skillOverrides).map(([skillTypeId, level]) => [String(skillTypeId), level]),
    );
  }
  return command;
}
