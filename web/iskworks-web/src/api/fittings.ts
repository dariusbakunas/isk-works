import type { MarketScope } from "./industry/market";
import { jsonRequest } from "./json-request";
import { ApiError } from "./workspace";

/** Mirrors `iskworks_core::fitting::FittingEngineInfo`. */
export interface FittingEngineInfo {
  /** The pinned engine data package version, e.g. `13.3650286.0`. */
  dataVersion: string;
  major: number;
  /** The EVE build the engine data was made from. */
  buildNumber: number;
}

/** Mirrors `iskworks_core::fitting::DamageProfilePreset`; only the ratios matter. */
export interface DamageProfilePreset {
  id: string;
  name: string;
  em: number;
  thermal: number;
  kinetic: number;
  explosive: number;
}

/** Mirrors `routes/fittings.rs` `FittingsMeta`. */
export interface FittingsMeta {
  /** Whether fit stats can be calculated (the engine data loaded). */
  available: boolean;
  engine: FittingEngineInfo | null;
  /** The active SDE import's build, versioned apart from the engine data. */
  sde: { buildNumber: string } | null;
  /** Uniform first. */
  damageProfiles: DamageProfilePreset[];
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Fittings request failed.",
});

export function getFittingsMeta(signal?: AbortSignal): Promise<FittingsMeta> {
  return request<FittingsMeta>("/api/fittings/meta", { signal });
}

/** Mirrors `iskworks_core::fitting::SlotKind`. */
export type SlotKind =
  | "high"
  | "med"
  | "low"
  | "rig"
  | "subsystem"
  | "service"
  | "fighter_tube"
  | "fighter_bay"
  | "drone_bay"
  | "cargo"
  | "implant"
  | "booster";

/** Mirrors `iskworks_core::fitting::ModuleState`; the requested state. */
export type ModuleState = "offline" | "online" | "active" | "overload";

/** Mirrors `iskworks_core::fitting::FitMutation`. */
export interface FitMutation {
  baseTypeId: number;
  /** Rolled value per attribute id. */
  attributes: Record<string, number>;
}

/** Mirrors `iskworks_core::fitting::FitItem`. */
export interface FitItem {
  typeId: number;
  slotKind: SlotKind;
  /** Position within a rack; `null` for bays and cargo. */
  slotIndex: number | null;
  quantity: number;
  state: ModuleState;
  chargeTypeId: number | null;
  mutation?: FitMutation;
}

/** Mirrors `iskworks_app::FitType`; names are `null` for a type the SDE import lacks. */
export interface FitType {
  typeId: number;
  name: string | null;
  groupName: string | null;
}

/** Mirrors `iskworks_app::FitItemView`. */
export interface FitItemView extends FitItem {
  name: string | null;
  groupName: string | null;
  chargeName: string | null;
  /** It loads charges; absent on items added in the editor until saved. */
  loadsCharges?: boolean;
}

/** Mirrors `iskworks_core::fitting::FlyabilityStatus`. */
export type FlyabilityStatus = "canUseAll" | "canFlyHull" | "cannot";

/** Mirrors `iskworks_app::FitListPilot`; `status` is `null` without synced skills. */
export interface FitListPilot {
  connectionId: string;
  name: string;
  status: FlyabilityStatus | null;
}

/** Mirrors `iskworks_app::FitSummary`. */
export interface FitSummary {
  id: string;
  name: string;
  ship: FitType;
  itemCount: number;
  updatedAt: string;
  pilots: FitListPilot[];
}

/** Mirrors `iskworks_app::FitDetail`. */
export interface FitDetail {
  id: string;
  name: string;
  description: string;
  ship: FitType;
  items: FitItemView[];
  createdAt: string;
  updatedAt: string;
}

/** Mirrors `iskworks_core::fitting::FitDraft`: a whole fit, items replaced. */
export interface FitDraft {
  name: string;
  description: string;
  shipTypeId: number;
  items: FitItem[];
}

/** Mirrors `iskworks_core::fitting::EftLine`; `line` counts from 1. */
export interface EftLine {
  line: number;
  text: string;
}

/** Mirrors `iskworks_app::FitTextPreview`. Nothing is saved. */
export interface FitTextPreview {
  fit: { name: string; ship: FitType; items: FitItemView[] } | null;
  unknownLines: EftLine[];
  error: string | null;
}

const jsonBody = (body: unknown): RequestInit => ({
  body: JSON.stringify(body),
  headers: { "Content-Type": "application/json" },
});

export function listFits(signal?: AbortSignal): Promise<FitSummary[]> {
  return request<FitSummary[]>("/api/fittings", { signal });
}

export function getFit(id: string, signal?: AbortSignal): Promise<FitDetail> {
  return request<FitDetail>(`/api/fittings/${encodeURIComponent(id)}`, { signal });
}

export function createFit(draft: FitDraft): Promise<FitDetail> {
  return request<FitDetail>("/api/fittings", { method: "POST", ...jsonBody(draft) });
}

export function updateFit(id: string, draft: FitDraft): Promise<FitDetail> {
  return request<FitDetail>(`/api/fittings/${encodeURIComponent(id)}`, { method: "PUT", ...jsonBody(draft) });
}

export function deleteFit(id: string): Promise<void> {
  return request<void>(`/api/fittings/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function previewFitText(text: string, signal?: AbortSignal): Promise<FitTextPreview> {
  return request<FitTextPreview>("/api/fittings/parse", { method: "POST", signal, ...jsonBody({ text }) });
}

/** `name` overrides the name in the text; without either the fit is `<Ship> fit`. */
export function importFitText(body: { text: string; name?: string }): Promise<FitDetail> {
  return request<FitDetail>("/api/fittings/import", { method: "POST", ...jsonBody(body) });
}

/** The fit as EFT, for pasting into the client. */
export async function exportFitText(id: string): Promise<string> {
  const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
  const response = await fetch(`${baseUrl}/api/fittings/${encodeURIComponent(id)}/export`, {
    credentials: "include",
    headers: { Accept: "text/plain" },
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new ApiError(response.status, payload?.error ?? { code: "api_error", message: "Fit export failed." });
  }
  return response.text();
}

/** `allV` (the default), `allZero`, or a character's connection id. */
export type PilotChoice = "allV" | "allZero" | string;

/** Mirrors `iskworks_core::fitting::Usage`. */
export interface Usage {
  used: number;
  total: number;
}

/** Mirrors `iskworks_core::fitting::CapacitorState`; `level` is a share (0.735 = 73.5%). */
export type CapacitorState = { kind: "stable"; level: number } | { kind: "depletes"; seconds: number };

/** Mirrors `iskworks_core::fitting::Resists`; fractions (0.75 = 75%). */
export interface Resists {
  em: number;
  thermal: number;
  kinetic: number;
  explosive: number;
}

export interface LayerStats {
  hp: number;
  ehp: number;
  resists: Resists;
}

/** Mirrors `iskworks_core::fitting::FitStats`. */
export interface FitStats {
  resources: {
    cpu: Usage;
    powergrid: Usage;
    calibration: Usage;
    turretHardpoints: Usage;
    launcherHardpoints: Usage;
    highSlots: Usage;
    medSlots: Usage;
    lowSlots: Usage;
    rigSlots: Usage;
    droneBandwidth: Usage;
    droneBay: Usage;
  };
  capacitor: { capacity: number; rechargeSeconds: number; peakDelta: number; state: CapacitorState };
  offense: { weaponDps: number; weaponDpsWithReload: number; volley: number; droneDps: number; totalDps: number };
  defense: {
    ehp: number;
    shield: LayerStats;
    armor: LayerStats;
    hull: LayerStats;
    shieldPassiveRegen: number;
    shieldBoost: number;
    armorRepair: number;
    hullRepair: number;
  };
  targeting: { maxTargets: number; lockRange: number; scanResolution: number; sensorStrength: number; signatureRadius: number };
  navigation: { maxVelocity: number; mass: number; inertia: number; alignSeconds: number; warpSpeed: number };
  drones: { active: number; maxActive: number; controlRange: number };
}

/** Mirrors `iskworks_core::fitting::ItemStats`. */
export interface ItemStats {
  reachedState: ModuleState;
  maxState: ModuleState;
  cpu: number;
  powergrid: number;
  calibration: number;
  capacitorPerSecond: number;
  optimal: number;
  falloff: number;
  dps: number;
  /** What it does, in one number; `null` shows as "—". */
  headline: Headline | null;
}

/** Mirrors `iskworks_core::fitting::HeadlineKind`. */
export type HeadlineKind =
  | "dps"
  | "shieldRepair"
  | "armorRepair"
  | "resist"
  | "velocity"
  | "shieldHp"
  | "armorHp"
  | "capacitor";

/** Mirrors `iskworks_core::fitting::Headline`. Repairs are HP/s, resist (the strongest bonus) and velocity percent, capacitor GJ. */
export interface Headline {
  kind: HeadlineKind;
  value: number;
}

/** Mirrors `iskworks_core::fitting::FitViolation`; `detail` is the engine's values. */
export interface FitViolation {
  target: { kind: "ship" } | { kind: "item"; index: number } | { kind: "charge"; index: number };
  rule: string;
  detail: Record<string, unknown>;
}

/** Mirrors `iskworks_core::fitting::MissingSkill`. */
export interface MissingSkill {
  skillTypeId: number;
  name: string;
  activeLevel: number;
  trainedLevel: number;
  requiredLevel: number;
  /** `null` when it needs Omega or the training rate is unknown. */
  seconds: number | null;
  omegaRequired: boolean;
}

/** Mirrors `iskworks_core::fitting::PilotFlyability`. */
export interface PilotFlyability {
  status: FlyabilityStatus;
  missing: MissingSkill[];
  remainingSeconds: number | null;
  missingSkillbooks: number[];
  omegaRequired: boolean;
  estimated: boolean;
  blockedItems: number[];
}

/** Mirrors `iskworks_app::FitStatsView`. */
export interface FitStatsView {
  stats: FitStats;
  /** Index-parallel to the fit's items; `null` for a type the engine data lacks. */
  items: (ItemStats | null)[];
  violations: FitViolation[];
  unknownItems: number[];
  /** Only for a character pilot. */
  flyability: PilotFlyability | null;
}

/** Mirrors `iskworks_app::FitPilot`. */
export interface FitPilot {
  connectionId: string;
  name: string;
  status: FlyabilityStatus | null;
  flyability: PilotFlyability | null;
  /** `null` without synced skills or without the fitting engine. */
  stats: { ehp: number; dps: number; capacitor: CapacitorState; maxVelocity: number; alignSeconds: number } | null;
}

/** Mirrors `iskworks_core::skill_plans::SkillPlan`. */
export interface CreatedSkillPlan {
  id: string;
  name: string;
  description: string;
  entries: { skillTypeId: number; level: number }[];
}

const query = (params: Record<string, string | undefined>) => {
  const defined = Object.entries(params).filter((entry): entry is [string, string] => entry[1] !== undefined);
  return defined.length ? `?${new URLSearchParams(defined)}` : "";
};

/** Calculates a fit that may not be saved (the editor's draft). */
export function calculateFit(
  body: { fit: { shipTypeId: number; items: FitItem[] }; pilot?: PilotChoice; profile?: string },
  signal?: AbortSignal,
): Promise<FitStatsView> {
  return request<FitStatsView>("/api/fittings/calculate", { method: "POST", signal, ...jsonBody(body) });
}

export function getFitStats(
  id: string,
  options: { pilot?: PilotChoice; profile?: string } = {},
  signal?: AbortSignal,
): Promise<FitStatsView> {
  return request<FitStatsView>(`/api/fittings/${encodeURIComponent(id)}/stats${query(options)}`, { signal });
}

export function getFitPilots(id: string, profile?: string, signal?: AbortSignal): Promise<FitPilot[]> {
  return request<FitPilot[]>(`/api/fittings/${encodeURIComponent(id)}/pilots${query({ profile })}`, { signal });
}

/** Without `connectionId` the plan holds everything the fit needs. */
export function createFitSkillPlan(id: string, connectionId?: string): Promise<CreatedSkillPlan> {
  return request<CreatedSkillPlan>(`/api/fittings/${encodeURIComponent(id)}/skill-plan`, {
    method: "POST",
    ...jsonBody(connectionId ? { connectionId } : {}),
  });
}

/** Mirrors `iskworks_core::fitting::SourceRef`; indexes are the fit's items. */
export type SourceRef =
  | { kind: "ship" }
  | { kind: "mode" }
  | { kind: "character" }
  | { kind: "item"; index: number }
  | { kind: "charge"; index: number }
  | { kind: "skill"; typeId: number }
  | { kind: "projected"; index: number }
  | { kind: "buff"; id: number };

/** Mirrors `iskworks_app::ExplainedSource`. */
export interface ExplainedSource {
  from: SourceRef;
  /** From the SDE import; `null` for a buff or a type it lacks. */
  name: string | null;
  /** The engine's operator: `pre_assign`, `pre_mul`, `mod_add`, `post_percent`, … */
  operator: string;
  value: number;
  /** The stacking penalty factor, when penalised. */
  penalty: number | null;
  applied: boolean;
}

/** Mirrors `iskworks_app::AttributeView`. */
export interface AttributeView {
  base: number;
  value: number;
  /** In the order applied. */
  sources: ExplainedSource[];
}

/** "Why?": every modifier on one attribute (a dogma name, e.g. `cpuOutput`) of the ship or an item. */
export function explainFitAttribute(
  id: string,
  options: { attribute: string; target?: "ship" | number; pilot?: PilotChoice; profile?: string },
  signal?: AbortSignal,
): Promise<AttributeView> {
  const params = query({
    target: String(options.target ?? "ship"),
    attribute: options.attribute,
    pilot: options.pilot,
    profile: options.profile,
  });
  return request<AttributeView>(`/api/fittings/${encodeURIComponent(id)}/explain${params}`, { signal });
}

/** Mirrors `iskworks_core::fitting::CatalogueKind`. */
export type CatalogueKind = "ship" | "module" | "drone" | "charge";

/** Mirrors `iskworks_app::CatalogueItem`. Fitting costs are before skills. */
export interface CatalogueItem {
  typeId: number;
  name: string;
  kind: CatalogueKind;
  slot: SlotKind | null;
  cpu: number;
  powergrid: number;
  calibration: number;
  hardpoint: "turret" | "launcher" | null;
  rigSize: number | null;
  chargeSize: number | null;
  /** Market groups from the root down. */
  marketPath: string[];
  /** Tech I, Tech II, Faction (4), …; `null` for most Tech I types. */
  metaGroupId: number | null;
  /** The type it is a variation of (a faction charge's Tech I one). */
  variationParent: number | null;
  /** For a character pilot, whether it has the skills; `null` for All V / All 0. */
  canUse: boolean | null;
}

/** Mirrors `iskworks_app::CatalogueView`. */
export interface CatalogueView {
  entries: CatalogueItem[];
  /** More matched than are listed (200); narrow the search. */
  truncated: boolean;
}

export type CatalogueRequest =
  | { kind: "modules"; shipTypeId: number; slot?: SlotKind }
  | { kind: "charges"; moduleTypeId: number }
  | { kind: "drones" }
  | { kind: "ships" };

/** The module browser: modules a ship fits, charges a module loads, drones or ships. */
export function searchFitCatalogue(
  search: CatalogueRequest,
  options: { q?: string; pilot?: PilotChoice } = {},
  signal?: AbortSignal,
): Promise<CatalogueView> {
  const params: Record<string, string | undefined> = { kind: search.kind };
  if (search.kind === "modules") {
    params.shipTypeId = String(search.shipTypeId);
    params.slot = search.slot;
  }
  if (search.kind === "charges") params.moduleTypeId = String(search.moduleTypeId);
  params.q = options.q?.trim() || undefined;
  params.pilot = options.pilot;
  return request<CatalogueView>(`/api/fittings/catalogue${query(params)}`, { signal });
}

/** A module's variations (Tech I, Tech II, meta, faction, …), itself included. */
export function getFitVariations(typeId: number, pilot?: PilotChoice, signal?: AbortSignal): Promise<CatalogueView> {
  return request<CatalogueView>(`/api/fittings/catalogue/variations${query({ typeId: String(typeId), pilot })}`, { signal });
}

export type CostSection = "hull" | "fittings" | "drones" | "cargo";

/** Mirrors `iskworks_app::FitCostLine`: one type of the fit, priced. */
export interface FitCostLine {
  typeId: number;
  section: CostSection;
  quantity: number;
  /** Lowest sell; `null` when the market has none. */
  unitPrice: string | null;
  total: string | null;
  name: string | null;
}

/** Mirrors `iskworks_app::FitCostView`: a saved fit at the workspace market. */
export interface FitCostView {
  scope: { regionId: number; locationId: number | null };
  lines: FitCostLine[];
  hull: string;
  fittings: string;
  drones: string;
  cargo: string;
  /** Of what has a price; `missing` lists what doesn't. */
  total: string;
  missing: number[];
  observedAt: string | null;
}

export interface FitShortfall {
  typeId: number;
  needed: number;
  have: number;
  name: string | null;
}

/** One character's holdings of the fit at one location. */
export interface FitLocationCoverage {
  connectionId: string;
  characterName: string;
  locationId: number;
  locationName: string;
  covered: number;
  needed: number;
  short: FitShortfall[];
}

/** Mirrors `iskworks_app::FitAssetsView`. */
export interface FitAssetsView {
  /** Units the fit is made of: hull, items and stacks. */
  needed: number;
  /** Best covered first, at most ten. */
  locations: FitLocationCoverage[];
}

export function getFitCost(id: string, signal?: AbortSignal): Promise<FitCostView> {
  return request<FitCostView>(`/api/fittings/${encodeURIComponent(id)}/cost`, { signal });
}

export function getFitAssets(id: string, signal?: AbortSignal): Promise<FitAssetsView> {
  return request<FitAssetsView>(`/api/fittings/${encodeURIComponent(id)}/assets`, { signal });
}

/** The cost's scope as the web's `MarketScope` (no location: region-wide). */
export function costScope(cost: FitCostView): MarketScope {
  return cost.scope.locationId === null
    ? { regionId: cost.scope.regionId }
    : { regionId: cost.scope.regionId, locationId: cost.scope.locationId };
}
