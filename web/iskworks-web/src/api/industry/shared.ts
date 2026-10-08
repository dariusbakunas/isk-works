export type Money = string;
export type RecipeCurrency =
  | "current"
  | "olderSdeVersion"
  | "recipeChanged"
  | "blueprintNoLongerAvailable"
  | "reactionFormulaNoLongerAvailable"
  | "unableToCompare";

export interface RecipeLine {
  typeId: number;
  typeName: string;
  quantityPerRun: number;
  sortOrder: number;
}

export interface CapturedRecipe {
  kind: "manufacturing";
  sourceSdeDatasetId: string;
  sourceSdeVersion: string;
  blueprintTypeId: number;
  blueprintName: string;
  durationSecondsPerRun: number | null;
  materials: RecipeLine[];
  products: RecipeLine[];
  fingerprint: string;
}

export interface CapturedReactionFormula {
  kind: "reaction";
  sourceSdeDatasetId: string;
  sourceSdeVersion: string;
  reactionFormulaTypeId: number;
  reactionFormulaName: string;
  durationSecondsPerRun: number | null;
  materials: RecipeLine[];
  products: RecipeLine[];
  fingerprint: string;
}

export type BuildRecipe = CapturedRecipe | CapturedReactionFormula;
