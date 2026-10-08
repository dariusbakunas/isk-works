import type { Build } from "../../../../api/industry";
import { EveTypeImage } from "../../../../components/eve-type-image";

/**
 * The primary card visual: the blueprint *actually selected* on the Build.
 *
 * - Manufacturing Build -> the blueprint's EVE art. `bpc` (Blueprint Copy)
 *   art when the selected blueprint's origin is a copy, `bp` (the generic
 *   blueprint art) for an original or when no concrete blueprint is
 *   selected yet. BPC is the state worth showing explicitly (finite runs);
 *   an unselected manufacturing Build does not need a third neutral visual.
 * - Reaction Build -> reactions have no blueprint, but the reaction formula
 *   is itself a blueprint-style item: the EVE image server only serves it
 *   under `bp`/`bpc` (its `icon` variation 404s), so render its `bp` art.
 *
 * BPO/BPC is never inferred from the output item -- only from
 * `build.selectedBlueprintOrigin`, which the API resolves from the Build's
 * blueprint selection.
 */
export function BlueprintThumbnail({ build, size = 48 }: { build: Build; size?: 48 | 64 | 128 }) {
  const { recipe } = build;
  if (recipe.kind === "manufacturing") {
    return (
      <EveTypeImage
        size={size}
        typeId={recipe.blueprintTypeId}
        typeName={recipe.blueprintName}
        variation={build.selectedBlueprintOrigin === "copy" ? "bpc" : "bp"}
      />
    );
  }
  return (
    <EveTypeImage
      size={size}
      typeId={recipe.reactionFormulaTypeId}
      typeName={recipe.reactionFormulaName}
      variation="bp"
    />
  );
}
