CREATE TABLE boards (
    id       INTEGER PRIMARY KEY,
    name     TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE lists (
    id       INTEGER PRIMARY KEY,
    board_id INTEGER NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    kind     TEXT NOT NULL DEFAULT 'normal',
    position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE cards (
    id           INTEGER PRIMARY KEY,
    list_id      INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    notes        TEXT NOT NULL DEFAULT '',
    estimate     REAL NOT NULL DEFAULT 0,
    due          TEXT,
    tags         TEXT NOT NULL DEFAULT '',
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    completed_at TEXT
);

CREATE TABLE sessions (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    card_id    INTEGER REFERENCES cards(id) ON DELETE SET NULL,
    started_at TEXT NOT NULL,
    ended_at   TEXT NOT NULL,
    duration_s INTEGER NOT NULL,
    planned_s  INTEGER NOT NULL DEFAULT 0,
    completed  INTEGER NOT NULL DEFAULT 0,
    -- Kept after the card is deleted so history stays readable.
    card_title TEXT,
    board_name TEXT,
    tags       TEXT NOT NULL DEFAULT ''
);
CREATE INDEX sessions_started ON sessions(started_at);

CREATE TABLE interruptions (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    at         TEXT NOT NULL,
    kind       TEXT NOT NULL,
    reason     TEXT NOT NULL DEFAULT ''
);

CREATE TABLE items (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    date         TEXT NOT NULL,
    time         TEXT,
    duration_min INTEGER,
    recur        TEXT NOT NULL DEFAULT 'none',
    recur_until  TEXT,
    done         INTEGER NOT NULL DEFAULT 0,
    source       TEXT,
    notes        TEXT NOT NULL DEFAULT ''
);
CREATE INDEX items_date ON items(date);

CREATE TABLE habits (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL DEFAULT 'check',
    target           REAL NOT NULL DEFAULT 1,
    unit             TEXT NOT NULL DEFAULT '',
    schedule         TEXT NOT NULL,
    link_tag         TEXT,
    link_board       INTEGER REFERENCES boards(id) ON DELETE SET NULL,
    rest_on_holidays INTEGER NOT NULL DEFAULT 0,
    skips_per_month  INTEGER NOT NULL DEFAULT 2,
    created          TEXT NOT NULL,
    archived         INTEGER NOT NULL DEFAULT 0,
    position         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE habit_entries (
    habit_id INTEGER NOT NULL REFERENCES habits(id) ON DELETE CASCADE,
    date     TEXT NOT NULL,
    value    REAL NOT NULL DEFAULT 0,
    skipped  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (habit_id, date)
);

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
