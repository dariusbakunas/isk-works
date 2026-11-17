import { RefreshCw, Users } from "lucide-react";

import { ButtonLink, InlineAlert } from "../../components/primitives";
import { ago } from "../jobs/jobs-format";
import { useNow } from "../jobs/use-jobs";
import { AgendaPanel } from "./agenda-panel";
import { AttentionPanel } from "./attention-panel";
import { CharactersStrip } from "./characters-strip";
import { EpicsPanel } from "./epics-panel";
import { IndustryPanel } from "./industry-panel";
import { MarketPanel } from "./market-panel";
import { MoneyPanel } from "./money-panel";
import { OverviewDataProvider, useOverviewData } from "./overview-data";
import { PlanetaryPanel } from "./planetary-panel";

// Overview: the home page. One read-only screen of what needs attention,
// what's coming up and how the account is doing; each panel links to the
// page it summarizes (spec 2026-11-17-dashboard-design).
export function OverviewPage() {
  return (
    <OverviewDataProvider>
      <Overview />
    </OverviewDataProvider>
  );
}

function Overview() {
  const { sources, hasCharacters, reloadAll, updatedAt } = useOverviewData();
  const nowMs = useNow();
  const characters = sources.characters;
  const refreshing = Object.values(sources).some((source) => source.refreshing);

  return (
    <>
      <header className="mb-3 flex items-center gap-2">
        <h1 className="iw-title">Overview</h1>
        {updatedAt !== null ? <span className="text-xs text-muted">Updated {ago(new Date(updatedAt).toISOString(), nowMs)}</span> : null}
        {hasCharacters ? (
          <button
            aria-label="Refresh overview"
            className="iw-button-secondary ml-auto h-8 w-8 justify-center p-0"
            disabled={refreshing}
            onClick={reloadAll}
            title="Refresh overview"
            type="button"
          >
            <RefreshCw aria-hidden="true" className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`} />
          </button>
        ) : null}
      </header>
      {characters.error && !characters.data ? (
        <InlineAlert title="Couldn't load your characters">{characters.errorMessage}</InlineAlert>
      ) : hasCharacters === false ? (
        <EmptyWorkspace />
      ) : (
        // While the roster loads (hasCharacters null), every panel shows
        // its loading state rather than a blank page.
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2 xl:grid-cols-12">
          <AttentionPanel className="lg:col-span-2 xl:col-span-12" />
          <AgendaPanel className="xl:col-span-4 xl:row-span-2" />
          <IndustryPanel className="xl:col-span-8" />
          <MarketPanel className="xl:col-span-4" />
          <MoneyPanel className="xl:col-span-4" />
          <EpicsPanel className="xl:col-span-6" />
          <PlanetaryPanel className="xl:col-span-6" />
          <CharactersStrip className="lg:col-span-2 xl:col-span-12" />
        </div>
      )}
    </>
  );
}

function EmptyWorkspace() {
  return (
    <section className="mx-auto mt-12 max-w-lg rounded-md border border-border bg-panel p-6">
      <span aria-hidden="true" className="grid h-10 w-10 place-items-center rounded-md border border-primary/50 bg-primary/10 text-primary">
        <Users className="h-5 w-5" />
      </span>
      <h2 className="mt-4 text-base font-semibold text-foreground">Connect your first character</h2>
      <p className="iw-muted mt-1">The Overview fills in once ISK Works can read a character from EVE.</p>
      <div className="mt-4">
        <ButtonLink to="/characters" variant="primary">Connect character</ButtonLink>
      </div>
      <ul className="mt-5 grid gap-1.5 border-t border-border pt-4 text-xs text-muted">
        <li>Plan builds and their material needs</li>
        <li>Track industry jobs and market orders</li>
        <li>Run planetary production across characters</li>
      </ul>
    </section>
  );
}
