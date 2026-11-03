import { Check, Copy, X } from "lucide-react";
import { useEffect, useId, useState } from "react";

import type { ColonyTemplate, TemplatePin } from "../../../../api/pi-planner";
import { eveTypeImageUrl } from "../../../../components/eve-image-url";
import { InlineAlert, ProgressBar } from "../../../../components/primitives";

const NODE = 13;
const PAD = 30;

const KIND_LABEL: Record<TemplatePin["kind"], string> = {
  launchpad: "Launchpad",
  storage: "Storage",
  factory: "Factory",
  extractor: "Extractor",
};

const PLURAL: Record<TemplatePin["kind"], string> = {
  launchpad: "launchpads",
  storage: "storage facilities",
  factory: "factories",
  extractor: "extractors",
};

const KIND_CLASS: Record<TemplatePin["kind"], string> = {
  launchpad: "fill-primary/20 stroke-primary",
  storage: "fill-muted/20 stroke-muted",
  factory: "fill-reaction/20 stroke-reaction",
  extractor: "fill-positive/20 stroke-positive",
};

const KIND_DOT: Record<TemplatePin["kind"], string> = {
  launchpad: "border-primary bg-primary/20",
  storage: "border-muted bg-muted/20",
  factory: "border-reaction bg-reaction/20",
  extractor: "border-positive bg-positive/20",
};

function pinTitle(pin: TemplatePin): string {
  const parts = [pin.name];
  if (pin.product) parts.push(pin.kind === "extractor" ? `extracts ${pin.product.name}` : `makes ${pin.product.name}`);
  if (pin.heads > 0) parts.push(`${pin.heads} heads`);
  return parts.join(" · ");
}

/** The colony's structures as laid out in the template, hub in the middle. */
function TemplateLayout({ template }: { template: ColonyTemplate }) {
  const points = template.pins.map((pin) => ({ x: Number(pin.xKm), y: -Number(pin.yKm) }));
  const reach = Math.max(1, ...points.map((point) => Math.max(Math.abs(point.x), Math.abs(point.y))));
  const scale = (120 - NODE) / reach;
  const at = (index: number) => ({ x: points[index].x * scale, y: points[index].y * scale });
  const size = 2 * (120 + PAD);
  return (
    <svg
      aria-label="Template layout"
      className="mx-auto block w-full max-w-sm"
      role="img"
      viewBox={`${-size / 2} ${-size / 2} ${size} ${size}`}
    >
      {template.links.map(([from, to]) => {
        const a = at(from);
        const b = at(to);
        return <line className="stroke-border" key={`${from}-${to}`} strokeWidth={1.5} x1={a.x} x2={b.x} y1={a.y} y2={b.y} />;
      })}
      {template.pins.map((pin, index) => {
        const { x, y } = at(index);
        const icon = pin.product?.typeId ?? pin.typeId;
        return (
          <g data-kind={pin.kind} data-testid="template-pin" key={index} transform={`translate(${x} ${y})`}>
            <title>{pinTitle(pin)}</title>
            {pin.kind === "launchpad" ? (
              <rect className={KIND_CLASS[pin.kind]} height={NODE * 2} rx={4} strokeWidth={1.5} width={NODE * 2} x={-NODE} y={-NODE} />
            ) : (
              <circle className={KIND_CLASS[pin.kind]} r={NODE} strokeWidth={1.5} />
            )}
            <image height={NODE * 1.4} href={eveTypeImageUrl(icon, "icon", 32)} width={NODE * 1.4} x={-NODE * 0.7} y={-NODE * 0.7} />
            {pin.heads > 0 ? (
              <text className="fill-foreground font-mono text-[9px]" textAnchor="middle" y={NODE + 10}>
                ×{pin.heads}
              </text>
            ) : null}
          </g>
        );
      })}
    </svg>
  );
}

function Load({ label, used }: { label: string; used: number }) {
  const percent = used * 100;
  return (
    <div className="grid gap-1">
      <div className="flex justify-between text-[0.65rem] uppercase text-muted">
        <span>{label}</span>
        <span className={`font-mono ${used > 1 ? "text-danger" : "text-foreground"}`}>{percent.toFixed(0)}%</span>
      </div>
      <ProgressBar percent={percent} tone={used > 1 ? "danger" : used > 0.95 ? "warning" : "primary"} />
    </div>
  );
}

/** A colony's EVE template: what it builds, whether it fits, and the JSON to copy. */
export function ColonyTemplateDialog({
  onClose,
  planetName,
  template,
}: {
  onClose: () => void;
  planetName: string;
  template: ColonyTemplate;
}) {
  const titleId = useId();
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const counts = (["factory", "extractor", "storage", "launchpad"] as const)
    .map((kind) => [kind, template.pins.filter((pin) => pin.kind === kind).length] as const)
    .filter(([, count]) => count > 0);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" onClick={onClose} role="presentation">
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-lg p-4"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold" id={titleId}>
            {planetName} template
          </h2>
          <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        <div className="iw-dialog-body mt-3 grid gap-3 text-xs">
          <TemplateLayout template={template} />
          <p className="flex flex-wrap justify-center gap-x-3 text-muted">
            {counts.map(([kind, count]) => (
              <span className="inline-flex items-center gap-1" key={kind}>
                <span aria-hidden="true" className={`h-2 w-2 rounded-full border ${KIND_DOT[kind]}`} />
                {count} {count === 1 ? KIND_LABEL[kind].toLowerCase() : PLURAL[kind]}
              </span>
            ))}
          </p>
          <div className="grid grid-cols-2 gap-3">
            <Load label="CPU" used={Number(template.cpuUsed)} />
            <Load label="Power" used={Number(template.powerUsed)} />
          </div>
          {template.fits ? null : (
            <InlineAlert title="Over the command center's limit">
              With these link lengths the colony needs more CPU or power than the command center gives. Remove a
              factory or some heads before you submit.
            </InlineAlert>
          )}
          <ol className="list-decimal space-y-0.5 pl-4 text-muted">
            <li>Place the command center and upgrade it to the level the plan uses.</li>
            <li>Templates › Import &amp; Export › Save from Clipboard, then place the template.</li>
            <li>Check CPU, power and routes, then Submit.</li>
            <li>Drag the extractor heads onto the hotspot and start the programs.</li>
          </ol>
          <div className="flex items-center gap-2">
            <span className="text-[0.65rem] text-muted">Generated by ISK Works, not yet tested in game.</span>
            <button
              className="iw-button-primary ml-auto inline-flex items-center gap-1.5"
              onClick={() => void navigator.clipboard?.writeText(template.json).then(() => setCopied(true))}
              type="button"
            >
              {copied ? <Check aria-hidden="true" className="h-3.5 w-3.5" /> : <Copy aria-hidden="true" className="h-3.5 w-3.5" />}
              {copied ? "Copied" : "Copy template"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
