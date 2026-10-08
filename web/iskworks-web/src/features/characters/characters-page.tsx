import { Search, UserPlus } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import { getCharacter, listCharacters, type CharacterRosterEntry } from "../../api/characters";
import { beginAuthorization } from "../../api/esi";
import { ApiError } from "../../api/workspace";
import { EmptyState, InlineAlert, LoadingState, PageHeader } from "../../components/primitives";
import { CharacterCard } from "./character-card";
import { CharacterInspector } from "./character-inspector";
import {
  matchesFilter,
  matchesSearch,
  sortEntries,
  type RosterFilter,
  type RosterSort,
} from "./characters-formatters";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; entries: CharacterRosterEntry[] };

const filterOptions: { value: RosterFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "training", label: "Training" },
  { value: "industry", label: "Industry" },
  { value: "needsAttention", label: "Needs Attention" },
];

export function CharactersPage() {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<RosterFilter>("all");
  const [sort, setSort] = useState<RosterSort>("name");
  const [connecting, setConnecting] = useState(false);
  const [connectError, setConnectError] = useState("");
  const [openConnectionId, setOpenConnectionId] = useState<string | null>(null);

  function load() {
    listCharacters()
      .then((entries) => setState({ status: "ready", entries }))
      .catch((error: unknown) => {
        const message = error instanceof ApiError ? error.message : "Characters failed to load.";
        setState({ status: "error", message });
      });
  }

  useEffect(load, []);

  // Eventually-consistent safety net for out-of-band queue edits (made in
  // the EVE client) that a targeted per-character reconciliation wouldn't
  // catch on its own -- see handleTrainingNeedsRefresh below for the
  // completion-triggered path, which is faster but only fires when this
  // page notices a boundary crossing locally.
  useEffect(() => {
    const interval = window.setInterval(load, 5 * 60 * 1000);
    return () => window.clearInterval(interval);
  }, []);

  const handleTrainingNeedsRefresh = useCallback((connectionId: string) => {
    getCharacter(connectionId)
      .then((detail) => {
        setState((previous) => {
          if (previous.status !== "ready") return previous;
          return {
            status: "ready",
            entries: previous.entries.map((entry) => (entry.connectionId === connectionId ? detail : entry)),
          };
        });
      })
      .catch(() => {
        // Best-effort background reconciliation -- a failure here just
        // leaves the locally-derived projection in place until the next
        // background poll or completion boundary retries it.
      });
  }, []);

  async function connectCharacter() {
    setConnecting(true);
    setConnectError("");
    try {
      const started = await beginAuthorization();
      if (started.fixtureMode) {
        load();
      } else {
        window.location.assign(started.authorizationUrl);
      }
    } catch (error) {
      setConnectError(error instanceof ApiError ? error.message : "Couldn't start EVE authorization.");
    } finally {
      setConnecting(false);
    }
  }

  const entries = state.status === "ready" ? state.entries : [];
  const needsAttentionCount = useMemo(
    () => entries.filter((entry) => entry.health !== "healthy").length,
    [entries],
  );
  const visible = useMemo(
    () => sortEntries(entries.filter((entry) => matchesFilter(entry, filter) && matchesSearch(entry, search)), sort),
    [entries, filter, search, sort],
  );

  return (
    <div>
      <PageHeader eyebrow="Characters" title="Characters">
        Every connected EVE character, at a glance.
      </PageHeader>

      {state.status === "loading" ? <LoadingState>Loading characters…</LoadingState> : null}
      {state.status === "error" ? <InlineAlert title="Couldn't load characters">{state.message}</InlineAlert> : null}

      {state.status === "ready" ? (
        <>
          <div className="mb-3 flex items-center justify-between gap-2 text-xs text-muted">
            <span>
              {`${entries.length} character${entries.length === 1 ? "" : "s"}`}
              {needsAttentionCount > 0 ? (
                <>
                  {" · "}
                  <span className="text-warning">{needsAttentionCount} need attention</span>
                </>
              ) : null}
            </span>
            <button
              className="iw-button-primary inline-flex items-center gap-1.5"
              disabled={connecting}
              onClick={() => void connectCharacter()}
              type="button"
            >
              <UserPlus aria-hidden="true" className="h-3.5 w-3.5" />
              {connecting ? "Connecting…" : "Connect Character"}
            </button>
          </div>

          {connectError ? <InlineAlert title="Couldn't connect a character">{connectError}</InlineAlert> : null}

          <div className="mb-3 flex flex-wrap items-center gap-2 border-b border-border pb-3">
            <div className="relative min-w-48 max-w-md flex-1">
              <Search aria-hidden="true" className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
              <input
                aria-label="Search characters"
                className="iw-input min-h-7 w-full py-1 pl-7 text-xs"
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search characters…"
                type="text"
                value={search}
              />
            </div>
            {filterOptions.map((option) => (
              <button
                aria-pressed={filter === option.value}
                className={filter === option.value ? "iw-button-primary" : "iw-button-secondary"}
                key={option.value}
                onClick={() => setFilter(option.value)}
                type="button"
              >
                {option.label}
              </button>
            ))}
            <select
              aria-label="Sort characters"
              className="iw-input ml-auto w-40"
              onChange={(event) => setSort(event.target.value as RosterSort)}
              value={sort}
            >
              <option value="name">Sort: Name</option>
              <option value="walletBalance">Sort: Wallet</option>
              <option value="totalSp">Sort: Skill points</option>
            </select>
          </div>

          {entries.length === 0 ? (
            <EmptyState
              action={
                <button
                  className="iw-button-primary inline-flex items-center gap-1.5"
                  disabled={connecting}
                  onClick={() => void connectCharacter()}
                  type="button"
                >
                  <UserPlus aria-hidden="true" className="h-3.5 w-3.5" />
                  {connecting ? "Connecting…" : "Connect EVE Character"}
                </button>
              }
              title="No connected characters"
            >
              Connect an EVE character to start tracking industry, assets, and skills.
            </EmptyState>
          ) : visible.length === 0 ? (
            <EmptyState title="No characters match">Try a different search or filter.</EmptyState>
          ) : (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
              {visible.map((entry) => (
                <CharacterCard
                  entry={entry}
                  key={entry.connectionId}
                  onOpen={setOpenConnectionId}
                  onTrainingNeedsRefresh={handleTrainingNeedsRefresh}
                />
              ))}
            </div>
          )}
        </>
      ) : null}

      {openConnectionId ? (
        <CharacterInspector
          connectionId={openConnectionId}
          onChanged={load}
          onClose={() => setOpenConnectionId(null)}
        />
      ) : null}
    </div>
  );
}
