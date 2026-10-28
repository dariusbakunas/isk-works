import { ChevronDown, Info } from "lucide-react";
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";

import type { CharacterRosterEntry } from "../../api/characters";
import type { MetaGroupList, SkillNode, TypeInfo, UsedInPage } from "../../api/types";
import { MetaMark, TypeIcon } from "./meta-tag";
import { RichText, TypeLink, type OpenType } from "./rich-text";

const GREEN = "#5c9d68";
const DIVIDER = "border-[#171f2c]";
const ROMAN = ["", "I", "II", "III", "IV", "V"];

export function SectionTitle({ children }: { children: ReactNode }) {
  return <h3 className="mb-1.5 mt-3 text-[8px] font-semibold uppercase tracking-[.12em] text-subtle">{children}</h3>;
}

export function LoadingRows() {
  return (
    <div aria-busy="true" aria-label="Loading" className="p-4" role="status">
      {Array.from({ length: 9 }, (_, index) => (
        <div className={`flex h-8 items-center gap-3 border-b ${DIVIDER}`} key={index}>
          <i className="block h-2.5 bg-panel-strong" style={{ width: index % 3 === 0 ? "42%" : "58%" }} />
          <i className="ml-auto block h-2.5 bg-panel-strong" style={{ width: 50 + (index % 2) * 35 }} />
        </div>
      ))}
    </div>
  );
}

export function NotInStaticData() {
  return (
    <div className="grid h-full place-items-center p-8 text-center">
      <div>
        <Info aria-hidden="true" className="mx-auto mb-3 h-[22px] w-[22px] text-subtle" />
        <p className="text-[10px] text-muted">This item isn't in the current static data.</p>
        <p className="mt-1 text-[8px] text-subtle">Window navigation and history remain available.</p>
      </div>
    </div>
  );
}

export function DescriptionSection({ info, onOpen }: { info: TypeInfo; onOpen: OpenType }) {
  return (
    <div className="p-4 text-[11px] leading-[1.65] text-foreground">
      <p>
        <RichText onOpen={onOpen} segments={info.description} />
      </p>
    </div>
  );
}

/** Two decimals, or three significant digits for a value that would round
 * to 0 (Antimatter Charge S is 0.0025 m³), like the API's attributes. */
function amount(value: string): string {
  const number = Number(value);
  if (number !== 0 && Math.abs(number) < 0.005) {
    return number.toLocaleString("en-US", { maximumSignificantDigits: 3 });
  }
  return number.toLocaleString("en-US", { maximumFractionDigits: 2 });
}

function volume(value: string | null): string | null {
  if (value === null) return null;
  return `${amount(value)} m³`;
}

function KeyValue({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={`grid min-h-7 grid-cols-2 items-center gap-2 border-b text-[9px] ${DIVIDER}`}>
      <span className="text-muted">{label}</span>
      <span className="text-right text-foreground">{children}</span>
    </div>
  );
}

export function hasBasics(info: TypeInfo): boolean {
  const { volumeM3, packagedVolumeM3, massKg, capacityM3, portionSize } = info.basics;
  return (
    [volumeM3, packagedVolumeM3, massKg, capacityM3].some((value) => value !== null && Number(value) !== 0) ||
    (portionSize ?? 1) > 1
  );
}

export function AttributesSection({ info, onOpen }: { info: TypeInfo; onOpen: OpenType }) {
  const { volumeM3, packagedVolumeM3, massKg, capacityM3, portionSize } = info.basics;
  const basics: [string, string | null][] = [
    ["Volume", volume(volumeM3)],
    ["Packaged volume", packagedVolumeM3 !== volumeM3 ? volume(packagedVolumeM3) : null],
    ["Mass", massKg !== null ? `${amount(massKg)} kg` : null],
    ["Capacity", capacityM3 !== null && Number(capacityM3) !== 0 ? volume(capacityM3) : null],
    // How many reprocess (or stack into a portion) at once: 100 for ammo.
    ["Units to reprocess", portionSize !== null && portionSize > 1 ? portionSize.toLocaleString("en-US") : null],
  ];
  const shownBasics = basics.filter((row): row is [string, string] => row[1] !== null);
  return (
    <div className="p-3">
      {shownBasics.length ? (
        <>
          <SectionTitle>Basics</SectionTitle>
          {shownBasics.map(([label, value]) => (
            <KeyValue key={label} label={label}>
              {value}
            </KeyValue>
          ))}
        </>
      ) : null}
      {info.attributes.map((group) => (
        <Fragment key={group.category}>
          <SectionTitle>{group.category}</SectionTitle>
          {group.rows.map((row) => (
            <KeyValue key={row.attributeId} label={row.label}>
              {row.link?.kind === "type" ? (
                <TypeLink name={row.link.name} onOpen={onOpen} typeId={row.link.id} />
              ) : row.link?.kind === "group" ? (
                <span className="text-primary">{row.display}</span>
              ) : (
                row.display
              )}
            </KeyValue>
          ))}
        </Fragment>
      ))}
    </div>
  );
}

export function TraitsSection({ info, onOpen }: { info: TypeInfo; onOpen: OpenType }) {
  return (
    <div className="p-3">
      {info.traits.map((group) => (
        <section key={group.heading}>
          <SectionTitle>{group.heading}</SectionTitle>
          {group.bonuses.map((bonus, index) => (
            <div className="grid grid-cols-[42px_1fr] py-1.5 text-[10px]" key={index}>
              <b className="pr-3 text-right font-mono text-primary">{bonus.amount ?? ""}</b>
              <span>
                <RichText onOpen={onOpen} segments={bonus.text} />
              </span>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

/** `3d 4h`, `5h 12m`, `40m`. */
export function formatTrainingTime(seconds: number): string {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days) return `${days}d ${hours}h`;
  if (hours) return `${hours}h ${minutes}m`;
  return `${Math.max(minutes, seconds > 0 ? 1 : 0)}m`;
}

function flatten(nodes: SkillNode[], depth = 0): { node: SkillNode; depth: number }[] {
  return nodes.flatMap((node) => [{ node, depth }, ...flatten(node.children, depth + 1)]);
}

function PilotPicker({
  pilot,
  characters,
  onPilot,
}: {
  pilot: string;
  characters: CharacterRosterEntry[];
  onPilot: (pilot: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const options = [
    { value: "allV", label: "All V" },
    { value: "allZero", label: "All 0" },
    ...characters.map((character) => ({ value: character.connectionId, label: character.characterName })),
  ];
  const current = options.find((option) => option.value === pilot) ?? options[0];

  useEffect(() => {
    if (!open) return;
    function close(event: PointerEvent) {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);

  return (
    <div className="relative" ref={ref}>
      <button
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={`Pilot: ${current.label}`}
        className="flex h-6 items-center gap-2 border border-border px-2 text-[8px] text-foreground hover:bg-hover"
        onClick={() => setOpen((value) => !value)}
        type="button"
      >
        {current.label}
        <ChevronDown aria-hidden="true" className="h-2.5 w-2.5" />
      </button>
      {open ? (
        <ul aria-label="Pilot" className="absolute left-0 top-7 z-20 min-w-[150px] border border-border bg-[#151b23] py-1 shadow-xl" role="listbox">
          {options.map((option) => (
            <li
              aria-selected={option.value === current.value}
              className={`flex h-7 cursor-pointer items-center px-2 text-[9px] hover:bg-[#1a222c] ${option.value === current.value ? "text-bright" : "text-foreground"}`}
              key={option.value}
              onClick={() => {
                setOpen(false);
                onPilot(option.value);
              }}
              role="option"
            >
              {option.label}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function RequirementsSection({
  info,
  pilot,
  characters,
  onPilot,
  onOpen,
}: {
  info: TypeInfo;
  pilot: string;
  characters: CharacterRosterEntry[];
  onPilot: (pilot: string) => void;
  onOpen: OpenType;
}) {
  const rows = flatten(info.requirements);
  const missing = info.missing;
  const allMet = rows.every(({ node }) => node.trainedLevel >= node.level);
  const summary = missing
    ? missing.missingSkills === 0
      ? null
      : `Missing ${missing.missingSkills} ${missing.missingSkills === 1 ? "skill" : "skills"}${
          missing.remainingSeconds ? ` · ${formatTrainingTime(missing.remainingSeconds)} to train` : ""
        }`
    : allMet
      ? null
      : "Not all requirements met";
  return (
    <div className="p-3">
      <div className="flex items-center gap-2">
        <PilotPicker characters={characters} onPilot={onPilot} pilot={pilot} />
        <span className={`ml-auto text-[9px] ${summary ? "text-warning" : ""}`} style={summary ? undefined : { color: GREEN }}>
          {summary ?? "All requirements met"}
        </span>
      </div>
      <ul className="mt-3">
        {rows.map(({ node, depth }, index) => {
          const met = node.trainedLevel >= node.level;
          return (
            <li className={`grid h-8 grid-cols-[1fr_64px_16px] items-center border-b text-[9px] ${DIVIDER}`} key={`${node.skillTypeId}-${index}`}>
              <span className="truncate text-foreground" style={{ paddingLeft: depth * 14 }}>
                <TypeLink name={node.name ?? `Skill ${node.skillTypeId}`} onOpen={onOpen} typeId={node.skillTypeId}>
                  <span className="text-foreground">{node.name ?? `Skill ${node.skillTypeId}`}</span>
                </TypeLink>{" "}
                <b>{ROMAN[node.level]}</b>
              </span>
              <span aria-label={`Trained ${node.trainedLevel} of ${node.level}`} className="flex gap-1" role="img">
                {[1, 2, 3, 4, 5].map((level) => {
                  const trained = level <= node.trainedLevel;
                  const short = !met && level <= node.level && !trained;
                  return (
                    <i
                      className={`block h-2 w-2 border ${short ? "border-warning" : "border-primary"} ${trained ? "bg-primary" : ""}`}
                      key={level}
                    />
                  );
                })}
              </span>
              <b aria-label={met ? "Met" : "Missing"} className={met ? "" : "text-warning"} style={met ? { color: GREEN } : undefined}>
                {met ? "✓" : "✕"}
              </b>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export function MetaGroupLists({
  groups,
  currentTypeId,
  onOpen,
}: {
  groups: MetaGroupList[];
  currentTypeId?: number;
  onOpen: OpenType;
}) {
  return (
    <div className="p-3">
      {groups.map((group) => (
        <section key={group.metaGroupId}>
          <SectionTitle>{group.metaGroupName}</SectionTitle>
          <ul>
            {group.items.map((item) => {
              const current = item.typeId === currentTypeId;
              return (
                <li className={`relative flex h-7 items-center gap-2 px-1 ${current ? "bg-[#0e1e2e]" : ""}`} key={item.typeId}>
                  {current ? <i aria-hidden="true" className="absolute inset-y-0 left-0 w-0.5 bg-primary" /> : null}
                  <TypeIcon metaGroupId={group.metaGroupId} name={item.name} size={22} typeId={item.typeId} />
                  {current ? (
                    <span aria-current="true" className="truncate text-[9px] text-bright">
                      {item.name}
                    </span>
                  ) : (
                    <span className="min-w-0 truncate text-[9px]">
                      <TypeLink name={item.name} onOpen={onOpen} typeId={item.typeId}>
                        <span className="text-foreground hover:text-primary">{item.name}</span>
                      </TypeLink>
                    </span>
                  )}
                  <MetaMark metaGroupId={group.metaGroupId} />
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}

export function IndustrySection({
  info,
  usedIn,
  loadingMore,
  onMore,
  onOpen,
}: {
  info: TypeInfo;
  usedIn: UsedInPage | null;
  loadingMore: boolean;
  onMore: () => void;
  onOpen: OpenType;
}) {
  const { producedBy, reprocessesInto, usedInCount } = info.industry;
  const remaining = usedIn ? usedIn.total - usedIn.items.length : 0;
  return (
    <div className="p-3">
      {producedBy.length ? (
        <>
          <SectionTitle>Produced by</SectionTitle>
          {producedBy.map((producer) => (
            <div className="flex h-8 items-center gap-2" key={producer.typeId}>
              <TypeIcon name={producer.name ?? ""} size={24} typeId={producer.typeId} variation="bp" />
              <TypeLink name={producer.name ?? `Type ${producer.typeId}`} onOpen={onOpen} typeId={producer.typeId} />
              {producer.kind === "reaction" ? <span className="ml-auto text-[8px] text-subtle">Reaction</span> : null}
            </div>
          ))}
        </>
      ) : null}
      <SectionTitle>{usedInCount ? `Used in · ${usedInCount.toLocaleString("en-US")} items` : "Used in"}</SectionTitle>
      {usedInCount === 0 ? (
        <p className="text-[9px] text-subtle">Not used as a material</p>
      ) : usedIn === null ? (
        <p className="text-[9px] text-subtle">Loading…</p>
      ) : (
        <>
          <ul>
            {usedIn.items.map((item) => (
              <li className={`flex h-8 items-center gap-2 border-b ${DIVIDER}`} key={`${item.kind}-${item.recipeTypeId}`}>
                <TypeIcon name={item.recipeName} size={24} typeId={item.recipeTypeId} variation="bp" />
                <span className="min-w-0 truncate text-[10px]">
                  <TypeLink name={item.recipeName} onOpen={onOpen} typeId={item.recipeTypeId} />
                </span>
                <span className="ml-auto shrink-0 text-[8px] text-subtle">{item.quantity.toLocaleString("en-US")} / run</span>
              </li>
            ))}
          </ul>
          {remaining > 0 ? (
            <button className="h-8 text-[9px] text-primary hover:underline disabled:opacity-60" disabled={loadingMore} onClick={onMore} type="button">
              {loadingMore ? "Loading…" : `Show more · ${remaining.toLocaleString("en-US")} items`}
            </button>
          ) : null}
        </>
      )}
      {reprocessesInto.length ? (
        <>
          <SectionTitle>Reprocessed materials</SectionTitle>
          <ul>
            {reprocessesInto.map((material) => (
              <li className={`flex h-7 items-center gap-2 border-b text-[9px] ${DIVIDER}`} key={material.typeId}>
                <TypeIcon name={material.name} size={20} typeId={material.typeId} />
                <TypeLink name={material.name} onOpen={onOpen} typeId={material.typeId} />
                <span className="ml-auto font-mono">{material.quantity.toLocaleString("en-US")}</span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}
