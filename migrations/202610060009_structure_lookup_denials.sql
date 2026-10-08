-- Structures a character recently could not look up
-- (docs/security/2026-10-06-esi-usage-audit.md, finding 4). ESI answers
-- `/universe/structures/{id}` with 403 when the character has no docking
-- access and 404 when the structure is gone; both spend ESI's error budget.
-- Asset sync used to ask again on every pass, for every character. A denial
-- holds until `denied_until`; access is per character, so it is keyed by
-- connection.

CREATE TABLE esi_structure_lookup_denials (
  connection_id uuid NOT NULL REFERENCES eve_connections(id) ON DELETE CASCADE,
  structure_id bigint NOT NULL CHECK (structure_id > 0),
  denied_until timestamptz NOT NULL,
  PRIMARY KEY (connection_id, structure_id)
);
