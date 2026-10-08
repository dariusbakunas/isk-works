import { Fragment, type ReactNode } from "react";
import { Link } from "react-router";

import { CCP_PROPRIETARY_NOTICE, HELP_PATH, LEGAL_PATH, communityLinks } from "./community-links";

const LINK_CLASS = "hover:text-foreground hover:underline";

/**
 * Site footer: the operator's community link and donation note (each only when
 * configured), Help, the About & Legal page, source code, and the CCP
 * proprietary notice (Developer License Agreement section 7.1) with a
 * non-affiliation line.
 */
export function AppFooter({ className = "" }: { className?: string }) {
  const { support, donationCharacter, sourceUrl } = communityLinks();
  const items: { key: string; node: ReactNode }[] = [
    ...(support
      ? [
          {
            key: "support",
            node: (
              <a className={LINK_CLASS} href={support.url} rel="noopener noreferrer" target="_blank">
                {support.label}
              </a>
            ),
          },
        ]
      : []),
    {
      key: "help",
      node: (
        <Link className={LINK_CLASS} to={HELP_PATH}>
          Help
        </Link>
      ),
    },
    {
      key: "legal",
      node: (
        <Link className={LINK_CLASS} to={LEGAL_PATH}>
          About, privacy &amp; legal
        </Link>
      ),
    },
    {
      key: "source",
      node: (
        <a className={LINK_CLASS} href={sourceUrl} rel="noopener noreferrer" target="_blank">
          Source code
        </a>
      ),
    },
    ...(donationCharacter
      ? [
          {
            key: "donation",
            node: (
              <span>
                Enjoying ISK Works? ISK donations to{" "}
                <strong className="font-semibold text-foreground">{donationCharacter}</strong> in-game are appreciated o7
              </span>
            ),
          },
        ]
      : []),
  ];

  return (
    <footer
      className={`flex flex-wrap items-center justify-center gap-x-3 gap-y-1 px-3 py-2 text-center text-[0.6875rem] leading-snug text-muted ${className}`}
    >
      {items.map(({ key, node }, index) => (
        <Fragment key={key}>
          {index > 0 ? <span aria-hidden="true">·</span> : null}
          {node}
        </Fragment>
      ))}
      <span className="basis-full">
        {CCP_PROPRIETARY_NOTICE} ISK Works is not affiliated with or endorsed by CCP hf. (Fenris Creations).
      </span>
    </footer>
  );
}
