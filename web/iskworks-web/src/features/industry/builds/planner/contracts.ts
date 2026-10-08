import type {
  CreateBuildPlanPreview,
} from "../../../../api/industry";

export type BuildPlannerMode = "create" | "draft";

export interface BuildPlannerPreviewView {
  mode: BuildPlannerMode;
  candidateFingerprint: string;
  decision: CreateBuildPlanPreview["decision"];
  candidate: CreateBuildPlanPreview["candidate"];
  coverage: CreateBuildPlanPreview["coverage"];
  warnings: CreateBuildPlanPreview["warnings"];
  validation: CreateBuildPlanPreview["validation"];
  completeness: CreateBuildPlanPreview["completeness"];
  profitabilityBasis: CreateBuildPlanPreview["profitabilityBasis"];
}
