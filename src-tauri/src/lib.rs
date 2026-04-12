pub mod commands;
pub mod db;
pub mod parsers;

use commands::downloads::DownloadManager;
use commands::mpv_player::MpvState;
use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            let database =
                Database::new(app_data_dir).expect("Failed to initialize database");

            app.manage(database);
            app.manage(MpvState::new());
            app.manage(DownloadManager::new());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Playlists
            commands::playlists::get_playlists,
            commands::playlists::add_playlist_from_url,
            commands::playlists::add_playlist_from_file,
            commands::playlists::add_playlist_from_xtream,
            commands::playlists::get_xtream_account_info,
            commands::playlists::get_series_info,
            commands::playlists::refresh_playlist,
            commands::playlists::delete_playlist,
            commands::playlists::get_playlist_groups,
            commands::playlists::merge_playlists,
            commands::playlists::split_playlist,
            commands::playlists::export_playlist,
            // Channels
            commands::channels::get_channels,
            commands::channels::get_channels_by_type,
            commands::channels::get_groups_by_type,
            commands::channels::get_recently_added,
            commands::channels::get_channels_by_group,
            commands::channels::get_content_type_counts,
            commands::channels::search_channels_in_playlist,
            commands::channels::search_channels,
            commands::channels::get_dashboard_stats,
            // Favorites
            commands::favorites::get_favorites,
            commands::favorites::get_favorite_categories,
            commands::favorites::add_favorite,
            commands::favorites::remove_favorite,
            commands::favorites::toggle_favorite,
            commands::favorites::is_favorite,
            // History
            commands::history::record_viewing,
            commands::history::get_recently_watched,
            commands::history::get_viewing_history,
            // Settings
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::get_all_settings,
            // Recommendations
            commands::recommendations::get_recommendations,
            // EPG
            commands::epg::fetch_epg,
            commands::epg::get_epg_for_channel,
            commands::epg::get_current_program,
            // Player
            commands::player::launch_external_player,
            commands::player::detect_external_players,
            // MPV Player
            commands::mpv_player::mpv_play,
            commands::mpv_player::mpv_load,
            commands::mpv_player::mpv_pause,
            commands::mpv_player::mpv_stop,
            commands::mpv_player::mpv_fullscreen,
            commands::mpv_player::mpv_volume,
            commands::mpv_player::mpv_is_running,
            // Downloads
            commands::downloads::start_download,
            commands::downloads::queue_download,
            commands::downloads::get_downloads,
            commands::downloads::pause_download,
            commands::downloads::resume_download,
            commands::downloads::cancel_download,
            commands::downloads::clear_completed_downloads,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
