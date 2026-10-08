-- The Batch entity (bundling multiple committed Plans under one name) is
-- superseded by the Board's own grouping (ACQ GROUP bands today, with
-- manufacturing-ticket grouping planned) -- removing it rather than
-- leaving a second, redundant grouping concept alongside the Board.
DROP TABLE batch_plans;
DROP TABLE batches;
