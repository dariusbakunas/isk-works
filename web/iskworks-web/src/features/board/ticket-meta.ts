import type { AcquisitionRunStatus } from "../../api/industry";
import type { Tone } from "../../components/primitives";

export const acquisitionRunStatusMeta: Record<AcquisitionRunStatus, { label: string; tone: Tone }> = {
  ready: { label: "Ready", tone: "positive" },
  inProgress: { label: "In Progress", tone: "primary" },
  complete: { label: "Complete", tone: "muted" },
};
