CREATE TABLE message_new (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    sender_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    body_path TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (session_id, sender_id)
        REFERENCES agent(session_id, id),
    FOREIGN KEY (session_id, recipient_id)
        REFERENCES agent(session_id, id),
    UNIQUE (seq, recipient_id)
);
INSERT INTO message_new (seq, session_id, sender_id, recipient_id, kind, body_path, created_at)
SELECT seq, session_id, sender_id, recipient_id, kind, body_path, unixepoch()
FROM message;
DROP TABLE message;
ALTER TABLE message_new RENAME TO message;
