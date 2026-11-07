import { useEffect, useRef, useState } from "react";

import { getHealth, getMaintenanceNotice, type MaintenanceNotice } from "../api/health";

const NOTICE_POLL_MS = 60_000;
const CLOCK_TICK_MS = 15_000;

/**
 * The banners above the app shell: the "this is not production" marker and the
 * maintenance notice. They read public endpoints, so they also appear on the
 * sign-in screen, before any session exists.
 *
 * They stack in one wrapper that publishes its total height as --iw-banner-h;
 * styles.css turns that into --iw-viewport-h, which every viewport-height layout
 * uses instead of 100vh so the footer stays in view.
 */
export function SiteBanners() {
  const label = useEnvironmentLabel();
  const notice = useMaintenanceNotice();
  const wrapperRef = useRef<HTMLDivElement>(null);
  const hasBanner = Boolean(label || notice);

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
    const element = wrapperRef.current;
    if (!hasBanner || !element) return;
    const root = document.documentElement;
    const publish = () => root.style.setProperty("--iw-banner-h", `${element.offsetHeight}px`);
    publish();
    // A banner can wrap onto a second line on very narrow screens.
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    observer?.observe(element);
    return () => {
      observer?.disconnect();
      root.style.removeProperty("--iw-banner-h");
    };
    // `notice` and `label`: without ResizeObserver, re-measure when the content changes.
  }, [hasBanner, label, notice]);

  if (!hasBanner) return null;

  return (
    <>
      <div className="sticky top-0 z-50" ref={wrapperRef}>
        {label ? <EnvironmentBar label={label} /> : null}
        {notice ? <MaintenanceBar notice={notice} /> : null}
      </div>
      {/* Frame the whole viewport so the marker stays visible after scrolling. */}
      {label ? (
        <div aria-hidden="true" className="pointer-events-none fixed inset-0 z-50 border-4 border-warning" />
      ) : null}
    </>
  );
}

function useEnvironmentLabel() {
  const [label, setLabel] = useState<string | null>(null);

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

  return label;
}

/**
 * Polls, because the people who most need the warning are in tabs that are
 * already open. A failed check keeps the last known notice.
 */
function useMaintenanceNotice() {
  const [notice, setNotice] = useState<MaintenanceNotice | null>(null);

  useEffect(() => {
    let active = true;
    const load = () => {
      getMaintenanceNotice()
        .then((latest) => {
          if (active) setNotice(latest);
        })
        .catch(() => {
          /* keep what we have; the start time still hides it */
        });
    };
    load();
    const timer = window.setInterval(load, NOTICE_POLL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  // Between polls, hide the notice as soon as its start time passes.
  const now = useClock(Boolean(notice));
  return notice && new Date(notice.startsAt).getTime() > now ? notice : null;
}

function useClock(running: boolean) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), CLOCK_TICK_MS);
    return () => window.clearInterval(timer);
  }, [running]);

  return now;
}

function EnvironmentBar({ label }: { label: string }) {
  return (
    <div
      className="iw-environment-banner flex min-h-6 items-center justify-center px-2 font-mono text-[11px] font-bold uppercase tracking-widest text-black"
      role="status"
    >
      <span className="rounded-sm bg-warning px-2">
        {label} environment
        <span className="hidden font-normal sm:inline"> — not production</span>
      </span>
    </div>
  );
}

function MaintenanceBar({ notice }: { notice: MaintenanceNotice }) {
  const now = useClock(true);
  const startsAt = new Date(notice.startsAt);

  return (
    <div
      className="iw-environment-banner flex min-h-6 items-center justify-center px-2 py-0.5 text-xs font-semibold text-black"
      role="status"
    >
      <span className="rounded-sm bg-warning px-2 text-center">
        Maintenance starts at {formatStart(startsAt, now)} ({formatLead(startsAt.getTime() - now)}).{" "}
        {notice.message}
      </span>
    </div>
  );
}

function formatStart(startsAt: Date, now: number) {
  const time = startsAt.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (startsAt.toDateString() === new Date(now).toDateString()) return time;
  return `${startsAt.toLocaleDateString([], { month: "short", day: "numeric" })}, ${time}`;
}

export function formatLead(remainingMs: number) {
  const minutes = Math.round(remainingMs / 60_000);
  if (minutes < 1) return "in less than a minute";
  if (minutes === 1) return "in about a minute";
  if (minutes < 90) return `in about ${minutes} minutes`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `in about ${hours} hours`;
  return `in about ${Math.round(hours / 24)} days`;
}
