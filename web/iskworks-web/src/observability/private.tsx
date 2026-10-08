import type { ElementType, ReactNode } from "react";

/**
 * DOM privacy markers for LogRocket session replay.
 *
 * LogRocket redacts the text and input values of any element carrying the
 * `data-private` attribute (and its whole subtree). This module is the one
 * place that knows the attribute name, so components just render
 * `<Private>` / `<CharacterName>` or spread {...PRIVATE_ATTR}.
 *
 * See docs/security/session-replay-privacy.md for what to mark and why.
 */

/** Spread onto any element to keep its contents out of session replay. */
export const PRIVATE_ATTR = { "data-private": "" } as const;

type PrivateProps = {
  children: ReactNode;
  /** Element/component to render as. Defaults to <span>. */
  as?: ElementType;
  className?: string;
  /** Remove from replay entirely rather than mask in place. */
  hard?: boolean;
} & Record<string, unknown>;

/**
 * Wrap any rendered value that is free-form user text, character identity,
 * or a sensitive financial figure so it is not captured in replay.
 */
export function Private({ children, as, className, hard = false, ...rest }: PrivateProps) {
  const Tag = as ?? "span";
  return (
    <Tag className={className} data-private={hard ? "delete" : ""} {...rest}>
      {children}
    </Tag>
  );
}

/**
 * An EVE character name. Pseudonymous but account-linked, so masked from
 * replay during the alpha. Renders a plain inline <span> otherwise.
 *
 * No `title` passthrough on purpose: a title attribute would leak the name
 * even while the text node is masked.
 */
export function CharacterName({ name, className }: { name: string; className?: string }) {
  return (
    <span className={className} data-private="">
      {name}
    </span>
  );
}
