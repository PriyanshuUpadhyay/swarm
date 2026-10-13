ALTER TABLE agent ADD COLUMN state_kind TEXT CHECK (state_kind IN ('permission','question')) CHECK (state_kind IS NULL OR state = 'waiting');
