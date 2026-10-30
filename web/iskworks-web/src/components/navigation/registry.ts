import {
  ArrowLeftRight,
  BarChart2,
  Boxes,
  Building2,
  CalendarDays,
  Factory,
  Globe2,
  GraduationCap,
  Hammer,
  Kanban,
  Landmark,
  Rocket,
  Shield,
  Store,
  Tags,
  Users,
  Warehouse,
  type LucideIcon,
} from "lucide-react";

/** A page under a nav item: a row under it in the expanded rail, and a palette entry. */
export interface NavChild {
  label: string;
  path: string;
  keywords?: string[];
  /** Starts something rather than showing a view ("New build"): palette only, no rail row. */
  action?: boolean;
}

export interface NavItem {
  label: string;
  path: string;
  icon: LucideIcon;
  /** Extra words the command palette matches, e.g. "wallet" for Finance. */
  keywords?: string[];
  children?: NavChild[];
}

export interface NavGroup {
  id: "pilots" | "industry" | "stock" | "trade";
  label: string;
  items: NavItem[];
}

/** A registry entry a location resolves to; a child carries its parent's icon. */
export interface NavEntry {
  label: string;
  path: string;
  icon: LucideIcon;
  parent?: NavItem;
}

// The one list of app pages: the primary nav and (later) the command palette
// read it. Characters is the alpha home surface (`/` redirects here), so it
// leads. Overview (static placeholder) and Sales (unbuilt) are deliberately
// absent for the alpha.
export const navGroups: NavGroup[] = [
  {
    id: "pilots",
    label: "Pilots",
    items: [
      { label: "Characters", path: "/characters", icon: Users, keywords: ["alts"] },
      {
        label: "Skills",
        path: "/skills",
        icon: GraduationCap,
        children: [{ label: "Plans", path: "/skills/plans" }],
      },
      { label: "Fittings", path: "/fittings", icon: Rocket, keywords: ["fits"] },
      { label: "Calendar", path: "/calendar", icon: CalendarDays, keywords: ["timers", "events"] },
    ],
  },
  {
    id: "industry",
    label: "Industry",
    items: [
      {
        label: "Builds",
        path: "/builds",
        icon: Factory,
        children: [
          { label: "New build", path: "/builds/new", action: true },
          { label: "Blueprints", path: "/builds/blueprints", keywords: ["bp", "bpo", "bpc"] },
        ],
      },
      { label: "Board", path: "/board", icon: Kanban, keywords: ["kanban", "tickets"] },
      {
        label: "Jobs",
        path: "/jobs",
        icon: Hammer,
        children: [{ label: "History", path: "/jobs/history" }],
      },
      {
        label: "Facilities",
        path: "/facilities",
        icon: Building2,
        children: [{ label: "Defaults", path: "/facilities/defaults" }],
      },
      { label: "Planetary", path: "/planetary", icon: Globe2, keywords: ["pi"] },
      { label: "Opportunities", path: "/opportunities", icon: BarChart2 },
    ],
  },
  {
    id: "stock",
    label: "Stock",
    items: [
      {
        label: "Inventory",
        path: "/inventory",
        icon: Boxes,
        children: [
          { label: "Runway", path: "/inventory/runway" },
          { label: "Consumption", path: "/inventory/consumption" },
        ],
      },
      { label: "Assets", path: "/assets", icon: Warehouse },
    ],
  },
  {
    id: "trade",
    label: "Trade",
    items: [
      { label: "Market", path: "/market", icon: Store },
      { label: "Market Orders", path: "/market-orders", icon: ArrowLeftRight, keywords: ["orders"] },
      {
        label: "Price Overrides",
        path: "/prices",
        icon: Tags,
        keywords: ["prices"],
        children: [
          { label: "Market imports", path: "/prices/imports" },
          { label: "New price source", path: "/prices/new", action: true },
        ],
      },
      {
        label: "Finance",
        path: "/finance",
        icon: Landmark,
        keywords: ["wallet", "journal"],
        children: [
          { label: "Transactions", path: "/finance/transactions" },
          { label: "Analytics", path: "/finance/analytics" },
        ],
      },
    ],
  },
];

export const adminNavItem: NavItem = { label: "Admin", path: "/admin", icon: Shield };

/** Every nav item in display order, with Admin last for admins. */
export function flatNavItems(isAdmin: boolean): NavItem[] {
  const items = navGroups.flatMap((group) => group.items);
  return isAdmin ? [...items, adminNavItem] : items;
}

/** Whole path segments only: "/market" must not light up on "/market-orders". */
export function isNavPathActive(pathname: string, path: string): boolean {
  return pathname === path || (path !== "/" && pathname.startsWith(`${path}/`));
}

/** The most specific item or child a location belongs to, or null. */
export function resolveNavEntry(pathname: string): NavEntry | null {
  let best: NavEntry | null = null;
  for (const item of flatNavItems(true)) {
    const candidates: NavEntry[] = [
      { label: item.label, path: item.path, icon: item.icon },
      ...(item.children ?? []).map((child) => ({ label: child.label, path: child.path, icon: item.icon, parent: item })),
    ];
    for (const candidate of candidates) {
      if (isNavPathActive(pathname, candidate.path) && (!best || candidate.path.length > best.path.length)) {
        best = candidate;
      }
    }
  }
  return best;
}
