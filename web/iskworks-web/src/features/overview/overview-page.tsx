import { Users } from "lucide-react";

import { ButtonLink, InlineAlert } from "../../components/primitives";
import { OverviewDataProvider, useOverviewData } from "./overview-data";

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
  const { sources, hasCharacters } = useOverviewData();
  const characters = sources.characters;
  return (
    <>
      <header className="mb-3 flex items-baseline gap-2">
        <h1 className="iw-title">Overview</h1>
      </header>
      {characters.error ? (
        <InlineAlert title="Couldn't load your characters">{characters.errorMessage}</InlineAlert>
      ) : hasCharacters === false ? (
        <EmptyWorkspace />
      ) : null}
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
