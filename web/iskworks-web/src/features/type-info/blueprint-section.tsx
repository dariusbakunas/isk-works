import { Atom, Copy, Factory, Gem, Hourglass, Microscope, type LucideIcon } from "lucide-react";

import type { CharacterRosterEntry } from "../../api/characters";
import type { BlueprintActivity, BlueprintActivityKind, TypeInfo } from "../../api/types";
import { TypeIcon } from "./meta-tag";
import { TypeLink, type OpenType } from "./rich-text";
import { DIVIDER, KeyValue, PilotBar, SectionTitle, SkillRows, pilotChecked } from "./type-info-sections";

/** The game's activity switcher, in its order. */
const ACTIVITIES: { kind: BlueprintActivityKind; label: string; icon: LucideIcon }[] = [
  { kind: "manufacturing", label: "Manufacturing", icon: Factory },
  { kind: "researchMaterial", label: "Material efficiency research", icon: Gem },
  { kind: "researchTime", label: "Time efficiency research", icon: Hourglass },
  { kind: "copying", label: "Copying", icon: Copy },
  { kind: "invention", label: "Invention", icon: Microscope },
  { kind: "reaction", label: "Reactions", icon: Atom },
];

/** The game's `01:40:00`, with days in front past a day. */
export function formatActivityTime(seconds: number): string {
  const days = Math.floor(seconds / 86_400);
  const pad = (value: number) => String(value).padStart(2, "0");
  const clock = `${pad(Math.floor((seconds % 86_400) / 3600))}:${pad(Math.floor((seconds % 3600) / 60))}:${pad(seconds % 60)}`;
  return days ? `${days}d ${clock}` : clock;
}

function percent(probability: string): string {
  return `${(Number(probability) * 100).toFixed(1)}%`;
}

/** The activity showing: the one picked if the blueprint has it, else its first. */
export function shownActivity(blueprint: NonNullable<TypeInfo["blueprint"]>, picked: string | undefined): BlueprintActivity | undefined {
  return (
    blueprint.activities.find((activity) => activity.kind === picked) ??
    ACTIVITIES.map(({ kind }) => blueprint.activities.find((activity) => activity.kind === kind)).find(Boolean)
  );
}

function OutcomeRow({ typeId, name, bpc, detail, onOpen }: { typeId: number; name: string; bpc?: boolean; detail?: string; onOpen?: OpenType }) {
  return (
    <li className={`flex h-8 items-center gap-2 border-b text-[10px] ${DIVIDER}`}>
      <TypeIcon name={name} size={24} typeId={typeId} variation={bpc ? "bpc" : "icon"} />
      <span className="min-w-0 truncate">{onOpen ? <TypeLink name={name} onOpen={onOpen} typeId={typeId} /> : name}</span>
      {detail ? <span className="ml-auto shrink-0 text-[8px] text-subtle">{detail}</span> : null}
    </li>
  );
}

function Outcome({ info, activity, onOpen }: { info: TypeInfo; activity: BlueprintActivity; onOpen: OpenType }) {
  switch (activity.kind) {
    case "researchMaterial":
      return <p className="py-1.5 text-[10px]"><b>+1%</b> Blueprint Material Efficiency</p>;
    case "researchTime":
      return <p className="py-1.5 text-[10px]"><b>+2%</b> Blueprint Time Efficiency</p>;
    case "copying":
      return (
        <ul>
          <OutcomeRow bpc name={info.name} typeId={info.typeId} />
        </ul>
      );
    case "invention": {
      // One chance for all outcomes is the activity's; otherwise each row has its own.
      const mixed = new Set(activity.outcome.map((outcome) => outcome.probability)).size > 1;
      return (
        <ul>
          {activity.outcome.map((outcome) => {
            const detail = [
              outcome.quantity > 1 ? `${outcome.quantity.toLocaleString("en-US")} runs` : null,
              mixed && outcome.probability ? percent(outcome.probability) : null,
            ].filter(Boolean);
            return <OutcomeRow bpc detail={detail.join(" · ") || undefined} key={outcome.typeId} name={outcome.name} onOpen={onOpen} typeId={outcome.typeId} />;
          })}
        </ul>
      );
    }
    default:
      return activity.outcome.length ? (
        <ul>
          {activity.outcome.map((outcome) => (
            <OutcomeRow
              detail={`${outcome.quantity.toLocaleString("en-US")} / run`}
              key={outcome.typeId}
              name={outcome.name}
              onOpen={onOpen}
              typeId={outcome.typeId}
            />
          ))}
        </ul>
      ) : (
        <None />
      );
  }
}

function None() {
  return <p className="py-1.5 text-[9px] text-subtle">None</p>;
}

/** A blueprint's Industry tab (the game's blueprint info window): pick an
 * activity, see its time, outcome, skills and materials. */
export function BlueprintSection({
  info,
  blueprint,
  activity: picked,
  onActivity,
  pilot,
  characters,
  onPilot,
  onOpen,
}: {
  info: TypeInfo;
  blueprint: NonNullable<TypeInfo["blueprint"]>;
  /** The picked activity's kind, if any. */
  activity: string | undefined;
  onActivity: (kind: BlueprintActivityKind) => void;
  pilot: string | undefined;
  /** Characters with synced skills; `null` while loading. */
  characters: CharacterRosterEntry[] | null;
  onPilot: (pilot: string) => void;
  onOpen: OpenType;
}) {
  const activity = shownActivity(blueprint, picked);
  if (!activity) return null;
  const label = ACTIVITIES.find((entry) => entry.kind === activity.kind)?.label ?? activity.kind;
  const chances = new Set(activity.outcome.map((outcome) => outcome.probability).filter((value): value is string => value !== null));
  const materialCount = activity.materials.reduce((total, group) => total + group.items.length, 0);
  return (
    <div className="p-3">
      <div aria-label="Activity" className="flex justify-center gap-1" role="group">
        {ACTIVITIES.map(({ kind, label: name, icon: Icon }) => {
          const available = blueprint.activities.some((entry) => entry.kind === kind);
          const selected = kind === activity.kind;
          return (
            <button
              aria-label={name}
              aria-pressed={selected}
              className={`grid h-8 w-9 place-items-center border focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${
                selected
                  ? "border-primary bg-primary/20 text-bright"
                  : available
                    ? "border-border text-muted hover:bg-hover hover:text-foreground"
                    : "cursor-not-allowed border-[#171f2c] text-subtle opacity-30"
              }`}
              disabled={!available}
              key={kind}
              onClick={() => onActivity(kind)}
              title={available ? name : `${name}: not available for this blueprint`}
              type="button"
            >
              <Icon aria-hidden="true" className="h-4 w-4" />
            </button>
          );
        })}
      </div>

      <h4 className="mb-1 mt-3 text-[11px] font-semibold text-bright">{label}</h4>
      <KeyValue label="Time per run">
        <span className="font-mono">{activity.timeSeconds !== null ? formatActivityTime(activity.timeSeconds) : "—"}</span>
      </KeyValue>
      {activity.kind === "copying" && blueprint.maxRuns !== null ? (
        <KeyValue label="Max runs per copy">
          <span className="font-mono">{blueprint.maxRuns.toLocaleString("en-US")}</span>
        </KeyValue>
      ) : null}
      {activity.kind === "invention" && chances.size === 1 ? (
        <KeyValue label="Success probability">
          <span className="font-mono">{percent([...chances][0])}</span>
        </KeyValue>
      ) : null}

      <SectionTitle>Outcome</SectionTitle>
      <Outcome activity={activity} info={info} onOpen={onOpen} />

      <SectionTitle>Required skills [{activity.skills.length}]</SectionTitle>
      {activity.skills.length ? (
        <>
          <PilotBar characters={characters} missing={activity.missing} onPilot={onPilot} pilot={pilot} />
          <div className="mt-1.5">
            <SkillRows checked={pilotChecked(pilot, characters, activity.missing)} nodes={activity.skills} onOpen={onOpen} />
          </div>
        </>
      ) : (
        <None />
      )}

      <SectionTitle>Required input materials [{materialCount}]</SectionTitle>
      {activity.materials.length ? (
        activity.materials.map((group) => (
          <section aria-label={group.name} key={group.name}>
            <h5 className="mt-1.5 text-[9px] text-muted">
              {group.name} [{group.items.length}]
            </h5>
            <ul>
              {group.items.map((item) => (
                <li className={`flex h-7 items-center gap-2 border-b text-[9px] ${DIVIDER}`} key={item.typeId}>
                  <TypeIcon name={item.name} size={20} typeId={item.typeId} />
                  <span className="w-14 shrink-0 text-right font-mono">{item.quantity.toLocaleString("en-US")} x</span>
                  <span className="min-w-0 truncate">
                    <TypeLink name={item.name} onOpen={onOpen} typeId={item.typeId} />
                  </span>
                </li>
              ))}
            </ul>
          </section>
        ))
      ) : (
        <None />
      )}
    </div>
  );
}
