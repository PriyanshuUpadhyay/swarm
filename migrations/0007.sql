-- Runner data survives app restarts; NULL usage means no provider source (ADR 0053).
ALTER TABLE agent ADD COLUMN profile TEXT;
ALTER TABLE agent ADD COLUMN runner TEXT;
ALTER TABLE agent ADD COLUMN model TEXT;
ALTER TABLE agent ADD COLUMN effort TEXT;
ALTER TABLE agent ADD COLUMN account TEXT;
ALTER TABLE agent ADD COLUMN cost_usd REAL;
ALTER TABLE agent ADD COLUMN tokens INTEGER;
