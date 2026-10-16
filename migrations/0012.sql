-- When the owner last sent the agent a prompt; the listing reads it as working until the agent reports.
ALTER TABLE agent ADD COLUMN sent_at INTEGER;
