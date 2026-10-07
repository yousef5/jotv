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

        // v4: rating / year / genre from the Xtream lists (filters, sorting)
        let has_rating: bool = conn.prepare("SELECT rating FROM channels LIMIT 0").is_ok();
        if !has_rating {
            conn.execute_batch(
                "ALTER TABLE channels ADD COLUMN rating REAL;
                 ALTER TABLE channels ADD COLUMN year INTEGER;
                 ALTER TABLE channels ADD COLUMN genre TEXT;"
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

        // v4: social downloads history table
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS social_downloads (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT NOT NULL,
                thumbnail TEXT,
                platform TEXT NOT NULL DEFAULT 'Other',
                format_label TEXT,
                output_dir TEXT,
                file_path TEXT,
                status TEXT NOT NULL DEFAULT 'downloading',
                progress REAL NOT NULL DEFAULT 0.0,
                downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                completed_at TEXT
            );
        ").ok();

        // v5: social downloads double as a video library (reels)
        let has_reel_cols: bool = conn.prepare("SELECT tags FROM social_downloads LIMIT 0").is_ok();
        if !has_reel_cols {
            conn.execute_batch(
                "ALTER TABLE social_downloads ADD COLUMN name TEXT;
                 ALTER TABLE social_downloads ADD COLUMN category_id INTEGER;
                 ALTER TABLE social_downloads ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
                 ALTER TABLE social_downloads ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE social_downloads ADD COLUMN duration REAL;
                 ALTER TABLE social_downloads ADD COLUMN width INTEGER;
                 ALTER TABLE social_downloads ADD COLUMN height INTEGER;
                 ALTER TABLE social_downloads ADD COLUMN file_size INTEGER;
                 ALTER TABLE social_downloads ADD COLUMN thumb_path TEXT;
                 ALTER TABLE social_downloads ADD COLUMN plays INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE social_downloads ADD COLUMN last_played_at TEXT;",
            )?;
        }
        // Completed downloads stay in the library after "clear history"
        if conn.prepare("SELECT in_history FROM social_downloads LIMIT 0").is_err() {
            conn.execute_batch("ALTER TABLE social_downloads ADD COLUMN in_history INTEGER NOT NULL DEFAULT 1;")?;
        }
        // Photos live in the library next to videos
        if conn.prepare("SELECT media_type FROM social_downloads LIMIT 0").is_err() {
            conn.execute_batch("ALTER TABLE social_downloads ADD COLUMN media_type TEXT NOT NULL DEFAULT 'video';")?;
        }
        // v6: your own categories for favorites, one set per type (channels / movies / series)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS favorite_lists (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                color TEXT,
                sort INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        if conn.prepare("SELECT list_id FROM favorites LIMIT 0").is_err() {
            conn.execute_batch("ALTER TABLE favorites ADD COLUMN list_id INTEGER;")?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reel_categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                parent_id INTEGER REFERENCES reel_categories(id) ON DELETE CASCADE,
                color TEXT,
                sort INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_reel_categories_parent ON reel_categories(parent_id);",
        )?;

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
