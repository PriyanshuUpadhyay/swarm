CREATE TABLE agent_new (
    id TEXT NOT NULL,
    session_id INTEGER NOT NULL REFERENCES session(id),
    role TEXT NOT NULL,
    pane_id TEXT,
    PRIMARY KEY (session_id, id)
);
INSERT INTO agent_new SELECT id, session_id, role, pane_id FROM agent;
DROP TABLE agent;
ALTER TABLE agent_new RENAME TO agent;

CREATE TABLE job_new (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL,
    agent_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'running', 'done', 'parked')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    run_after INTEGER NOT NULL DEFAULT 0 CHECK (run_after >= 0),
    FOREIGN KEY (session_id, agent_id) REFERENCES agent(session_id, id)
);
INSERT INTO job_new (id, session_id, agent_id, kind, state, attempts, run_after)
SELECT job.id, agent.session_id, job.agent_id, job.kind, job.state, job.attempts, job.run_after
FROM job JOIN agent ON agent.id = job.agent_id;
DROP TABLE job;
ALTER TABLE job_new RENAME TO job;
