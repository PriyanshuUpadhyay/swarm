ALTER TABLE session ADD COLUMN continuation_of TEXT REFERENCES session(id);
CREATE INDEX session_continuation ON session(continuation_of);
