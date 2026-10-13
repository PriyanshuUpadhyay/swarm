-- Each item swarm added to a file outside its home (ADR 0042). `id` is a hash of the place, so a
-- writer that adds the same place again updates its row. `before` NULL means the place was absent.
-- `off` 1 means swarm removed the item; the row stays so the app can show it as off.
CREATE TABLE managed_edit (
    id      TEXT PRIMARY KEY,
    writer  TEXT NOT NULL,
    file    TEXT NOT NULL,
    kind    TEXT NOT NULL,
    path    TEXT NOT NULL,
    wrote   TEXT NOT NULL,
    before  TEXT,
    created INTEGER NOT NULL,
    with_id TEXT,
    at_s    INTEGER NOT NULL,
    off     INTEGER NOT NULL DEFAULT 0 CHECK (off IN (0, 1))
);
