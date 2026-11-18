-- The HUD is the notice inbox (ADR 0147). `kind` is done, asks, failed, waiting, or test, and more
-- may come; NULL is an older row. `seen_at` NULL means the owner has not seen the row; old rows
-- count as seen, so the inbox starts empty.
ALTER TABLE notice ADD COLUMN kind TEXT;
ALTER TABLE notice ADD COLUMN seen_at INTEGER;
UPDATE notice SET seen_at = created_at;
CREATE INDEX notice_unseen ON notice (seen_at, session_id);

-- When the agent last started a turn: a change to working from a state that is not working or
-- blocked. A short turn posts no done notice.
ALTER TABLE agent ADD COLUMN turn_at INTEGER;
