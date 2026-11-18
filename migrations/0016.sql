-- Agent states take the names of the Program Status Protocol that swarm relies on (ADR 0021):
-- `waiting` becomes `blocked` and `failed` becomes `error`, and a block may be of kind `auth`.
-- SQLite cannot change a column check, so `agent` is rebuilt with the same columns, as in 0013.
CREATE TABLE agent_0016 (
    id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES session(id),
    role TEXT NOT NULL,
    pane_id TEXT,
    provider TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    state TEXT CHECK (state IN ('working', 'blocked', 'done', 'error')),
    state_at INTEGER,
    state_source TEXT CHECK (state_source IN ('hook', 'host', 'screen')),
    state_detail TEXT,
    log TEXT,
    profile TEXT,
    runner TEXT,
    model TEXT,
    effort TEXT,
    account TEXT,
    cost_usd REAL,
    tokens INTEGER,
    usage_state TEXT,
    state_kind TEXT CHECK (state_kind IN ('permission', 'question', 'auth')) CHECK (state_kind IS NULL OR state = 'blocked'),
    sent_at INTEGER,
    PRIMARY KEY (session_id, id)
);
INSERT INTO agent_0016 (id, session_id, role, pane_id, provider, created_at, state, state_at,
                        state_source, state_detail, log, profile, runner, model, effort, account,
                        cost_usd, tokens, usage_state, state_kind, sent_at)
    SELECT id, session_id, role, pane_id, provider, created_at,
           CASE state WHEN 'waiting' THEN 'blocked' WHEN 'failed' THEN 'error' ELSE state END,
           state_at, state_source, state_detail, log, profile, runner, model, effort, account,
           cost_usd, tokens, usage_state, state_kind, sent_at
    FROM agent;
DROP TABLE agent;
ALTER TABLE agent_0016 RENAME TO agent;
