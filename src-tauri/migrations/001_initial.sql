CREATE TABLE IF NOT EXISTS playlists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK(source_type IN ('m3u_url', 'm3u_file', 'xtream', 'stalker')),
    source_url TEXT NOT NULL,
    xtream_username TEXT,
    xtream_password TEXT,
    auto_update_interval INTEGER NOT NULL DEFAULT 0,
    last_updated_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS channels (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    group_name TEXT NOT NULL DEFAULT '',
    stream_url TEXT NOT NULL,
    logo_url TEXT,
    epg_id TEXT,
    content_type TEXT NOT NULL DEFAULT 'live',
    added_on_server INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_channels_playlist ON channels(playlist_id);
CREATE INDEX IF NOT EXISTS idx_channels_group ON channels(group_name);
CREATE INDEX IF NOT EXISTS idx_channels_content_type ON channels(content_type);

CREATE TABLE IF NOT EXISTS favorites (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    category TEXT NOT NULL DEFAULT 'General',
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(channel_id)
);

CREATE TABLE IF NOT EXISTS epg_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_epg_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    category TEXT
);

CREATE INDEX IF NOT EXISTS idx_epg_channel ON epg_data(channel_epg_id);
CREATE INDEX IF NOT EXISTS idx_epg_time ON epg_data(start_time, end_time);

CREATE TABLE IF NOT EXISTS viewing_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    duration_seconds INTEGER NOT NULL DEFAULT 0,
    group_name TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_history_channel ON viewing_history(channel_id);
CREATE INDEX IF NOT EXISTS idx_history_started ON viewing_history(started_at);

CREATE TABLE IF NOT EXISTS downloads (
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

CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO settings (key, value) VALUES ('theme', '"dark"');
INSERT OR IGNORE INTO settings (key, value) VALUES ('download_dir', '"~/JoTV/Downloads"');
INSERT OR IGNORE INTO settings (key, value) VALUES ('max_concurrent_downloads', '3');
INSERT OR IGNORE INTO settings (key, value) VALUES ('external_player', '"none"');
INSERT OR IGNORE INTO settings (key, value) VALUES ('external_player_path', '""');
INSERT OR IGNORE INTO settings (key, value) VALUES ('auto_update_default', '0');
