pub mod models;

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn new(app_data_dir: PathBuf) -> Result<Self, rusqlite::Error> {
        fs::create_dir_all(&app_data_dir).ok();
        let db_path = app_data_dir.join("jotv.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let migration = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(migration)?;

        // v2: add added_on_server column to channels
        let has_added: bool = conn.prepare("SELECT added_on_server FROM channels LIMIT 0").is_ok();
        if !has_added {
            conn.execute_batch(
                "ALTER TABLE channels ADD COLUMN added_on_server INTEGER NOT NULL DEFAULT 0;"
            )?;
        }

        // v2: add category column to favorites (safe for existing DBs)
        let has_category: bool = conn
            .prepare("SELECT category FROM favorites LIMIT 0")
            .is_ok();
        if !has_category {
            conn.execute_batch(
                "ALTER TABLE favorites ADD COLUMN category TEXT NOT NULL DEFAULT 'General';"
            )?;
        }

        // v2: index for search
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_channels_name ON channels(name COLLATE NOCASE);"
        ).ok();

        // v2: composite index for channels
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_channels_playlist_type_group ON channels(playlist_id, content_type, group_name);"
        ).ok();

        // v2: favorites category index
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_favorites_category ON favorites(category);"
        ).ok();

        // v3: recreate downloads table with nullable channel_id
        let needs_dl_fix: bool = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='downloads'",
            [],
            |row| row.get::<_, String>(0),
        ).map(|sql| sql.contains("NOT NULL REFERENCES channels")).unwrap_or(false);

        if needs_dl_fix {
            conn.execute_batch("
                CREATE TABLE IF NOT EXISTS downloads_new (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    channel_id INTEGER REFERENCES channels(id) ON DELETE SET NULL,
                    url TEXT NOT NULL,
                    file_path TEXT,
                    status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued', 'downloading', 'paused', 'completed', 'failed')),
                    progress REAL NOT NULL DEFAULT 0.0,
                    total_bytes INTEGER,
                    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                    retry_count INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    completed_at TEXT
                );
                INSERT OR IGNORE INTO downloads_new SELECT * FROM downloads;
                DROP TABLE downloads;
                ALTER TABLE downloads_new RENAME TO downloads;
                CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
            ").ok();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let migration = include_str!("../../migrations/001_initial.sql");
        conn.execute_batch(migration).unwrap();
        Database {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn test_creates_tables() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"playlists".to_string()));
        assert!(tables.contains(&"channels".to_string()));
        assert!(tables.contains(&"favorites".to_string()));
        assert!(tables.contains(&"epg_data".to_string()));
        assert!(tables.contains(&"viewing_history".to_string()));
        assert!(tables.contains(&"downloads".to_string()));
        assert!(tables.contains(&"settings".to_string()));
    }

    #[test]
    fn test_default_settings_inserted() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let theme: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'theme'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(theme, "\"dark\"");
    }
}
