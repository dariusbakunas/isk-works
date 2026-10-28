-- "Used with": the charges in a module's charge groups.
CREATE INDEX sde_types_group_idx ON sde_types (import_id, group_id);
