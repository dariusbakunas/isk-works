import gettingStarted from "./getting-started.md?raw";
import selfHosting from "./self-hosting.md?raw";
import whereItRuns from "./where-it-runs.md?raw";
import whereItRunsHero from "../../../assets/help/barn-office.webp";

export type HelpSection = {
  slug: string;
  title: string;
  source: string;
  /** Optional illustration shown above the section's Markdown. */
  hero?: { src: string; alt: string };
};

// Help sections in navigation order. Add a section by writing a Markdown
// file next to this one and listing it here.
export const HELP_SECTIONS: HelpSection[] = [
  { slug: "getting-started", title: "Getting started", source: gettingStarted },
  { slug: "self-hosting", title: "Self-hosting", source: selfHosting },
  {
    slug: "where-it-runs",
    title: "Where it runs",
    source: whereItRuns,
    hero: {
      src: whereItRunsHero,
      alt: "The ISK Works beaver in a space suit, sitting in a wooden barn office between a desk of computers and a server rack tangled in orange cables, with lightning flashing outside the windows.",
    },
  },
];
