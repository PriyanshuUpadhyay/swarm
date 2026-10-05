-- What the last ring of a message proved (ADR 0041); NULL until a ring ends.
ALTER TABLE message ADD COLUMN delivery TEXT
    CHECK (delivery IN ('hook', 'screen', 'seen', 'unconfirmed', 'unchecked'));

-- An older build checked no ring, and its pane has moved on since, so no later pass can settle it.
UPDATE message SET delivery = 'unchecked' WHERE rings > 0;

-- A stall or lost-ring report reaches the chair once: the kind names the message it is about.
-- `swarm send` takes any kind, so an older db can hold two hand-sent messages of one such kind.
-- A unique index would refuse them and the migration with them, so a trigger checks new rows only.
-- Every insert runs in an immediate transaction, so two passes cannot both pass the check.
CREATE TRIGGER message_report BEFORE INSERT ON message
    WHEN NEW.kind GLOB 'stall:*' OR NEW.kind GLOB 'unconfirmed:*'
BEGIN
    SELECT RAISE(ABORT, 'report already sent')
    WHERE EXISTS (SELECT 1 FROM message
                  WHERE session_id = NEW.session_id AND sender_id = NEW.sender_id
                    AND kind = NEW.kind);
END;
