import { Check, ChevronDown, Users } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import type { CalendarTimezone } from "./calendar-dates";
import type { CalendarTypeFilter } from "./calendar-url-state";

export interface CalendarCharacterOption {
  connectionId: string;
  eveCharacterId: number;
  characterName: string;
}

export function CalendarFilters({
  characters,
  onCharactersChange,
  onTimezoneChange,
  onTypeChange,
  selectedCharacterIds,
  timezone,
  type,
}: {
  characters: CalendarCharacterOption[];
  onCharactersChange: (connectionIds: string[]) => void;
  onTimezoneChange: (timezone: CalendarTimezone) => void;
  onTypeChange: (type: CalendarTypeFilter) => void;
  selectedCharacterIds: string[];
  timezone: CalendarTimezone;
  type: CalendarTypeFilter;
}) {
  return (
    <div className="iw-calendar-filters flex flex-wrap items-center gap-2">
      <Segment
        label="Milestone type"
        onChange={onTypeChange}
        options={[
          ["all", "All", "All milestone types"],
          ["industry", "Industry", "Industry milestones"],
          ["skill", "Skills", "Skill milestones"],
          ["planetary", "Planetary", "Planetary timers"],
        ]}
        value={type}
      />
      <CharacterMenu characters={characters} onChange={onCharactersChange} selected={selectedCharacterIds} />
      <Segment
        label="Timezone"
        onChange={onTimezoneChange}
        options={[["eve", "EVE", "EVE time"], ["local", "Local", "Local time"]]}
        value={timezone}
      />
    </div>
  );
}

export function Segment<T extends string>({ label, onChange, options, value }: {
  label: string;
  onChange: (value: T) => void;
  options: readonly (readonly [T, string, string])[];
  value: T;
}) {
  return (
    <div aria-label={label} className="iw-calendar-segment" role="group">
      {options.map(([option, text, accessibleLabel]) => (
        <button aria-label={accessibleLabel} aria-pressed={option === value} key={option} onClick={() => onChange(option)} type="button">{text}</button>
      ))}
    </div>
  );
}

function CharacterMenu({ characters, onChange, selected }: {
  characters: CalendarCharacterOption[];
  onChange: (connectionIds: string[]) => void;
  selected: string[];
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const selectedCharacter = selected.length === 1
    ? characters.find(({ connectionId }) => connectionId === selected[0])
    : null;
  const summary = selected.length === 0
    ? "All characters"
    : selectedCharacter?.characterName ?? `${selected.length} characters`;

  function closeAndRestoreFocus() {
    triggerRef.current?.focus();
    setOpen(false);
  }

  useEffect(() => {
    if (!open) return;
    itemRefs.current[0]?.focus();
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) closeAndRestoreFocus();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  function handleMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeAndRestoreFocus();
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const current = itemRefs.current.indexOf(document.activeElement as HTMLButtonElement);
    const direction = event.key === "ArrowDown" ? 1 : -1;
    const next = (current + direction + characters.length) % characters.length;
    itemRefs.current[next]?.focus();
  }

  function toggle(connectionId: string) {
    const next = selected.length === 0
      ? [connectionId]
      : selected.includes(connectionId)
        ? selected.filter((id) => id !== connectionId)
        : [...selected, connectionId];
    onChange(next);
    closeAndRestoreFocus();
  }

  return (
    <div className="relative" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Filter by character: ${summary}`}
        className="iw-calendar-character-trigger"
        onClick={() => setOpen((value) => !value)}
        ref={triggerRef}
        type="button"
      >
        <Users aria-hidden="true" className="h-3 w-3" />
        <span className="max-w-32 truncate">{summary}</span>
        <ChevronDown aria-hidden="true" className="h-3 w-3" />
      </button>
      {open ? (
        <div className="iw-calendar-character-menu" onKeyDown={handleMenuKeyDown} role="menu">
          {characters.map((character, index) => {
            const checked = selected.length === 0 || selected.includes(character.connectionId);
            return (
              <button
                aria-checked={checked}
                aria-label={character.characterName}
                className="iw-calendar-character-option"
                key={character.connectionId}
                onClick={() => toggle(character.connectionId)}
                ref={(node) => { itemRefs.current[index] = node; }}
                role="menuitemcheckbox"
                type="button"
              >
                <EveCharacterPortrait characterId={character.eveCharacterId} characterName={character.characterName} size={32} />
                <span className="min-w-0 flex-1 truncate" title={character.characterName}>{character.characterName}</span>
                <Check aria-hidden="true" className={`h-3.5 w-3.5 ${checked ? "opacity-100" : "opacity-0"}`} />
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
