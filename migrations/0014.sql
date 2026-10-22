-- A branch is its own chat that starts from part of another's history (ADR 0114). Unlike
-- continuation_of, it never folds into the chat it names.
ALTER TABLE session ADD COLUMN branch_of TEXT REFERENCES session(id);
