import { NavLink, Navigate, Route, Routes } from "react-router";
import { PageHeader } from "../../components/primitives";
import { InvitesPage } from "./invites-page";
import { UsersPage } from "./users-page";

// Sub-sections of the app-wide Admin area. Future screens (users, ...) add
// an entry here and a <Route> below.
const sections = [
  { label: "Users", path: "/admin/users" },
  { label: "Invites", path: "/admin/invites" },
];

export function AdminPage({ isAdmin }: { isAdmin: boolean }) {
  // Convenience only: the API enforces admin access on every /api/admin/*
  // route, so a hand-typed URL gets 403s, never data.
  if (!isAdmin) return <Navigate replace to="/" />;
  return (
    <>
      <PageHeader eyebrow="Application" title="Admin">
        App-wide tools for the operator. These apply to every workspace.
      </PageHeader>
      <nav aria-label="Admin sections" className="mb-4 flex gap-1 border-b border-border">
        {sections.map((section) => (
          <NavLink
            className={({ isActive }) =>
              `border-b-2 px-3 py-2 text-sm ${
                isActive
                  ? "border-primary text-foreground"
                  : "border-transparent text-muted hover:text-foreground"
              }`
            }
            key={section.path}
            to={section.path}
          >
            {section.label}
          </NavLink>
        ))}
      </nav>
      <Routes>
        <Route path="/" element={<Navigate replace to="/admin/users" />} />
        <Route path="/users" element={<UsersPage />} />
        <Route path="/invites" element={<InvitesPage />} />
        <Route path="*" element={<Navigate replace to="/admin/users" />} />
      </Routes>
    </>
  );
}
