-- What the last ring of a message proved (ADR 0041); NULL until a ring ends.
ALTER TABLE message ADD COLUMN delivery TEXT
    CHECK (delivery IN ('hook', 'screen', 'unconfirmed', 'unchecked'));

-- A stall or lost-ring report reaches the chair once: the kind names the message it is about.
CREATE UNIQUE INDEX message_report ON message (session_id, sender_id, kind)
    WHERE kind GLOB 'stall:*' OR kind GLOB 'unconfirmed:*';
