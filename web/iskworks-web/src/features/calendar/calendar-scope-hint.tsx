import { X } from "lucide-react";
import { useState } from "react";
import type { CalendarCharacterOption } from "./calendar-filters";

const DISMISSED_KEY = "iskworks.calendar.scopeHintDismissed";

/** Identifies the set of characters the hint was dismissed for. */
function hintKey(characters: CalendarCharacterOption[]): string {
  return characters.map((character) => character.connectionId).sort().join(",");
}

function readDismissed(): string | null {
  try {
    return window.localStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

function writeDismissed(key: string) {
  try {
    window.localStorage.setItem(DISMISSED_KEY, key);
  } catch {
    // Without storage the hint simply returns on the next visit.
  }
}

/**
 * Says which characters' in-game events are missing. EVE SSO lets the
 * player pick the character on its own login page, so the button cannot
 * target one: the copy says so instead of implying it can. Dismissed per
 * set of characters, so a newly connected character without the scope
 * brings it back.
 */
export function CalendarScopeHint({ characters, onGrant }: {
  characters: CalendarCharacterOption[];
  onGrant: () => void;
}) {
  const key = hintKey(characters);
  const [dismissed, setDismissed] = useState(() => readDismissed());
  const [expanded, setExpanded] = useState(false);
  if (characters.length === 0 || dismissed === key) return null;

  const single = characters.length === 1 ? characters[0] : null;
  return (
    <div aria-label="Calendar access" className="iw-calendar-scope-hint" role="note">
      <div className="min-w-0 flex-1">
        <p>
          <span className="font-semibold text-foreground">
            {single
              ? `Calendar events are missing for ${single.characterName}.`
              : `Calendar events are missing for ${characters.length} characters.`}
          </span>{" "}
          {single
            ? `Grant calendar access and log in as ${single.characterName} on EVE's page.`
            : "EVE's login page asks which character to authorize, so grant access once per character."}
          {single ? null : (
            <>
              {" "}
              <button aria-expanded={expanded} className="iw-calendar-scope-hint-toggle" onClick={() => setExpanded((value) => !value)} type="button">
                {expanded ? "Hide" : "Which ones?"}
              </button>
            </>
          )}
        </p>
        {expanded && !single ? (
          <ul aria-label="Characters without calendar access" className="iw-calendar-scope-hint-list">
            {characters.map((character) => <li key={character.connectionId}>{character.characterName}</li>)}
          </ul>
        ) : null}
      </div>
      <button className="iw-button-secondary" onClick={onGrant} type="button">Grant calendar access</button>
      <button
        aria-label="Dismiss calendar access note"
        className="iw-icon-button"
        onClick={() => {
          writeDismissed(key);
          setDismissed(key);
        }}
        type="button"
      >
        <X aria-hidden="true" className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
