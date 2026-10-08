import { useEffect, useRef, useState } from "react";

import { getHealth } from "../api/health";

/**
 * Loud "this is not production" marker. Shown only when the API reports an
 * `environmentLabel` (`ISKWORKS_ENVIRONMENT_LABEL`), so production renders
 * nothing. Reads the public `/api/health` endpoint so it also appears on the
 * sign-in screen, before any session exists. Also prefixes the tab title so
 * stage and prod tabs are distinguishable at a glance.
 *
 * The banner sits above the app shell, so it publishes its height as
 * --iw-banner-h; styles.css turns that into --iw-viewport-h, which every
 * viewport-height layout uses instead of 100vh so the footer stays in view.
 */
export function EnvironmentBanner() {
  const [label, setLabel] = useState<string | null>(null);
  const bannerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let active = true;
    getHealth()
      .then((health) => {
        if (active) setLabel(health.environmentLabel?.trim() || null);
      })
      .catch(() => {
        /* no label is the safe default; the app surfaces API outages itself */
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!label) return;
    const prefix = `[${label.toUpperCase()}] `;
    const original = document.title;
    document.title = prefix + original;
    return () => {
      document.title = original;
    };
  }, [label]);

  useEffect(() => {
    const element = bannerRef.current;
    if (!label || !element) return;
    const root = document.documentElement;
    const publish = () => root.style.setProperty("--iw-banner-h", `${element.offsetHeight}px`);
    publish();
    // The label wraps onto a second line on very narrow screens.
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    observer?.observe(element);
    return () => {
      observer?.disconnect();
      root.style.removeProperty("--iw-banner-h");
    };
  }, [label]);

  if (!label) return null;

  return (
    <>
      <div
        ref={bannerRef}
        className="iw-environment-banner sticky top-0 z-50 flex min-h-6 items-center justify-center px-2 font-mono text-[11px] font-bold uppercase tracking-widest text-black"
        role="status"
      >
        <span className="rounded-sm bg-warning px-2">
          {label} environment
          <span className="hidden font-normal sm:inline"> — not production</span>
        </span>
      </div>
      {/* Frame the whole viewport so the marker stays visible after scrolling. */}
      <div aria-hidden="true" className="pointer-events-none fixed inset-0 z-50 border-4 border-warning" />
    </>
  );
}
