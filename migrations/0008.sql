-- Claude reads resume at a byte offset; usage_message holds the latest 64 counted message ids as JSON (ADR 0061).
ALTER TABLE agent ADD COLUMN usage_offset INTEGER;
ALTER TABLE agent ADD COLUMN usage_message TEXT;
