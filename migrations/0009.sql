CREATE TABLE notice (
    id INTEGER PRIMARY KEY,
    session_id TEXT,
    agent_id TEXT,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    posted_at INTEGER
);
