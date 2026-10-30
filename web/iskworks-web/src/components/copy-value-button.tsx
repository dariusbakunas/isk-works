import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";

// Small copy-to-clipboard button: copies `value` verbatim (callers pass the
// raw number or price, never the display-formatted text, so it pastes cleanly
// into EVE's fields) and shows a check for a moment. The click never reaches
// an enclosing row. `revealOnHover` hides it until a `group` ancestor is
// hovered or the button is focused, for dense panels where an always-on icon
// per value would be noise.
export function CopyValueButton({
  value,
  label,
  revealOnHover = false,
  children,
}: {
  value: string;
  /** Accessible name and tooltip, e.g. "Copy 1,531 Fullerides". */
  label: string;
  revealOnHover?: boolean;
  /** Optional visible text after the icon. */
  children?: React.ReactNode;
}) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const reveal = revealOnHover && !copied ? "opacity-0 group-hover:opacity-100 focus-visible:opacity-100" : "";
  return (
    <button
      aria-label={label}
      className={`inline-flex shrink-0 items-center gap-1 ${copied ? "text-positive" : "text-muted hover:text-foreground"} ${reveal}`}
      onClick={(event) => {
        event.stopPropagation();
        void navigator.clipboard?.writeText(value).then(() => setCopied(true));
      }}
      title={label}
      type="button"
    >
      {copied ? <Check aria-hidden="true" className="h-3 w-3" /> : <Copy aria-hidden="true" className="h-3 w-3" />}
      {children}
    </button>
  );
}
