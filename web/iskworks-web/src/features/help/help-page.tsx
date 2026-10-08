import { Link, Navigate, useParams } from "react-router";

import { AppFooter } from "../../components/app-footer";
import { HELP_PATH } from "../../components/community-links";
import { HELP_SECTIONS } from "./content/sections";
import { HelpMarkdown } from "./markdown";

/**
 * Help: Markdown sections from src/help/, one per route (/help/:slug).
 * Reachable without signing in, like About & Legal.
 */
export function HelpPage() {
  const { slug } = useParams();
  const section = HELP_SECTIONS.find((candidate) => candidate.slug === slug);
  if (!section) {
    return <Navigate replace to={`${HELP_PATH}/${HELP_SECTIONS[0].slug}`} />;
  }

  return (
    <div className="min-h-[var(--iw-viewport-h)] text-foreground">
      <div className="mx-auto grid w-full max-w-5xl gap-4 px-4 py-8 lg:grid-cols-[12rem_minmax(0,1fr)]">
        <aside className="grid content-start gap-3">
          <Link className="text-xs text-muted hover:text-foreground hover:underline" to="/">
            ← Back to ISK Works
          </Link>
          <nav aria-label="Help sections">
            <p className="iw-eyebrow mb-1.5">Help</p>
            <ul className="flex flex-wrap gap-1 lg:grid">
              {HELP_SECTIONS.map((candidate) => {
                const active = candidate.slug === section.slug;
                return (
                  <li key={candidate.slug}>
                    <Link
                      aria-current={active ? "page" : undefined}
                      className={`block rounded-md px-2 py-1 text-sm ${
                        active ? "bg-panel-strong text-foreground" : "text-muted hover:bg-panel-strong hover:text-foreground"
                      }`}
                      to={`${HELP_PATH}/${candidate.slug}`}
                    >
                      {candidate.title}
                    </Link>
                  </li>
                );
              })}
            </ul>
          </nav>
        </aside>
        <main className="iw-panel min-w-0 px-4 py-4 sm:px-6">
          {section.hero ? (
            <img
              alt={section.hero.alt}
              className="mb-4 h-auto w-full rounded-md border border-border"
              src={section.hero.src}
            />
          ) : null}
          <HelpMarkdown source={section.source} />
        </main>
      </div>
      <AppFooter />
    </div>
  );
}
