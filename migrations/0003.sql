ALTER TABLE agent ADD COLUMN state TEXT CHECK (state IN ('working', 'waiting', 'done', 'failed'));
ALTER TABLE agent ADD COLUMN state_at INTEGER;
ALTER TABLE agent ADD COLUMN state_source TEXT CHECK (state_source IN ('hook', 'screen'));
ALTER TABLE agent ADD COLUMN state_detail TEXT;
