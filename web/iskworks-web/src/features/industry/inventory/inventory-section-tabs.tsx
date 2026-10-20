import { NavLink } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Inventory is the stock itself plus Runway: what committed Epics still have
// to buy against it. `end` keeps Stock from matching /inventory/runway, and
// Stock stays at /inventory so existing `?item=` links keep working.
export function InventorySectionTabs() {
  return (
    <nav aria-label="Inventory views" className="mb-3 flex items-stretch border-b border-border">
      <NavLink className={tabClass} end to="/inventory">Stock</NavLink>
      <NavLink className={tabClass} to="/inventory/runway">Runway</NavLink>
    </nav>
  );
}
