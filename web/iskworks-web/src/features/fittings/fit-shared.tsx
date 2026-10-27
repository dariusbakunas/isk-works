import { createContext, useContext } from "react";

import type { CharacterRosterEntry } from "../../api/characters";
import type { FittingsMeta } from "../../api/fittings";
import { EveCharacterPortrait } from "../../components/eve-character-portrait";
import { PRIVATE_ATTR } from "../../observability/private";

/** What every Fittings view needs: whether stats work, the damage
 * profiles, and the roster (for portraits). */
export interface FittingsContextValue {
  meta: FittingsMeta | null;
  roster: CharacterRosterEntry[];
}

export const FittingsContext = createContext<FittingsContextValue>({ meta: null, roster: [] });

export function useFittingsContext() {
  return useContext(FittingsContext);
}

/** Whether fit stats and fit text (import, export) work. */
export function useEngineAvailable(): boolean {
  return useFittingsContext().meta?.available ?? false;
}

/** A workspace character's portrait, by connection id. */
export function PilotPortrait({ connectionId, name, size = 16 }: { connectionId: string; name: string; size?: 16 | 32 }) {
  const { roster } = useFittingsContext();
  const entry = roster.find((character) => character.connectionId === connectionId);
  if (!entry) {
    return (
      <span
        {...PRIVATE_ATTR}
        className="grid shrink-0 place-items-center rounded border border-border bg-panel-strong text-[0.5rem] font-semibold text-muted"
        style={{ width: size, height: size }}
      >
        {name.slice(0, 2).toUpperCase()}
      </span>
    );
  }
  return <EveCharacterPortrait characterId={entry.eveCharacterId} characterName={name} size={size} />;
}

/** The operator hint shown wherever fit stats or fit text are missing. */
export function EngineUnavailableHint() {
  return (
    <p className="rounded-md border border-warning/60 bg-warning/10 px-3 py-2 text-xs text-warning" role="status">
      Fit stats and fit import/export are unavailable: the server has no fitting engine data. Saved fits and who can
      fly them still work. An operator fixes this by redeploying the API image (or running <code>make fitting-data</code>{" "}
      locally).
    </p>
  );
}
