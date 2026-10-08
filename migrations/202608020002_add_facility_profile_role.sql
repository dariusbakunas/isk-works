-- Which production activity a facility profile can run (manufacturing vs.
-- reaction). Every profile that existed before reactions were modeled was a
-- manufacturing facility, so the default preserves current behavior exactly.
ALTER TABLE industry_facility_profiles
  ADD COLUMN role text NOT NULL DEFAULT 'manufacturing'
    CHECK (role IN ('manufacturing', 'reaction'));

ALTER TABLE industry_facility_profiles
  ALTER COLUMN role DROP DEFAULT;
