-- One append-only list of typed posts per project (ADR 0115). The project id is the one the app's
-- project list uses, so all worktrees of a repository share one board.
CREATE TABLE post (
    project_id TEXT NOT NULL,
    n INTEGER NOT NULL CHECK (n >= 1),
    kind TEXT NOT NULL
        CHECK (kind IN ('ask', 'answer', 'finding', 'decision', 'claim', 'handoff')),
    session_id TEXT NOT NULL,
    author TEXT NOT NULL,
    author_role TEXT NOT NULL,
    to_session TEXT,
    to_agent TEXT,
    reply_to INTEGER,
    supersedes INTEGER,
    work_key TEXT,
    generation INTEGER,
    depth INTEGER NOT NULL DEFAULT 0 CHECK (depth >= 0),
    needs_owner INTEGER NOT NULL DEFAULT 0 CHECK (needs_owner IN (0, 1)),
    relayed INTEGER NOT NULL DEFAULT 0 CHECK (relayed IN (0, 1)),
    sources TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL,
    body_path TEXT NOT NULL,
    idem TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (project_id, n),
    UNIQUE (project_id, supersedes),
    UNIQUE (project_id, idem),
    CHECK ((to_session IS NULL) = (to_agent IS NULL)),
    FOREIGN KEY (session_id, author) REFERENCES agent(session_id, id),
    FOREIGN KEY (to_session, to_agent) REFERENCES agent(session_id, id),
    FOREIGN KEY (project_id, reply_to) REFERENCES post(project_id, n),
    FOREIGN KEY (project_id, supersedes) REFERENCES post(project_id, n)
);
CREATE INDEX post_reply ON post (project_id, reply_to);
CREATE INDEX post_work ON post (project_id, work_key);
CREATE INDEX post_author ON post (session_id, author);
CREATE INDEX post_to ON post (to_session, to_agent);

CREATE TRIGGER post_no_update BEFORE UPDATE ON post
BEGIN
    SELECT RAISE(ABORT, 'posts are append-only; post a new one with --supersedes');
END;
CREATE TRIGGER post_no_delete BEFORE DELETE ON post
BEGIN
    SELECT RAISE(ABORT, 'posts are append-only');
END;

-- The one changing record of a work key. The posts keep its history. A new holder gets the next
-- generation, so a holder whose lease ran out cannot hand off work that another seat now holds.
CREATE TABLE claim (
    project_id TEXT NOT NULL,
    work_key TEXT NOT NULL,
    session_id TEXT NOT NULL,
    holder TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK (generation >= 1),
    attempts INTEGER NOT NULL CHECK (attempts BETWEEN 1 AND 3),
    lease_until INTEGER NOT NULL,
    released_at INTEGER,
    PRIMARY KEY (project_id, work_key),
    FOREIGN KEY (session_id, holder) REFERENCES agent(session_id, id)
);
CREATE INDEX claim_holder ON claim (session_id, holder);
