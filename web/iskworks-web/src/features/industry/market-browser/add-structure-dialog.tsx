import { CheckCircle2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  resolveKnownStructure,
  searchKnownStructures,
  verifyStructureMarketAccess,
  type KnownStructure,
  type StructureMarketAccess,
} from "../../../api/industry";
import { ButtonLink, InlineAlert } from "../../../components/primitives";
import { CharacterName } from "../../../observability/private";
import { Field } from "../shared/field";
import { parseStructureReference } from "../shared/structure-reference";
import { useDebouncedLookup } from "../../../hooks/use-debounced-lookup";
import { apiMessage } from "./shared";

type Phase =
  | { status: "idle" }
  | { status: "verifying" }
  | { status: "resolved"; structure: KnownStructure; access: StructureMarketAccess }
  | { status: "error"; message: string };

/**
 * No existing UI lets a user turn a structure into a market scope --
 * `searchKnownStructures`/`resolveKnownStructure` (already built for the
 * Facilities feature, `facility-form-dialog.tsx`) cover "find or resolve a
 * structure by name/ID" with zero new ESI scope, since asset/blueprint sync
 * already discovers and resolves most structures a character has ever used
 * (`EsiApplicationService::sync_assets` -> `market_location_names`). What's
 * missing, and unique to this dialog, is the market-specific half: does any
 * connected character actually have `esi-markets.structure_markets.v1` and
 * docking access to *price* it.
 */
export function AddStructureDialog({
  open,
  onClose,
  onAdded,
}: {
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [query, setQuery] = useState("");
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [selected, setSelected] = useState<KnownStructure | null>(null);
  const [resolvingId, setResolvingId] = useState(false);
  const [needsReconnection, setNeedsReconnection] = useState(false);
  const [phase, setPhase] = useState<Phase>({ status: "idle" });

  const lookup = useDebouncedLookup(
    query,
    searchKnownStructures,
    (error) => setPhase({ status: "error", message: apiMessage(error) }),
    { enabled: open && dropdownOpen && !selected, minLength: 0 },
  );

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (open && !element.open) {
      if (typeof element.showModal === "function") element.showModal();
      else element.setAttribute("open", "");
    }
    if (!open && element.open) {
      if (typeof element.close === "function") element.close();
      else element.removeAttribute("open");
    }
  }, [open]);

  useEffect(() => {
    if (open) return;
    setQuery("");
    setDropdownOpen(false);
    setSelected(null);
    setNeedsReconnection(false);
    setPhase({ status: "idle" });
  }, [open]);

  function selectStructure(structure: KnownStructure) {
    setSelected(structure);
    setQuery(structure.structureName);
    setDropdownOpen(false);
    setPhase({ status: "idle" });
  }

  async function resolveById(structureId: number) {
    setResolvingId(true);
    setNeedsReconnection(false);
    setPhase({ status: "idle" });
    try {
      const resolution = await resolveKnownStructure(structureId);
      if (!resolution.configured) {
        setPhase({
          status: "error",
          message: "EVE SSO is not configured, so structures can't be resolved by ID.",
        });
        return;
      }
      if (resolution.structure) {
        selectStructure(resolution.structure);
        return;
      }
      if (resolution.needsReconnection) {
        setNeedsReconnection(true);
        setPhase({
          status: "error",
          message: "Reconnect an EVE character with structure read access to resolve this ID.",
        });
        return;
      }
      setPhase({
        status: "error",
        message:
          resolution.warnings[0] ??
          "Could not resolve this structure. Make sure a connected character has docking access.",
      });
    } catch (error) {
      setPhase({ status: "error", message: apiMessage(error) });
    } finally {
      setResolvingId(false);
    }
  }

  async function verifyAccess() {
    if (!selected) return;
    setPhase({ status: "verifying" });
    try {
      const access = await verifyStructureMarketAccess(selected.structureId);
      setPhase({ status: "resolved", structure: selected, access });
      if (access.access === "confirmed") onAdded();
    } catch (error) {
      setPhase({ status: "error", message: apiMessage(error) });
    }
  }

  const referencedId = !selected ? parseStructureReference(query) : null;
  const busy = phase.status === "verifying" || resolvingId;

  return (
    <dialog
      aria-labelledby="add-structure-dialog-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(480px,calc(100vw-2rem))] overflow-y-auto p-0 text-foreground backdrop:bg-black/70"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
      ref={dialog}
    >
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-sm font-semibold" id="add-structure-dialog-title">
          Add a Structure
        </span>
        <button aria-label="Close add structure dialog" className="iw-icon-button" disabled={busy} onClick={onClose} type="button">
          <X aria-hidden="true" className="h-4 w-4" />
        </button>
      </div>
      <div className="p-5">
        <p className="iw-muted text-sm">
          Search structures your characters already know about, or paste a structure ID/Show Info link. Market data can only be fetched by a connected character with docking access and market permissions there.
        </p>

        <div className="relative mt-4">
          <Field
            label="Structure"
            value={query}
            onChange={(value) => {
              setQuery(value);
              setDropdownOpen(true);
              setSelected(null);
              setPhase({ status: "idle" });
            }}
            onFocus={() => setDropdownOpen(true)}
          />
          {lookup.searching ? <p className="iw-muted mt-1 text-xs">Searching known structures...</p> : null}
          {!lookup.searching && dropdownOpen && !selected ? (
            <div className="absolute z-40 mt-1 max-h-56 w-full overflow-y-auto border border-border bg-panel shadow-xl">
              {lookup.results.length > 0 ? (
                lookup.results.map((structure) => (
                  <button
                    className="block w-full px-3 py-2 text-left hover:bg-panel-strong"
                    key={structure.structureId}
                    onClick={() => selectStructure(structure)}
                    type="button"
                  >
                    <span className="block text-sm">{structure.structureName}</span>
                    <span className="iw-muted block text-xs">
                      {[structure.structureTypeName, structure.solarSystemName].filter(Boolean).join(" · ") || "Resolved through ESI"}
                    </span>
                  </button>
                ))
              ) : (
                <div className="px-3 py-2">
                  <p className="iw-muted text-sm">
                    No known structures match. Sync assets/blueprints for a character with access there, or paste an ID.
                  </p>
                  {referencedId ? (
                    <button
                      className="iw-button-secondary mt-2"
                      disabled={resolvingId}
                      onClick={() => void resolveById(referencedId)}
                      type="button"
                    >
                      {resolvingId ? "Resolving via ESI..." : `Resolve structure ${referencedId} via ESI`}
                    </button>
                  ) : (
                    <p className="iw-muted mt-2 text-xs">
                      Have docking access but never synced assets there? Shift-drag the structure into
                      chat, mail, or a note in the EVE client to get a Show Info link
                      (<code>showinfo:...</code>), then paste it here.
                    </p>
                  )}
                  {needsReconnection ? (
                    <p className="mt-2 text-xs">
                      <ButtonLink to="/characters">Open Characters</ButtonLink>
                    </p>
                  ) : null}
                </div>
              )}
            </div>
          ) : null}
        </div>

        {phase.status === "error" ? (
          <div className="mt-4">
            <InlineAlert title="Structure not added">{phase.message}</InlineAlert>
          </div>
        ) : null}

        {phase.status === "resolved" ? (
          <div className="mt-4 border-t border-border pt-4">
            {phase.access.access === "confirmed" ? (
              <p className="flex items-center gap-2 text-sm text-success">
                <CheckCircle2 aria-hidden="true" className="h-4 w-4 shrink-0" />
                <CharacterName name={phase.access.characterName} /> can price this market.
              </p>
            ) : phase.access.access === "noEligibleCharacter" ? (
              <InlineAlert title="No character has market access granted" tone="info">
                Reconnect a character and grant read-only structure market access.
                <span className="ml-2">
                  <ButtonLink to="/characters">Open Characters</ButtonLink>
                </span>
              </InlineAlert>
            ) : (
              <InlineAlert title="No connected character can dock here" tone="info">
                Every connected character with market access was denied -- this usually means corp/alliance docking rights are needed.
              </InlineAlert>
            )}
          </div>
        ) : null}

        <div className="mt-5 flex justify-end gap-2">
          <button className="iw-button-secondary" disabled={busy} onClick={onClose} type="button">
            {phase.status === "resolved" && phase.access.access === "confirmed" ? "Done" : "Cancel"}
          </button>
          {phase.status !== "resolved" || phase.access.access !== "confirmed" ? (
            <button
              className="iw-button-primary"
              disabled={busy || !selected}
              onClick={() => void verifyAccess()}
              type="button"
            >
              {phase.status === "verifying" ? "Checking access..." : "Verify market access"}
            </button>
          ) : null}
        </div>
      </div>
    </dialog>
  );
}
