-- A host record (ADR 0070), such as Rex's program status, is a state source too. SQLite cannot
-- change a column check, so `agent` is rebuilt with the same columns. `migrate` runs before
-- `open` turns foreign keys on, and the keys of `message` and `job` name `agent` by name, so they
-- hold after the rename.
CREATE TABLE agent_0013 (
    id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES session(id),
    role TEXT NOT NULL,
    pane_id TEXT,
    provider TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    state TEXT CHECK (state IN ('working', 'waiting', 'done', 'failed')),
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
    state_kind TEXT CHECK (state_kind IN ('permission','question')) CHECK (state_kind IS NULL OR state = 'waiting'),
    sent_at INTEGER,
    PRIMARY KEY (session_id, id)
);
INSERT INTO agent_0013 (id, session_id, role, pane_id, provider, created_at, state, state_at,
                        state_source, state_detail, log, profile, runner, model, effort, account,
                        cost_usd, tokens, usage_state, state_kind, sent_at)
    SELECT id, session_id, role, pane_id, provider, created_at, state, state_at, state_source,
           state_detail, log, profile, runner, model, effort, account, cost_usd, tokens,
           usage_state, state_kind, sent_at
    FROM agent;
DROP TABLE agent;
ALTER TABLE agent_0013 RENAME TO agent;
