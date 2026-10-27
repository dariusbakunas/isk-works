import { Check, ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";

import { searchFitCatalogue, type CatalogueItem, type PilotChoice } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";

/** The charges a module loads, by module type, for this page's life. */
const chargesByModule = new Map<number, Promise<CatalogueItem[]>>();

function chargesFor(moduleTypeId: number, pilot: PilotChoice): Promise<CatalogueItem[]> {
  const key = moduleTypeId;
  let charges = chargesByModule.get(key);
  if (!charges) {
    charges = searchFitCatalogue({ kind: "charges", moduleTypeId }, { pilot }).then((view) => view.entries);
    // A failed load is tried again next time.
    charges.catch(() => chargesByModule.delete(key));
    chargesByModule.set(key, charges);
  }
  return charges;
}

const FACTION_META_GROUP = 4;
const SIZE_GROUPS = new Set(["Small", "Medium", "Large", "Extra Large", "Capital", "XL"]);

/** The market group a charge is listed under: the deepest one, or the one
 * above it when that only says the size ("Standard Ammo", not "Small"). */
function heading(charge: CatalogueItem): string {
  const deepest = charge.marketPath.at(-1);
  if (deepest && SIZE_GROUPS.has(deepest) && charge.marketPath.length > 1) return charge.marketPath.at(-2)!;
  return deepest ?? "Other";
}

/** pyfa's charge menu: Tech I and II charges under their market group,
 * each faction charge folded under the Tech I charge it varies. */
function arrange(charges: CatalogueItem[]) {
  const listed = new Set(charges.map((charge) => charge.typeId));
  const variants = new Map<number, CatalogueItem[]>();
  const groups: Array<{ name: string; charges: CatalogueItem[] }> = [];
  for (const charge of charges) {
    const parent = charge.variationParent;
    if (charge.metaGroupId === FACTION_META_GROUP && parent !== null && listed.has(parent)) {
      variants.set(parent, [...(variants.get(parent) ?? []), charge]);
      continue;
    }
    const name = heading(charge);
    const group = groups.find((entry) => entry.name === name);
    if (group) group.charges.push(charge);
    else groups.push({ name, charges: [charge] });
  }
  return { groups, variants };
}

/**
 * A module's charge, picked from the charges it loads (as pyfa's Charge
 * menu): the module table's Charge cell for a gun, launcher or booster.
 */
export function ChargePicker({
  moduleTypeId,
  moduleName,
  chargeTypeId,
  chargeName,
  pilot,
  onPick,
}: {
  moduleTypeId: number;
  moduleName: string;
  chargeTypeId: number | null;
  chargeName: string | null;
  pilot: PilotChoice;
  onPick: (charge: { typeId: number; name: string } | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const [charges, setCharges] = useState<CatalogueItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** The Tech I charge whose faction variants are shown. */
  const [expanded, setExpanded] = useState<number | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    let live = true;
    setError(null);
    chargesFor(moduleTypeId, pilot)
      .then((loaded) => {
        if (!live) return;
        setCharges(loaded);
        // A loaded faction charge opens with its family shown.
        const current = loaded.find((charge) => charge.typeId === chargeTypeId);
        setExpanded(current?.metaGroupId === FACTION_META_GROUP ? current.variationParent : null);
      })
      .catch((reason: unknown) => live && setError(reason instanceof Error ? reason.message : "Couldn't load charges."));
    const close = () => setOpen(false);
    const outside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!triggerRef.current?.contains(target) && !menuRef.current?.contains(target)) close();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    // A fixed menu would drift from its cell on scroll or resize.
    const moved = (event: Event) => {
      if (!menuRef.current?.contains(event.target as Node)) close();
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    window.addEventListener("scroll", moved, true);
    window.addEventListener("resize", close);
    return () => {
      live = false;
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
      window.removeEventListener("scroll", moved, true);
      window.removeEventListener("resize", close);
    };
  }, [open, moduleTypeId, pilot]);

  const pick = (charge: { typeId: number; name: string } | null) => {
    setOpen(false);
    if ((charge?.typeId ?? null) !== chargeTypeId) onPick(charge);
  };

  const arranged = charges ? arrange(charges) : null;

  // Below the cell, or above it when there's no room underneath.
  const style: CSSProperties | undefined = anchor
    ? window.innerHeight - anchor.bottom >= 240
      ? { position: "fixed", top: anchor.bottom + 4, left: anchor.left, maxHeight: window.innerHeight - anchor.bottom - 16 }
      : { position: "fixed", bottom: window.innerHeight - anchor.top + 4, left: anchor.left, maxHeight: anchor.top - 16 }
    : undefined;

  const option = (
    key: string,
    label: string,
    checked: boolean,
    charge: { typeId: number; name: string } | null,
    dim = false,
    indent = false,
  ) => (
    <button
      aria-checked={checked}
      aria-label={label}
      className={`flex min-w-0 flex-1 items-center gap-2 rounded-md py-1 pr-2 text-left text-xs text-foreground hover:bg-panel ${indent ? "pl-7" : "pl-2"} ${dim ? "opacity-60" : ""}`}
      key={key}
      onClick={() => pick(charge)}
      role="menuitemradio"
      type="button"
    >
      {charge ? <EveTypeImage size={24} typeId={charge.typeId} typeName={charge.name} /> : <span className="h-6 w-6" />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {checked ? <Check aria-hidden="true" className="h-3.5 w-3.5 text-primary" /> : null}
    </button>
  );

  return (
    <>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Charge: ${chargeName ?? "none"}`}
        className="flex min-w-0 max-w-full items-center gap-1 rounded px-1 py-0.5 text-left hover:bg-panel-strong"
        onClick={(event) => {
          // A mouse click leaves no focus ring behind.
          if (event.detail > 0) event.currentTarget.blur();
          setAnchor(triggerRef.current?.getBoundingClientRect() ?? null);
          setOpen((current) => !current);
        }}
        ref={triggerRef}
        title="Pick a charge"
        type="button"
      >
        <span className={`truncate ${chargeName ? "text-muted" : "text-muted/60"}`}>{chargeName ?? "Load charge"}</span>
        <ChevronDown aria-hidden="true" className="h-3 w-3 shrink-0 text-muted" />
      </button>
      {open
        ? createPortal(
            <div
              aria-label={`Charges for ${moduleName}`}
              className="iw-panel-strong z-50 w-64 overflow-y-auto border p-1 shadow-2xl"
              ref={menuRef}
              role="menu"
              style={style}
            >
              {error ? <p className="px-2 py-1.5 text-xs text-danger">{error}</p> : null}
              {!error && charges === null ? <p className="px-2 py-1.5 text-xs text-muted">Loading charges…</p> : null}
              {arranged
                ? arranged.groups.map((group) => (
                    <div aria-label={group.name} key={group.name} role="group">
                      <p className="px-2 pb-0.5 pt-1.5 text-[0.625rem] uppercase tracking-wide text-muted">{group.name}</p>
                      {group.charges.map((charge) => {
                        const family = arranged.variants.get(charge.typeId) ?? [];
                        const open = expanded === charge.typeId;
                        return (
                          <div key={charge.typeId}>
                            <div className="flex items-center">
                              {option(String(charge.typeId), charge.name, charge.typeId === chargeTypeId, charge, charge.canUse === false)}
                              {family.length ? (
                                <button
                                  aria-expanded={open}
                                  aria-label={`Variants of ${charge.name}`}
                                  className="rounded p-1 text-muted hover:bg-panel hover:text-foreground"
                                  onClick={() => setExpanded(open ? null : charge.typeId)}
                                  title={`${family.length} faction ${family.length === 1 ? "variant" : "variants"}`}
                                  type="button"
                                >
                                  <ChevronRight aria-hidden="true" className={`h-3.5 w-3.5 transition ${open ? "rotate-90" : ""}`} />
                                </button>
                              ) : null}
                            </div>
                            {open
                              ? family.map((variant) => (
                                  <div className="flex" key={variant.typeId}>
                                    {option(
                                      String(variant.typeId),
                                      variant.name,
                                      variant.typeId === chargeTypeId,
                                      variant,
                                      variant.canUse === false,
                                      true,
                                    )}
                                  </div>
                                ))
                              : null}
                          </div>
                        );
                      })}
                    </div>
                  ))
                : null}
              {charges ? (
                <div className="mt-1 flex border-t border-border pt-1">{option("empty", "Empty", chargeTypeId === null, null)}</div>
              ) : null}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
