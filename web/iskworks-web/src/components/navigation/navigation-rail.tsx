import { CircleHelp, PanelLeftClose, PanelLeftOpen, Search, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Link, useLocation } from "react-router";

import mascotHead from "../../assets/mascot-head.png";
import { HELP_PATH } from "../community-links";
import { EsiStatusItem } from "../esi-status";
import { adminNavItem, isNavPathActive, navGroups } from "./registry";

export const RAIL_COLLAPSED_WIDTH = 52;
export const RAIL_EXPANDED_WIDTH = 220;

const rowFocus = "focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary";

/** Text that shows when the rail is expanded and stays readable to screen readers when it isn't. */
function RailLabel({ expanded, className = "", children }: { expanded: boolean; className?: string; children: ReactNode }) {
  return <span className={expanded ? `truncate ${className}` : "sr-only"}>{children}</span>;
}

function RailLink({ to, label, icon: Icon, expanded, active }: { to: string; label: string; icon: LucideIcon; expanded: boolean; active: boolean }) {
  return (
    <Link
      aria-current={active ? "page" : undefined}
      className={`relative flex h-9 w-full items-center hover:bg-hover hover:text-foreground ${rowFocus} ${
        active ? "bg-hover text-primary" : "text-muted"
      }`}
      to={to}
    >
      {active ? <span aria-hidden="true" className="absolute inset-y-0 left-0 w-0.5 bg-primary" /> : null}
      <span className="grid w-[51px] shrink-0 place-items-center">
        <Icon aria-hidden="true" className="h-[17px] w-[17px]" strokeWidth={1.7} />
      </span>
      <RailLabel className={`text-[11px] ${active ? "text-bright" : "text-foreground"}`} expanded={expanded}>
        {label}
      </RailLabel>
    </Link>
  );
}

export interface RailContentProps {
  expanded: boolean;
  isAdmin: boolean;
  workspaceName: string;
  /** Rendered between Help and the pin toggle (the account menu, or the API version). */
  account?: ReactNode;
  onOpenSearch?: () => void;
  /** Omitted in the mobile drawer, which can't be pinned. */
  pin?: { pinned: boolean; onToggle: () => void };
}

/** The rail's contents, shared by the desktop rail and the mobile drawer. */
export function RailContent({ expanded, isAdmin, workspaceName, account, onOpenSearch, pin }: RailContentProps) {
  const { pathname } = useLocation();

  return (
    <>
      <Link className={`flex h-[52px] shrink-0 items-center overflow-hidden ${rowFocus}`} to="/characters">
        <span className="grid w-[51px] shrink-0 place-items-center">
          <img alt="" aria-hidden="true" className="h-7 w-7" src={mascotHead} />
        </span>
        <span className={expanded ? "min-w-0 whitespace-nowrap leading-tight" : "sr-only"}>
          <strong className="block text-xs text-bright">ISK Works</strong>
          <small className="block truncate text-[9px] text-subtle">{workspaceName}</small>
        </span>
      </Link>

      {onOpenSearch ? (
        <div className="shrink-0 px-2 pb-2">
          <button
            className={`flex h-8 w-full items-center overflow-hidden border border-border bg-panel hover:bg-panel-strong ${rowFocus}`}
            onClick={onOpenSearch}
            type="button"
          >
            <span className="grid w-[34px] shrink-0 place-items-center">
              <Search aria-hidden="true" className="h-3.5 w-3.5 text-muted" />
            </span>
            <RailLabel className="text-[10px] text-subtle" expanded={expanded}>
              Search…
            </RailLabel>
            {expanded ? (
              <kbd aria-hidden="true" className="ml-auto mr-2 rounded-[2px] border border-border bg-background px-1 font-mono text-[8px] text-subtle">
                ⌘K
              </kbd>
            ) : null}
          </button>
        </div>
      ) : null}

      <nav aria-label="Primary navigation" className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
        {navGroups.map((group, index) => (
          <div className={expanded ? "pb-1" : "py-1"} key={group.id}>
            {expanded ? (
              <div className="flex h-5 items-center px-[14px] text-[8px] font-semibold uppercase tracking-[.14em] text-subtle">{group.label}</div>
            ) : index > 0 ? (
              <div aria-hidden="true" className="mx-3 my-1 border-t border-border" />
            ) : null}
            {group.items.map((item) => (
              <RailLink
                active={isNavPathActive(pathname, item.path)}
                expanded={expanded}
                icon={item.icon}
                key={item.path}
                label={item.label}
                to={item.path}
              />
            ))}
          </div>
        ))}
      </nav>

      <nav aria-label="Account and help" className="shrink-0 border-t border-border">
        {isAdmin ? (
          <RailLink
            active={isNavPathActive(pathname, adminNavItem.path)}
            expanded={expanded}
            icon={adminNavItem.icon}
            label={adminNavItem.label}
            to={adminNavItem.path}
          />
        ) : null}
        <EsiStatusItem expanded={expanded} />
        <RailLink active={false} expanded={expanded} icon={CircleHelp} label="Help" to={HELP_PATH} />
        {account}
        {pin ? (
          <button
            aria-pressed={pin.pinned}
            className={`flex h-9 w-full items-center text-muted hover:bg-hover hover:text-foreground ${rowFocus}`}
            onClick={pin.onToggle}
            title={`${pin.pinned ? "Unpin" : "Pin"} navigation  [`}
            type="button"
          >
            <span className="grid w-[51px] shrink-0 place-items-center">
              {pin.pinned ? <PanelLeftClose aria-hidden="true" className="h-4 w-4" /> : <PanelLeftOpen aria-hidden="true" className="h-4 w-4" />}
            </span>
            <RailLabel className="text-[10px] text-foreground" expanded={expanded}>
              {pin.pinned ? "Unpin navigation" : "Pin navigation"}
            </RailLabel>
          </button>
        ) : null}
      </nav>
    </>
  );
}

export interface NavigationRailProps {
  pinned: boolean;
  onTogglePinned: () => void;
  isAdmin: boolean;
  workspaceName: string;
  account?: ReactNode;
  onOpenSearch?: () => void;
}

/**
 * The desktop navigation: a 52px icon rail that can be pinned open at 220px.
 * The slot div is what the page layout sees; the rail itself fills it.
 */
export function NavigationRail({ pinned, onTogglePinned, isAdmin, workspaceName, account, onOpenSearch }: NavigationRailProps) {
  const width = pinned ? RAIL_EXPANDED_WIDTH : RAIL_COLLAPSED_WIDTH;

  return (
    <div className="relative h-full shrink-0" data-testid="nav-rail-slot" style={{ width }}>
      <aside
        aria-label="App navigation"
        className="flex h-full flex-col border-r border-border bg-sidebar transition-[width] duration-[140ms] ease-out"
        style={{ width }}
      >
        <RailContent
          account={account}
          expanded={pinned}
          isAdmin={isAdmin}
          onOpenSearch={onOpenSearch}
          pin={{ pinned, onToggle: onTogglePinned }}
          workspaceName={workspaceName}
        />
      </aside>
    </div>
  );
}
