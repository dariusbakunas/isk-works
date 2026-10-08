import type { Components } from "react-markdown";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import { Link } from "react-router";

import { communityLinks } from "../../components/community-links";

// Help pages link to the instance's own channels through placeholder hrefs,
// resolved from the operator's runtime config: `support:` is the community
// channel (plain text when none is configured), `source:` the source code.
const SUPPORT_HREF = "support:";
const SOURCE_HREF = "source:";

function resolveHref(href: string): string | null {
  if (href === SUPPORT_HREF) return communityLinks().support?.url ?? null;
  if (href === SOURCE_HREF) return communityLinks().sourceUrl;
  return href;
}

function urlTransform(url: string): string {
  return url === SUPPORT_HREF || url === SOURCE_HREF ? url : defaultUrlTransform(url);
}

// Help content is our own Markdown (src/help/*.md), styled with the app's
// tokens. Raw HTML is not rendered (react-markdown's default).
const components: Components = {
  h1: ({ children }) => <h1 className="iw-title mb-3">{children}</h1>,
  h2: ({ children }) => <h2 className="mt-6 mb-2 text-lg font-semibold text-foreground">{children}</h2>,
  h3: ({ children }) => <h3 className="mt-4 mb-1.5 text-base font-semibold text-foreground">{children}</h3>,
  p: ({ children }) => <p className="my-2 text-sm leading-relaxed">{children}</p>,
  ul: ({ children }) => <ul className="my-2 list-disc space-y-1 pl-5 text-sm leading-relaxed">{children}</ul>,
  ol: ({ children }) => <ol className="my-2 list-decimal space-y-1 pl-5 text-sm leading-relaxed">{children}</ol>,
  strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
  code: ({ children }) => (
    <code className="rounded bg-panel-strong px-1 py-0.5 font-mono text-[0.8125rem]">{children}</code>
  ),
  blockquote: ({ children }) => (
    <blockquote className="my-3 rounded-md border border-warning/40 bg-warning/10 px-3 py-1 text-sm">{children}</blockquote>
  ),
  a: ({ href: rawHref = "", children }) => {
    const href = resolveHref(rawHref);
    if (href === null) return <>{children}</>;
    return href.startsWith("/") ? (
      <Link className="text-primary underline" to={href}>
        {children}
      </Link>
    ) : (
      <a className="text-primary underline" href={href} rel="noopener noreferrer" target="_blank">
        {children}
      </a>
    );
  },
};

export function HelpMarkdown({ source }: { source: string }) {
  return <ReactMarkdown components={components} urlTransform={urlTransform}>{source}</ReactMarkdown>;
}
