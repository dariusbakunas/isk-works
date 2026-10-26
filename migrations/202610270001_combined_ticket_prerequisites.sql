-- A combined ticket keeps a copy of each member's prerequisites, tagged
-- with the Epic (`order_id`) and step (`operation_occurrence_key`) it
-- serves, so it can hold the same item more than once. Every other ticket
-- has one Epic and one step per prerequisite, so for those this is the
-- old (ticket_id, type_id) rule.
ALTER TABLE ticket_prerequisites
  DROP CONSTRAINT ticket_prerequisites_ticket_id_type_id_key;

ALTER TABLE ticket_prerequisites
  ADD CONSTRAINT ticket_prerequisites_ticket_epic_step_type_key
  UNIQUE NULLS NOT DISTINCT (ticket_id, order_id, operation_occurrence_key, type_id);
