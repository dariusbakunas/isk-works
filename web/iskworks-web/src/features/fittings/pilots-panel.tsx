import type { FitDetail, FitPilot } from "../../api/fittings";

export function PilotsPanel(_: {
  fit: FitDetail;
  pilots: FitPilot[];
  selected: string;
  onSelect: (pilot: string) => void;
  onPlanCreated: () => void;
}) {
  return null;
}
