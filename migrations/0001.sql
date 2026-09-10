CREATE TABLE session (
    id TEXT PRIMARY KEY NOT NULL,
    talk_mode TEXT not NULL
    check (talk_mode in ('lane', 'relay', 'open'))
    );

CREATE TABLE agent (
      id TEXT PRIMARY KEY NOT NULL,
      session_id TEXT NOT NULL REFERENCES session(id),
      role TEXT NOT NULL,
      UNIQUE (session_id, id)
  );

CREATE TABLE message (
         seq INTEGER PRIMARY KEY AUTOINCREMENT,
         session_id TEXT NOT NULL,
         sender_id TEXT NOT NULL,
         recipient_id TEXT NOT NULL,
         kind TEXT NOT NULL,
         body_path TEXT NOT NULL,
         FOREIGN KEY (session_id, sender_id)
             REFERENCES agent(session_id, id),
         FOREIGN KEY (session_id, recipient_id)
                    REFERENCES agent(session_id, id),
                UNIQUE (seq, recipient_id)
            );


CREATE TABLE read_mark (
        message_seq INTEGER NOT NULL,
        agent_id TEXT NOT NULL,
        PRIMARY KEY (message_seq, agent_id),
        FOREIGN KEY (message_seq, agent_id)
            REFERENCES message(seq, recipient_id)
    );

CREATE TABLE job (
     id INTEGER PRIMARY KEY,
     agent_id TEXT NOT NULL REFERENCES agent(id),
     kind TEXT NOT NULL,
     state TEXT NOT NULL DEFAULT 'queued'
         CHECK (state IN ('queued', 'running', 'done', 'parked')),
     attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
     run_after INTEGER NOT NULL DEFAULT 0 CHECK (run_after >= 0)
 );
