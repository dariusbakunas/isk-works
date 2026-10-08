import type { OpportunityEvidenceQuality } from "../../../api/opportunities";
import { Badge, type Tone } from "../../../components/primitives";

const EVIDENCE_QUALITY_TONE: Record<OpportunityEvidenceQuality, Tone> = {
  strong: "positive",
  qualified: "warning",
  weak: "danger",
};

const EVIDENCE_QUALITY_LABEL: Record<OpportunityEvidenceQuality, string> = {
  strong: "Strong",
  qualified: "Qualified",
  weak: "Weak",
};

export function EvidenceQualityBadge({ quality }: { quality: OpportunityEvidenceQuality }) {
  return (
    <Badge square tone={EVIDENCE_QUALITY_TONE[quality]}>
      {EVIDENCE_QUALITY_LABEL[quality].toUpperCase()}
    </Badge>
  );
}
