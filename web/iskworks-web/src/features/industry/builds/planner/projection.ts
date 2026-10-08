import type { CreateBuildPlanPreview } from "../../../../api/industry";
import type { BuildPlannerPreviewView } from "./contracts";

export function projectCreatePreview(
  preview: CreateBuildPlanPreview,
): BuildPlannerPreviewView {
  return {
    mode: "create",
    candidateFingerprint: preview.candidateFingerprint,
    decision: preview.decision,
    candidate: preview.candidate,
    coverage: preview.coverage,
    warnings: preview.warnings,
    validation: preview.validation,
    completeness: preview.completeness,
    profitabilityBasis: preview.profitabilityBasis,
  };
}
