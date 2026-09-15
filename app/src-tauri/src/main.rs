#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod discovery;
mod downloads;
mod home;
mod images;
mod jellyfin;
mod library;
mod live;
mod notify;
mod playback;
mod settings;
mod trailer;
mod zoom;
#[cfg(target_os = "linux")]
mod player;

use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    gl_renderer: Option<String>,
    player_ready: bool,
    /// Discord presence is available in this build.
    discord_ready: bool,
}

#[tauri::command]
fn app_info(app: tauri::AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        #[cfg(target_os = "linux")]
        gl_renderer: player::gl_renderer(),
        #[cfg(not(target_os = "linux"))]
        gl_renderer: None,
        #[cfg(target_os = "linux")]
        player_ready: player::ready(),
        #[cfg(not(target_os = "linux"))]
        player_ready: false,
        discord_ready: discord::configured(),
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_info,
            jellyfin::startup,
            jellyfin::check_server,
            discovery::discover_servers,
            jellyfin::forget_server,
            jellyfin::sign_in,
            jellyfin::quick_connect_start,
            jellyfin::quick_connect_poll,
            jellyfin::quick_connect_cancel,
            jellyfin::sign_out,
            jellyfin::saved_accounts,
            jellyfin::switch_account,
            jellyfin::forget_account,
            jellyfin::servers,
            jellyfin::ping_server,
            jellyfin::remove_server,
            images::cached_posters,
            home::home,
            home::spotlight,
            home::set_favorite,
            library::libraries,
            library::library_items,
            library::search,
            library::item_detail,
            library::season_episodes,
            library::set_played,
            library::watch_queue,
            library::season_list,
            library::similar,
            library::card_detail,
            library::item_collections,
            library::collection_items,
            library::skip_segments,
            zoom::zoom_level,
            zoom::zoom_step,
            settings::settings,
            settings::update_settings,
            settings::system_fonts,
            playback::play,
            playback::playback_stop,
            playback::playback_toggle_pause,
            playback::next_episode,
            playback::playback_seek,
            playback::playback_set_volume,
            playback::playback_set_muted,
            playback::playback_set_track,
            playback::player_set_viewport,
            playback::player_set_fullscreen,
            playback::playback_set_speed,
            playback::play_trailer,
            trailer::trailers_in_app,
            trailer::open_trailer,
            playback::trickplay,
            downloads::downloads,
            downloads::downloaded_titles,
            downloads::server_downloads,
            downloads::remove_server_downloads,
            downloads::download_item,
            downloads::download_season,
            downloads::download_pause,
            downloads::download_resume,
            downloads::download_remove,
            downloads::download_set_quality,
            downloads::download_storage,
            downloads::check_download_location,
            downloads::sync_downloads,
        ])
        .register_asynchronous_uri_scheme_protocol("bloom-img", |ctx, req, responder| {
            images::protocol(ctx, req, responder)
        })
        .on_window_event(|window, event| {
            // Closing mid-playback: hold the window until the server has heard where it stopped.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                if app.state::<playback::Playback>().is_playing() {
                    api.prevent_close();
                    let window = window.clone();
                    tauri::async_runtime::spawn(async move {
                        playback::report_stop_before_exit(&app.state::<playback::Playback>()).await;
                        let _ = window.destroy();
                    });
                }
            }
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let cache = app.path().app_cache_dir()?;
            let version = app.package_info().version.to_string();

            let store = settings::SettingsStore::load(&dir);
            let saved = store.get();
            let zoom = zoom::Zoom::new(saved.zoom);
            if let Some(win) = app.get_webview_window("main") {
                if let Err(e) = win.set_zoom(zoom.level()) {
                    eprintln!("bloom: couldn't restore zoom: {e}");
                }
            }
            app.manage(zoom);
            app.manage(discord::Presence::new(app.handle().clone(), saved.discord_presence, saved.discord_show_title));
            app.manage(store);
            app.manage(playback::Playback::new(app.handle().clone(), dir.join("track-choices.json")));
            // The artwork cache is trimmed once a launch, off the startup path.
            let prune_from = cache.clone();
            tauri::async_runtime::spawn_blocking(move || images::prune_cache(&prune_from));
            let videos = app.path().video_dir().or_else(|_| app.path().home_dir().map(|home| home.join("Videos")))?;
            app.manage(downloads::Downloads::new(app.handle().clone(), &dir, videos.join("Bloom")));
            app.manage(jellyfin::Jellyfin::new(dir, cache, version)?);
            downloads::start(app.handle().clone());
            live::start(app.handle().clone());

            #[cfg(target_os = "linux")]
            {
                // Before attach: mpv's event thread starts there and delivers to this listener.
                let handle = app.handle().clone();
                player::set_listener(move |event| playback::on_player_event(&handle, event));
                let win = app.get_webview_window("main").expect("main window is declared in tauri.conf.json");
                player::attach(&win)?;
                if let Err(e) = player::set_volume(saved.volume) {
                    eprintln!("bloom: couldn't restore the volume: {e}");
                }
                if let Err(e) = player::set_subtitle_style(saved.subtitle_size.mpv_scale(), saved.subtitle_background.mpv_value()) {
                    eprintln!("bloom: couldn't restore the subtitle style: {e}");
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bloom");
}
