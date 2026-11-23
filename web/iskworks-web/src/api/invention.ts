import { json, request } from "./industry/request";
import type { MarketScope } from "./industry";

// Mirrors `routes/invention.rs`: inventable products and the decryptor
// comparison quote. Both are reads; nothing is saved.

export interface InventableProduct {
  productTypeId: number;
  productName: string;
  groupName: string | null;
  /** The invented blueprint, which manufactures the product. */
  blueprintTypeId: number;
  sourceBlueprintTypeId: number;
  sourceBlueprintName: string;
  /** Above 1 the source can invent several blueprints. */
  sourceOutputCount: number;
}

export interface InventionProductSearch {
  /** False when the active SDE import has no invention data. */
  inventionDataAvailable: boolean;
  products: InventableProduct[];
}

export interface InventionQuoteCommand {
  productTypeId: number;
  characterId?: string;
  /** Levels to use in place of the character's, by skill type id. */
  skillOverrides?: Record<string, number>;
  inventionFacilityId?: string;
  manufacturingFacilityId?: string;
  targetRuns?: number;
}

export type InventionSkillRole = "encryption" | "science";

export interface InventionSkill {
  skillTypeId: number;
  skillName: string;
  role: InventionSkillRole;
  requiredLevel: number;
  /** `null` when no character was chosen. */
  characterLevel: number | null;
  /** The level the quote used. */
  level: number;
  overridden: boolean;
}

/** An invention or copying job fee, line by line. `null` is unknown. */
export interface ScienceJobFee {
  estimatedItemValue: string | null;
  jobCostBase: string | null;
  systemCostIndex: string | null;
  systemIndexCost: string | null;
  facilityTax: string | null;
  sccSurcharge: string | null;
  allianceSurcharge: string | null;
  total: string | null;
}

export interface InventionMaterial {
  typeId: number;
  typeName: string;
  quantity: number;
  unitPrice: string | null;
}

export interface InventionDecryptor {
  typeId: number;
  name: string;
  probabilityMultiplier: string;
  meModifier: number;
  teModifier: number;
  runModifier: number;
  unitPrice: string | null;
}

export type InventionGap =
  | { kind: "materialPrice"; typeId: number }
  | { kind: "decryptorPrice"; typeId: number }
  | { kind: "jobFee" }
  | { kind: "copyCost" }
  | { kind: "manufacturingCost" };

export interface InventedCopy {
  materialEfficiency: number;
  timeEfficiency: number;
  runs: number;
}

export interface InventionOption {
  /** `null` for "no decryptor". */
  decryptor: InventionDecryptor | null;
  /** A fraction, to six decimals. */
  chance: string;
  copy: InventedCopy;
  attemptCost: string | null;
  costPerCopy: string | null;
  costPerRun: string | null;
  manufacturingCostPerRun: string | null;
  unitCost: string | null;
  /** Against "no decryptor". */
  unitCostDelta: string | null;
  unitCostDeltaPercent: string | null;
  copiesNeeded: number;
  attempts: { expected: number; confident: number } | null;
  /** Attempt cost times the confident attempts. */
  shoppingCost: string | null;
  best: boolean;
  complete: boolean;
  gaps: InventionGap[];
}

export interface InventionOutput {
  productTypeId: number | null;
  productName: string | null;
  blueprintTypeId: number;
  blueprintName: string;
}

export interface InventionQuote {
  productTypeId: number;
  productName: string;
  blueprintTypeId: number;
  sourceBlueprintTypeId: number;
  sourceBlueprintName: string;
  outputs: InventionOutput[];
  baseChance: string;
  baseRuns: number;
  skills: InventionSkill[];
  /** True when no character was chosen and every level is 4. */
  skillsDefaulted: boolean;
  skillFactor: string;
  chanceWithSkills: string;
  /** Skills below the job's level; empty when it can start. */
  missingSkills: string[];
  materials: InventionMaterial[];
  materialsCost: string | null;
  jobFee: ScienceJobFee;
  /** One run of the Tech I blueprint copy. */
  copyFee: ScienceJobFee;
  inventionDurationSeconds: number | null;
  targetRuns: number;
  outputPerRun: number;
  manufacturingInstallationIncluded: boolean;
  marketScope: MarketScope;
  pricesObservedAt: string | null;
  options: InventionOption[];
}

export interface InventionQuoteResponse {
  inventionDataAvailable: boolean;
  quote: InventionQuote | null;
}

export function searchInventableProducts(query: string, signal?: AbortSignal): Promise<InventionProductSearch> {
  return request(`/api/invention/products?query=${encodeURIComponent(query)}`, { signal });
}

export function quoteInvention(command: InventionQuoteCommand, signal?: AbortSignal): Promise<InventionQuoteResponse> {
  return request("/api/invention/quote", { ...json("POST", command), signal });
}
