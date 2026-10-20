import { textToneClasses, type Tone } from "../../../components/primitives";

/** One headline figure: label, value (tone-coloured) and a muted detail line. */
export function InventoryKpiCard({
  label,
  value,
  title,
  tone,
  detail,
}: {
  label: string;
  value: string;
  title?: string;
  tone: Tone;
  detail: string;
}) {
  return (
    <div className="iw-panel min-w-0 px-3 py-2" data-testid="inventory-kpi">
      <div className="text-[0.625rem] font-semibold uppercase tracking-wider text-muted">{label}</div>
      <div className={`mt-0.5 whitespace-nowrap font-mono text-lg font-bold tabular-nums ${textToneClasses[tone]}`} title={title}>
        {value}
      </div>
      <div className="mt-0.5 text-[0.625rem] text-muted">{detail}</div>
    </div>
  );
}
