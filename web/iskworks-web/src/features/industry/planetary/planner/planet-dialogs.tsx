import { useEffect, useState, type FormEvent, type ReactNode } from "react";

import type { PlannerPlanet } from "../../../../api/pi-planner";
import { percentRate, ratePercent } from "./planner-format";

function Dialog({
  children,
  id,
  onCancel,
  onSubmit,
  title,
}: {
  children: ReactNode;
  id: string;
  onCancel: () => void;
  onSubmit: () => void;
  title: string;
}) {
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit();
  };
  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4"
      onKeyDown={(event) => event.key === "Escape" && onCancel()}
      role="presentation"
    >
      <form
        aria-labelledby={id}
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-sm p-4"
        onSubmit={submit}
        role="dialog"
      >
        <h2 className="text-base font-semibold" id={id}>
          {title}
        </h2>
        <div className="iw-dialog-body mt-3 grid gap-3">{children}</div>
        <div className="mt-4 flex justify-end gap-2">
          <button className="iw-button-secondary" onClick={onCancel} type="button">
            Cancel
          </button>
          <button className="iw-button-primary" type="submit">
            Save
          </button>
        </div>
      </form>
    </div>
  );
}

/**
 * Head values for one planet: the average value the game shows next to each
 * extractor head in a draft program. A head value overrides the density.
 */
export function HeadValueDialog({
  onCancel,
  onSave,
  planet,
}: {
  onCancel: () => void;
  /** By resource type id; `null` clears a head value. */
  onSave: (headValues: Map<number, string | null>) => void;
  planet: PlannerPlanet;
}) {
  const [values, setValues] = useState(() =>
    new Map(planet.resources.map((resource) => [resource.typeId, resource.headValue ?? ""])),
  );
  useEffect(() => {
    setValues(new Map(planet.resources.map((resource) => [resource.typeId, resource.headValue ?? ""])));
  }, [planet]);
  const invalid = [...values.values()].some((value) => value !== "" && !(Number(value) > 0));

  return (
    <Dialog
      id="head-value-title"
      onCancel={onCancel}
      onSubmit={() => {
        if (invalid) return;
        onSave(new Map([...values].map(([typeId, value]) => [typeId, value.trim() === "" ? null : value.trim()])));
      }}
      title={`Head values · ${planet.name}`}
    >
      <p className="text-xs text-muted">
        Drag heads onto the hotspot in a draft program and read the value next to each head. Enter the average; it
        replaces the density % for that resource.
      </p>
      {planet.resources.map((resource) => (
        <label className="grid grid-cols-[1fr_6rem] items-center gap-2 text-sm" key={resource.typeId}>
          <span>{resource.name}</span>
          <input
            aria-label={`${resource.name} head value`}
            className="iw-input text-right font-mono"
            inputMode="decimal"
            onChange={(event) =>
              setValues((current) => new Map(current).set(resource.typeId, event.target.value.replace(/[^\d.]/g, "")))
            }
            placeholder="—"
            value={values.get(resource.typeId) ?? ""}
          />
        </label>
      ))}
      {invalid ? <p className="text-xs text-destructive">A head value is a positive number.</p> : null}
    </Dialog>
  );
}

/** One planet's customs office tax, overriding the scenario default. */
export function CustomsTaxDialog({
  defaultRate,
  onCancel,
  onSave,
  planet,
}: {
  defaultRate: string;
  onCancel: () => void;
  /** `null` falls back to the default. */
  onSave: (customsRate: string | null) => void;
  planet: PlannerPlanet;
}) {
  const [text, setText] = useState(planet.customsRate === null ? "" : ratePercent(planet.customsRate));
  const invalid = text.trim() !== "" && percentRate(text) === null;
  return (
    <Dialog
      id="customs-tax-title"
      onCancel={onCancel}
      onSubmit={() => {
        if (!invalid) onSave(percentRate(text));
      }}
      title={`Customs tax · ${planet.name}`}
    >
      <label className="grid grid-cols-[1fr_6rem] items-center gap-2 text-sm">
        <span>Tax %</span>
        <input
          aria-label="Customs tax percent"
          className="iw-input text-right font-mono"
          inputMode="decimal"
          onChange={(event) => setText(event.target.value.replace(/[^\d.]/g, ""))}
          placeholder={ratePercent(defaultRate)}
          value={text}
        />
      </label>
      <p className="text-xs text-muted">Leave blank to use the scenario default ({ratePercent(defaultRate)} %).</p>
      {invalid ? <p className="text-xs text-destructive">Tax is 0–100 %.</p> : null}
    </Dialog>
  );
}
