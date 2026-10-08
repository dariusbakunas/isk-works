import { X } from "lucide-react";

import type { OpportunityEvidenceQuality } from "../../../api/opportunities";
import { DEFAULT_FILTERS, type OpportunityFilters } from "./opportunity-filtering";

const EVIDENCE_QUALITY_OPTIONS: { quality: OpportunityEvidenceQuality; label: string; toneClass: string }[] = [
  { quality: "strong", label: "Strong", toneClass: "text-positive" },
  { quality: "qualified", label: "Qualified", toneClass: "text-warning" },
  { quality: "weak", label: "Weak", toneClass: "text-danger" },
];

const DURATION_OPTIONS: { label: string; seconds: number | null }[] = [
  { label: "Any", seconds: null },
  { label: "30 minutes", seconds: 30 * 60 },
  { label: "1 hour", seconds: 60 * 60 },
  { label: "4 hours", seconds: 4 * 60 * 60 },
  { label: "12 hours", seconds: 12 * 60 * 60 },
  { label: "24 hours", seconds: 24 * 60 * 60 },
];

export function OpportunityFiltersPanel({
  filters,
  onChange,
  onClose,
}: {
  filters: OpportunityFilters;
  onChange: (next: OpportunityFilters) => void;
  onClose: () => void;
}) {
  function toggleEvidenceQuality(quality: OpportunityEvidenceQuality) {
    const next = new Set(filters.evidenceQuality);
    if (next.has(quality)) next.delete(quality);
    else next.add(quality);
    onChange({ ...filters, evidenceQuality: next });
  }

  return (
    <div className="w-56 shrink-0 border-r border-border pr-3">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-sm font-semibold">Filters</span>
        <button aria-label="Close filters" className="text-muted hover:text-foreground" onClick={onClose} type="button">
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="mb-3 border-b border-border pb-3">
        <p className="iw-eyebrow mb-1.5">Evidence quality</p>
        {EVIDENCE_QUALITY_OPTIONS.map(({ quality, label, toneClass }) => (
          <label className="flex items-center gap-2 py-0.5 text-sm" key={quality}>
            <input
              checked={filters.evidenceQuality.has(quality)}
              onChange={() => toggleEvidenceQuality(quality)}
              type="checkbox"
            />
            <span className={toneClass}>{label}</span>
          </label>
        ))}
        <label className="mt-1.5 flex items-center gap-2 border-t border-border pt-1.5 text-sm">
          <input
            checked={filters.hideWeak}
            onChange={(event) => onChange({ ...filters, hideWeak: event.target.checked })}
            type="checkbox"
          />
          <span className="text-warning">Hide Weak evidence</span>
        </label>
      </div>

      <div className="mb-3 border-b border-border pb-3">
        <label className="iw-eyebrow mb-1.5 block" htmlFor="min-gross-margin">
          Min. gross margin
        </label>
        <div className="flex items-center gap-2">
          <input
            className="flex-1"
            id="min-gross-margin"
            max={100}
            min={0}
            onChange={(event) => onChange({ ...filters, minGrossMarginPercent: Number(event.target.value) })}
            type="range"
            value={filters.minGrossMarginPercent}
          />
          <span className="w-10 text-right font-mono text-sm">{filters.minGrossMarginPercent}%</span>
        </div>
      </div>

      <div className="mb-3 border-b border-border pb-3">
        <label className="iw-eyebrow mb-1.5 block" htmlFor="max-capital-required">
          Max capital required
        </label>
        <div className="flex items-center gap-1.5">
          <input
            className="iw-input flex-1"
            id="max-capital-required"
            onChange={(event) =>
              onChange({ ...filters, maxCapitalRequired: event.target.value === "" ? null : Number(event.target.value) })
            }
            placeholder="Any"
            type="number"
            value={filters.maxCapitalRequired ?? ""}
          />
          <span className="text-xs text-muted">ISK</span>
        </div>
      </div>

      <div className="mb-3">
        <label className="iw-eyebrow mb-1.5 block" htmlFor="max-duration">
          Max duration
        </label>
        <select
          className="iw-input w-full"
          id="max-duration"
          onChange={(event) => {
            const option = DURATION_OPTIONS[Number(event.target.value)];
            onChange({ ...filters, maxDurationSeconds: option.seconds });
          }}
          value={DURATION_OPTIONS.findIndex((option) => option.seconds === filters.maxDurationSeconds)}
        >
          {DURATION_OPTIONS.map((option, index) => (
            <option key={option.label} value={index}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <button className="iw-button-secondary w-full" onClick={() => onChange(DEFAULT_FILTERS)} type="button">
        Reset
      </button>
    </div>
  );
}
