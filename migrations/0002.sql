CREATE TABLE session_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    talk_mode TEXT not NULL
    check (talk_mode in ('lane', 'relay', 'open'))
    );
INSERT INTO session_new SELECT * FROM session;
DROP TABLE session;
ALTER TABLE session_new RENAME TO session;
