-- Rename model aliases to model groups across the durable schema.

ALTER TABLE aliases RENAME TO model_groups;
ALTER TABLE model_groups RENAME COLUMN alias TO name;
ALTER TABLE request_log RENAME COLUMN resolved_alias TO resolved_model_group;
