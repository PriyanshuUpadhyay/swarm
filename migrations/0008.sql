-- Claude usage state keeps offsets, token totals and recent message counts for each log (ADR 0061).
ALTER TABLE agent ADD COLUMN usage_state TEXT;
