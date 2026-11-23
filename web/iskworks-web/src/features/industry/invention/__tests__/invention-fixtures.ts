import type { InventionDecryptor, InventionOption, InventionQuote, ScienceJobFee } from "../../../../api/invention";

export const PROCESS = 34206;
export const ACCELERANT = 34201;

export function decryptor(overrides: Partial<InventionDecryptor>): InventionDecryptor {
  return {
    typeId: PROCESS,
    name: "Process Decryptor",
    probabilityMultiplier: "1.1",
    meModifier: 3,
    teModifier: 6,
    runModifier: 0,
    unitPrice: "500000.0000",
    ...overrides,
  };
}

export function option(overrides: Partial<InventionOption>): InventionOption {
  return {
    decryptor: null,
    chance: "0.4375",
    copy: { materialEfficiency: 2, timeEfficiency: 4, runs: 10 },
    attemptCost: "429000.0000",
    costPerCopy: "980571.4286",
    costPerRun: "98057.1429",
    manufacturingCostPerRun: "604000.0000",
    unitCost: "702057.1429",
    unitCostDelta: "0",
    unitCostDeltaPercent: "0",
    copiesNeeded: 10,
    attempts: { expected: 23, confident: 29 },
    shoppingCost: "12441000.0000",
    best: true,
    complete: true,
    gaps: [],
    ...overrides,
  };
}

export const jobFee: ScienceJobFee = {
  estimatedItemValue: "9890000.0000",
  jobCostBase: "197800.0000",
  systemCostIndex: "0.041",
  systemIndexCost: "8109.8000",
  facilityTax: "1978.0000",
  sccSurcharge: "7912.0000",
  allianceSurcharge: "0.0000",
  total: "17999.8000",
};

export function quote(overrides: Partial<InventionQuote> = {}): InventionQuote {
  return {
    productTypeId: 2185,
    productName: "Hammerhead II",
    blueprintTypeId: 2186,
    sourceBlueprintTypeId: 2184,
    sourceBlueprintName: "Hammerhead I Blueprint",
    outputs: [{ productTypeId: 2185, productName: "Hammerhead II", blueprintTypeId: 2186, blueprintName: "Hammerhead II Blueprint" }],
    baseChance: "0.30",
    baseRuns: 10,
    skills: [
      { skillTypeId: 23121, skillName: "Gallente Encryption Methods", role: "encryption", requiredLevel: 1, characterLevel: 5, level: 5, overridden: false },
      { skillTypeId: 11453, skillName: "Electronic Engineering", role: "science", requiredLevel: 1, characterLevel: 5, level: 5, overridden: false },
      { skillTypeId: 11446, skillName: "Graviton Physics", role: "science", requiredLevel: 1, characterLevel: 5, level: 5, overridden: false },
    ],
    skillsDefaulted: false,
    skillFactor: "1.458333",
    chanceWithSkills: "0.4375",
    missingSkills: [],
    materials: [
      { typeId: 20418, typeName: "Datacore - Electronic Engineering", quantity: 2, unitPrice: "95000.0000" },
      { typeId: 20419, typeName: "Datacore - Graviton Physics", quantity: 2, unitPrice: "110000.0000" },
    ],
    materialsCost: "410000.0000",
    jobFee,
    copyFee: { ...jobFee, total: "1000.2000" },
    inventionDurationSeconds: 13920,
    targetRuns: 100,
    outputPerRun: 1,
    manufacturingInstallationIncluded: true,
    marketScope: { regionId: 10000002, locationId: 60003760 } as InventionQuote["marketScope"],
    pricesObservedAt: "2026-11-23T10:00:00Z",
    options: [
      option({}),
      option({
        decryptor: decryptor({ typeId: ACCELERANT, name: "Accelerant Decryptor", unitPrice: null, meModifier: 2, teModifier: 10, runModifier: 1, probabilityMultiplier: "1.2" }),
        chance: "0.525",
        copy: { materialEfficiency: 4, timeEfficiency: 14, runs: 11 },
        attemptCost: null,
        costPerCopy: null,
        costPerRun: null,
        unitCost: null,
        unitCostDelta: null,
        unitCostDeltaPercent: null,
        attempts: { expected: 20, confident: 25 },
        shoppingCost: null,
        best: false,
        complete: false,
        gaps: [{ kind: "decryptorPrice", typeId: ACCELERANT }],
      }),
      option({
        decryptor: decryptor({}),
        chance: "0.48125",
        copy: { materialEfficiency: 5, timeEfficiency: 10, runs: 10 },
        attemptCost: "929000.0000",
        costPerCopy: "1930389.6104",
        costPerRun: "193038.9610",
        manufacturingCostPerRun: "581000.0000",
        unitCost: "774038.9610",
        unitCostDelta: "71981.8181",
        unitCostDeltaPercent: "10.3",
        attempts: { expected: 21, confident: 27 },
        shoppingCost: "25083000.0000",
        best: false,
      }),
    ],
    ...overrides,
  };
}
