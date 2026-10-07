pub mod commands;
pub mod db;
pub mod parsers;

use commands::downloads::DownloadManager;
use commands::mpv_player::MpvState;
use commands::social::SocialDownloadManager;
use db::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Live TV renders MPV inside the app window (`--wid`), which only works on X11.
    // On Wayland sessions, run through XWayland unless the user picked a backend.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GDK_BACKEND").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }

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
            app.manage(commands::search::SearchIndex::default());
            app.manage(DownloadManager::new());
            app.manage(SocialDownloadManager::new());

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
            commands::playlists::get_vod_info,
            commands::playlists::refresh_playlist,
            commands::playlists::delete_playlist,
            commands::playlists::get_playlist_groups,
            commands::playlists::merge_playlists,
            commands::playlists::split_playlist,
            commands::playlists::export_playlist,
            // Channels
            commands::channels::get_channels,
            commands::channels::get_channel,
            commands::channels::browse_media,
            commands::channels::get_media_facets,
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
            commands::favorites::favorite_lists,
            commands::favorites::favorite_list_save,
            commands::favorites::favorite_list_delete,
            commands::favorites::favorite_set_list,
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
            commands::mpv_player::mpv_status,
            commands::mpv_player::mpv_go_live,
            commands::mpv_player::mpv_reconnect,
            commands::mpv_player::mpv_set_buffer,
            commands::mpv_player::mpv_badge_reconnecting,
            commands::mpv_player::mpv_toggle_mute,
            commands::mpv_player::mpv_seek,
            commands::mpv_player::mpv_osc,
            commands::mpv_player::mpv_tracks,
            commands::mpv_player::mpv_set_track,
            commands::mpv_player::probe_stream,
            commands::search::global_search,
            commands::search::search_warmup,
            commands::tmdb::tmdb_details,
            commands::tmdb::tmdb_check_key,
            commands::embed::embed_attach,
            commands::embed::embed_place,
            commands::embed::embed_detach,
            // Downloads
            commands::downloads::start_download,
            commands::downloads::queue_download,
            commands::downloads::get_downloads,
            commands::downloads::pause_download,
            commands::downloads::resume_download,
            commands::downloads::cancel_download,
            commands::downloads::clear_completed_downloads,
            commands::downloads::open_download_file,
            commands::downloads::show_in_folder,
            commands::downloads::get_default_download_dir,
            // Social Downloads
            commands::social::check_ytdlp,
            commands::social::get_social_video_info,
            commands::social::start_social_download,
            commands::social::cancel_social_download,
            commands::social::get_social_downloads,
            commands::social::clear_social_downloads,
            commands::reels::reels_list,
            commands::reels::reel_update,
            commands::reels::reels_move,
            commands::reels::reels_tag,
            commands::reels::reels_delete,
            commands::reels::reels_import,
            commands::reels::reel_probe,
            commands::reels::reel_set_cover,
            commands::reels::reel_played,
            commands::reels::reel_categories,
            commands::reels::reel_category_save,
            commands::reels::reel_category_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
