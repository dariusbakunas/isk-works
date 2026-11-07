-- The advance notice of planned maintenance shown to every visitor. At most one
-- row, set and cleared from the admin page. A row whose starts_at has passed is
-- simply not shown, so a forgotten notice cannot stay up.
CREATE TABLE maintenance_notice (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    message TEXT NOT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
