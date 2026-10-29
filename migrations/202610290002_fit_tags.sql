-- Tags group fits in the fit browser beside the hull tree. A fit holds a
-- tag once whatever its case; the API stores it in the casing the workspace
-- already uses. Tagging doesn't change a fit's updated_at.
CREATE TABLE fit_tags (
  fit_id uuid NOT NULL REFERENCES fits(id) ON DELETE CASCADE,
  tag text NOT NULL CHECK (length(tag) BETWEEN 1 AND 40 AND tag = btrim(tag))
);

CREATE UNIQUE INDEX fit_tags_fit_tag_unique ON fit_tags (fit_id, lower(tag));
