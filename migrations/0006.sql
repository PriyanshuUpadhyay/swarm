ALTER TABLE message ADD COLUMN seen_at INTEGER;
-- Existing messages predate receipts, so a ring lost before this upgrade is not retried.
UPDATE message SET seen_at = created_at;
