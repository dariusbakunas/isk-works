-- Skill training dogma from the SDE's typeDogma: each skill's rank (training
-- time multiplier), the two attributes it trains from, and whether Alpha
-- clones can train it. The Skills page builds its catalogue and training
-- times from this.
CREATE TABLE sde_skills (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  rank integer NOT NULL CHECK (rank >= 1),
  primary_attribute text NOT NULL,
  secondary_attribute text NOT NULL,
  omega_only boolean NOT NULL,
  PRIMARY KEY (import_id, type_id),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  CONSTRAINT sde_skills_attributes_known CHECK (
    primary_attribute IN ('charisma', 'intelligence', 'memory', 'perception', 'willpower')
    AND secondary_attribute IN ('charisma', 'intelligence', 'memory', 'perception', 'willpower')
  )
);

-- requiredSkill1..6 of any type (skills, ships, modules); ordinal is N.
CREATE TABLE sde_type_required_skills (
  import_id uuid NOT NULL,
  type_id bigint NOT NULL,
  ordinal smallint NOT NULL CHECK (ordinal BETWEEN 1 AND 6),
  skill_type_id bigint NOT NULL,
  level smallint NOT NULL CHECK (level BETWEEN 1 AND 5),
  PRIMARY KEY (import_id, type_id, ordinal),
  FOREIGN KEY (import_id, type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE,
  FOREIGN KEY (import_id, skill_type_id)
    REFERENCES sde_types(import_id, type_id)
    ON DELETE CASCADE
);

-- "Required for" lookups: which types need a given skill.
CREATE INDEX sde_type_required_skills_skill_idx
  ON sde_type_required_skills (import_id, skill_type_id);
