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

/** An item icon. No meta group corner tag: CCP's meta groups mislabel some hulls (the Bane is "Tech II"). */
export function TypeIcon({
  typeId,
  name,
  size,
  variation,
}: {
  typeId: number;
  name: string;
  size: number;
  variation?: "icon" | "render" | "bp" | "bpc";
}) {
  return (
    <span className="relative inline-block shrink-0" style={{ width: size, height: size }}>
      <EveTypeImage size={size} typeId={typeId} typeName={name} variation={variation} />
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
