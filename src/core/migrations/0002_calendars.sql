-- Additive only, with defaults, so a running instance built before this
-- migration keeps working against the migrated database.
CREATE TABLE calendars (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    color    TEXT,
    tz       TEXT,
    visible  INTEGER NOT NULL DEFAULT 1,
    position INTEGER NOT NULL DEFAULT 0
);
INSERT INTO calendars(id, name) VALUES (1, 'Personal');

-- No REFERENCES: SQLite refuses a non-NULL default on a foreign key column
-- added later. Deleting a calendar removes its items in code.
ALTER TABLE items ADD COLUMN calendar_id INTEGER NOT NULL DEFAULT 1;
-- IANA zone the date and time are written in; NULL means local time.
ALTER TABLE items ADD COLUMN tz TEXT;
