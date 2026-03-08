use anyhow::Result;
use rusqlite::Connection;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   TEXT NOT NULL,
            event_type  TEXT NOT NULL,
            source      TEXT NOT NULL,
            project     TEXT,
            file_path   TEXT,
            payload     TEXT NOT NULL,
            session_id  TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_events_file_path ON events(file_path);
        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);

        CREATE TABLE IF NOT EXISTS file_nodes (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            path          TEXT UNIQUE NOT NULL,
            project       TEXT NOT NULL,
            first_seen    TEXT NOT NULL,
            last_touched  TEXT NOT NULL,
            touch_count   INTEGER DEFAULT 0,
            total_time_s  INTEGER DEFAULT 0,
            tags          TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_file_nodes_path ON file_nodes(path);

        CREATE TABLE IF NOT EXISTS file_relations (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_a      INTEGER REFERENCES file_nodes(id),
            file_b      INTEGER REFERENCES file_nodes(id),
            relation    TEXT NOT NULL,
            strength    REAL DEFAULT 1.0,
            last_seen   TEXT NOT NULL,
            UNIQUE(file_a, file_b, relation)
        );

        CREATE TABLE IF NOT EXISTS patterns (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            pattern_type    TEXT NOT NULL,
            description     TEXT NOT NULL,
            file_paths      TEXT NOT NULL,
            first_seen      TEXT NOT NULL,
            last_seen       TEXT NOT NULL,
            occurrence_count INTEGER DEFAULT 1,
            confidence      REAL DEFAULT 0.5
        );

        CREATE TABLE IF NOT EXISTS insights (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            created_at    TEXT NOT NULL,
            trigger_event INTEGER REFERENCES events(id),
            insight_type  TEXT NOT NULL,
            title         TEXT NOT NULL,
            body          TEXT NOT NULL,
            relevance     REAL NOT NULL,
            surfaced      INTEGER DEFAULT 0,
            dismissed     INTEGER DEFAULT 0,
            file_path     TEXT,
            project       TEXT
        );

        CREATE TABLE IF NOT EXISTS embeddings (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_type TEXT NOT NULL,
            source_id   INTEGER NOT NULL,
            vector      BLOB NOT NULL,
            text        TEXT NOT NULL
        );
        ",
    )?;

    Ok(())
}
