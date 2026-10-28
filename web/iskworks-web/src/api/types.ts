import type { PilotChoice } from "./fittings";
import type { MarketScope } from "./industry/market";
import { jsonRequest } from "./json-request";

/** Mirrors `routes/types.rs`: the Type Info window (EVE's "Show Info"). */

/** Mirrors `iskworks_core::type_info::RichSegment`: CCP markup as runs the
 * window renders itself, never as HTML. */
export type RichSegment =
  | { kind: "text"; text: string; bold: boolean }
  /** `showinfo:<typeId>`: opens that type in the same window. */
  | { kind: "link"; typeId: number; text: string }
  | { kind: "url"; href: string; text: string }
  | { kind: "break" };

export interface NamedId {
  id: number;
  name: string;
}

/** Mirrors `iskworks_core::type_info::ValueLink`. */
export type ValueLink = { kind: "type"; id: number; name: string } | { kind: "group"; id: number; name: string };

/** Mirrors `iskworks_core::type_info::AttributeRow`. */
export interface AttributeRow {
  attributeId: number;
  label: string;
  /** Already formatted: `-100 %`, `Small`, `1,079 HP`. */
  display: string;
  /** The stored value, as a decimal string. */
  raw: string;
  unitId: number | null;
  highIsGood: boolean;
  link: ValueLink | null;
}

export interface AttributeGroup {
  category: string;
  rows: AttributeRow[];
}

/** Mirrors `iskworks_core::type_info::TraitGroup`. */
export interface TraitGroup {
  /** "Caldari Frigate bonuses (per skill level)", "Role Bonus", "Misc Bonus". */
  heading: string;
  skillTypeId: number | null;
  /** `amount` is `4%`, `+5`; `null` for a bonus with no number. */
  bonuses: { amount: string | null; text: RichSegment[] }[];
}

/** Mirrors `iskworks_core::type_info::SkillNode`. */
export interface SkillNode {
  skillTypeId: number;
  /** `null` for a skill the SDE doesn't list. */
  name: string | null;
  level: number;
  trainedLevel: number;
  children: SkillNode[];
}

export interface RequirementSummary {
  missingSkills: number;
  /** `null` when the training rate is unknown. */
  remainingSeconds: number | null;
}

export type RecipeKind = "manufacturing" | "reaction";
export type UsedWithKind = "charges" | "modules";

/** Mirrors `iskworks_app::TypeInfoView`. Decimals are strings. */
export interface TypeInfo {
  typeId: number;
  name: string;
  published: boolean;
  group: NamedId | null;
  category: NamedId | null;
  metaGroup: NamedId | null;
  /** Root first. */
  marketGroupPath: string[];
  /** Lowest sell at the workspace's market; `null` without data. */
  price: { bestSell: string; scope: MarketScope; observedAt: string | null } | null;
  description: RichSegment[];
  basics: {
    volumeM3: string | null;
    packagedVolumeM3: string | null;
    massKg: string | null;
    capacityM3: string | null;
    portionSize: number | null;
  };
  attributes: AttributeGroup[];
  traits: TraitGroup[];
  requirements: SkillNode[];
  /** For a character pilot only. */
  missing: RequirementSummary | null;
  industry: {
    producedBy: { kind: RecipeKind; typeId: number; name: string | null }[];
    /** Per portion, before reprocessing yield. */
    reprocessesInto: { typeId: number; name: string; quantity: number }[];
    usedInCount: number;
  };
  /** Itself included. */
  variationCount: number;
  /** `null`: no Used with tab. */
  usedWith: { kind: UsedWithKind; count: number } | null;
}

/** Mirrors `iskworks_app::MetaGroupList`. */
export interface MetaGroupList {
  metaGroupId: number;
  metaGroupName: string;
  items: { typeId: number; name: string; metaLevel: number | null }[];
}

export interface UsedWithView {
  /** `null` when the type neither loads charges nor is one. */
  kind: UsedWithKind | null;
  groups: MetaGroupList[];
}

/** Mirrors `iskworks_app::UsedInPage`. */
export interface UsedInPage {
  items: {
    kind: RecipeKind;
    recipeTypeId: number;
    recipeName: string;
    productTypeId: number;
    productName: string;
    /** Of the type, per run. */
    quantity: number;
  }[];
  total: number;
  offset: number;
  limit: number;
  /** `null` on the last page. */
  nextOffset: number | null;
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "Item info request failed.",
});

function query(params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) search.set(key, String(value));
  }
  const text = search.toString();
  return text ? `?${text}` : "";
}

/** A type's info, its requirements checked against `pilot` (All V by default). */
export function getTypeInfo(typeId: number, pilot?: PilotChoice, signal?: AbortSignal): Promise<TypeInfo> {
  return request<TypeInfo>(`/api/types/${typeId}/info${query({ pilot })}`, { signal });
}

/** The type's variation family by meta group, itself included. */
export function getTypeVariations(typeId: number, signal?: AbortSignal): Promise<MetaGroupList[]> {
  return request<MetaGroupList[]>(`/api/types/${typeId}/variations`, { signal });
}

/** A module's charges, or a charge's modules. */
export function getTypeUsedWith(typeId: number, signal?: AbortSignal): Promise<UsedWithView> {
  return request<UsedWithView>(`/api/types/${typeId}/used-with`, { signal });
}

/** One page of the recipes that consume a type; the API caps `limit` at 100. */
export function getTypeUsedIn(
  typeId: number,
  options: { limit?: number; offset?: number } = {},
  signal?: AbortSignal,
): Promise<UsedInPage> {
  return request<UsedInPage>(`/api/types/${typeId}/used-in${query(options)}`, { signal });
}
