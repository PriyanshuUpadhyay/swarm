CREATE TABLE session (
    id TEXT PRIMARY KEY,
    talk_mode TEXT NOT NULL CHECK (talk_mode IN ('lane', 'relay', 'open')),
    cwd TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    chair_log TEXT,
    adapter TEXT,
    chair_provider TEXT,
    chair_id TEXT,
    archived_at INTEGER
);

CREATE TABLE agent (
    id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES session(id),
    role TEXT NOT NULL,
    pane_id TEXT,
    provider TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (session_id, id)
);

CREATE TABLE message (
    session_id TEXT NOT NULL,
    seq INTEGER NOT NULL,
    sender_id TEXT NOT NULL,
    recipient_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    body_path TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    rung_at INTEGER,
    rings INTEGER NOT NULL DEFAULT 0,
    seen_at INTEGER,
    PRIMARY KEY (session_id, seq),
    FOREIGN KEY (session_id, sender_id) REFERENCES agent(session_id, id),
    FOREIGN KEY (session_id, recipient_id) REFERENCES agent(session_id, id),
    UNIQUE (session_id, seq, recipient_id)
);

CREATE TABLE read_mark (
    session_id TEXT NOT NULL,
    message_seq INTEGER NOT NULL,
    agent_id TEXT NOT NULL,
    PRIMARY KEY (session_id, message_seq, agent_id),
    FOREIGN KEY (session_id, message_seq, agent_id)
        REFERENCES message(session_id, seq, recipient_id)
);

CREATE TABLE job (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'running', 'done', 'parked')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    run_after INTEGER NOT NULL DEFAULT 0 CHECK (run_after >= 0),
    FOREIGN KEY (session_id, agent_id) REFERENCES agent(session_id, id)
);

CREATE INDEX message_inbox ON message (session_id, recipient_id, seen_at);
