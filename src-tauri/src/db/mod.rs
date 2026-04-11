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
