-- Sequence numbers belong to sessions, while existing body paths must keep pointing to their files.
CREATE TEMP TABLE message_seq_map AS
SELECT seq AS old_seq, session_id,
       ROW_NUMBER() OVER (PARTITION BY session_id ORDER BY created_at, seq) AS new_seq
FROM message;

CREATE TABLE message_new (
    session_id INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    sender_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    body_path TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    rung_at INTEGER,
    seen_at INTEGER,
    PRIMARY KEY (session_id, seq),
    FOREIGN KEY (session_id, sender_id) REFERENCES agent(session_id, id),
    FOREIGN KEY (session_id, recipient_id) REFERENCES agent(session_id, id),
    UNIQUE (session_id, seq, recipient_id)
);
INSERT INTO message_new (session_id, seq, sender_id, recipient_id, kind, body_path, created_at, rung_at, seen_at)
SELECT message.session_id, message_seq_map.new_seq, sender_id, recipient_id, kind, body_path, created_at, rung_at, seen_at
FROM message JOIN message_seq_map ON message.seq = message_seq_map.old_seq;

CREATE TABLE read_mark_new (
    session_id INTEGER NOT NULL,
    message_seq INTEGER NOT NULL,
    agent_id TEXT NOT NULL,
    PRIMARY KEY (session_id, message_seq, agent_id),
    FOREIGN KEY (session_id, message_seq, agent_id)
        REFERENCES message_new(session_id, seq, recipient_id)
);
INSERT INTO read_mark_new (session_id, message_seq, agent_id)
SELECT message_seq_map.session_id, message_seq_map.new_seq, read_mark.agent_id
FROM read_mark JOIN message_seq_map ON read_mark.message_seq = message_seq_map.old_seq;

DROP TABLE read_mark;
DROP TABLE message;
ALTER TABLE message_new RENAME TO message;
ALTER TABLE read_mark_new RENAME TO read_mark;
DROP TABLE message_seq_map;
