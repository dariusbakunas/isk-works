import type { CSSProperties } from "react";

import { EveTypeImage } from "../../components/eve-type-image";

// EVE's meta group ids (sde_meta_groups).
const TECH_I = 1;
const TECH_II = 2;
const STORYLINE = 3;
const FACTION = 4;
const OFFICER = 5;
const DEADSPACE = 6;
const TECH_III = 14;

const GREEN = "#5c9d68";
const PURPLE = "#8a68bd";

/** The client's corner tag on an item icon (spec §9.1); none without a meta group. */
function cornerTag(metaGroupId: number | null | undefined): { text: string; className: string; style?: CSSProperties } | null {
  switch (metaGroupId) {
    case TECH_I:
      return { text: "I", className: "bg-subtle" };
    case TECH_II:
      return { text: "II", className: "bg-warning" };
    case TECH_III:
      return { text: "III", className: "bg-warning" };
    case STORYLINE:
    case FACTION:
      return { text: "", className: "", style: { background: GREEN } };
    case DEADSPACE:
      return { text: "", className: "bg-primary" };
    case OFFICER:
      return { text: "", className: "", style: { background: PURPLE } };
    default:
      return null;
  }
}

/** An item icon with its meta group's corner tag. */
export function TypeIcon({
  typeId,
  name,
  metaGroupId,
  size,
  variation,
}: {
  typeId: number;
  name: string;
  metaGroupId?: number | null;
  size: number;
  variation?: "icon" | "render" | "bp" | "bpc";
}) {
  const tag = cornerTag(metaGroupId);
  return (
    <span className="relative inline-block shrink-0" style={{ width: size, height: size }}>
      <EveTypeImage size={size} typeId={typeId} typeName={name} variation={variation} />
      {tag ? (
        <b
          aria-hidden="true"
          className={`absolute left-0 top-0 min-w-[6px] px-[2px] font-mono leading-[9px] text-[#0b0d10] ${tag.className}`}
          style={{ fontSize: size >= 40 ? 7 : 6, minHeight: 6, ...tag.style }}
        >
          {tag.text}
        </b>
      ) : null}
    </span>
  );
}

/** The right-hand mark in a Variations / Used with row. */
export function MetaMark({ metaGroupId }: { metaGroupId: number }) {
  const [text, className, color] =
    metaGroupId === TECH_I
      ? ["I", "text-subtle", undefined]
      : metaGroupId === TECH_II
        ? ["II", "text-warning", undefined]
        : metaGroupId === TECH_III
          ? ["III", "text-warning", undefined]
          : metaGroupId === STORYLINE || metaGroupId === FACTION
            ? ["◆", "", GREEN]
            : metaGroupId === DEADSPACE
              ? ["◆", "text-primary", undefined]
              : metaGroupId === OFFICER
                ? ["★", "", PURPLE]
                : ["", "", undefined];
  if (!text) return null;
  return (
    <span aria-hidden="true" className={`ml-auto shrink-0 text-[7px] ${className}`} style={color ? { color } : undefined}>
      {text}
    </span>
  );
}
