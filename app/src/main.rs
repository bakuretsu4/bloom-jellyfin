//! Bloom's Slint interface, on `bloom-core`.
//!
//! Development knobs (all optional, none needed to use it):
//!   BLOOM_DATA_DIR, BLOOM_CACHE_DIR  where it keeps things (default `dev.bloom.app` under the
//!                                    XDG folders)
//!   BLOOM_SERVER, BLOOM_USER, BLOOM_PASS  sign in without typing; the test account, never a real one
//!   BLOOM_SILENT=1                   play with no sound
//!   BLOOM_GO=title|library|libraries|downloads|settings|search:text  open a page once Home loads
//!   BLOOM_AUTOPLAY=1                 play the first film or episode on Home once it loads
//!   BLOOM_HWDEC=nvdec-copy|vaapi|no  force a decoder, over the setting
//!   BLOOM_MPV_OPTS=a=b,c=d           more mpv options, last; BLOOM_MPV_LOG=/path writes mpv's log
//!   BLOOM_VSYNC=1                    leave the driver's swap interval alone (Wayland)
//!   BLOOM_STATS=1                    print frames per second (and mpv's counters) once a second
//!   BLOOM_LOG_EVENTS=1               print the player's events as they arrive
//!   BENCH=wheel                      feed scroll events to Home (through Slint's own input path)
//!   BLOOM_SCRIPT="5:click:40,178;9:type:odyssey;12:key:Escape"  drive the UI through Slint's input path
//!   BENCH=keys                       feed pointer and key events to the player: pause, resume, mute
//!   SPIKE_SECONDS=N                  quit after N seconds
//!   LIGHT=1                          the light scheme

mod awake;
mod cards;
mod collections;
mod downloads;
mod events;
mod flow;
mod home;
mod hover;
mod live;
mod music;
mod mpv;
mod nav;
mod person;
mod servers;
mod palette;
mod pointer;
mod playpage;
mod presets;
mod search;
mod session;
mod settings;
mod shell;
mod stats;
mod text;
mod themes;
mod themes_editor;
mod theme;
mod title;
mod wall;
mod watch;
mod zoom;

use bloom_core::{Ctx, Paths};
use slint::{ComponentHandle, RenderingState};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

slint::include_modules!();

const APP_ID: &str = "dev.bloom.app";
/// What the Slint interface called its folder while it ran beside the first (Tauri) version.
const OLD_SLINT_ID: &str = "dev.bloom.slint";

/// How long the controls stay up with nothing moving and a video playing.
const CONTROLS_IDLE: Duration = Duration::from_millis(2500);

fn folder(var: &str, base: Option<std::path::PathBuf>) -> std::path::PathBuf {
    if let Some(chosen) = std::env::var_os(var) {
        return chosen.into();
    }
    let base = base.unwrap_or_else(std::env::temp_dir);
    let folder = base.join(APP_ID);
    // The sign-ins, settings and downloads of the interface's development builds carry over, once, to
    // the folder Bloom keeps them in now (one that already exists is never replaced).
    let old = base.join(OLD_SLINT_ID);
    if !folder.exists() && old.exists() {
        let _ = std::fs::rename(&old, &folder);
    }
    folder
}

/// The window system's display, which mpv needs for VA-API (Intel and AMD).
fn display_of(app: &App) -> Option<mpv::Display> {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
    let handle = app.window().window_handle();
    match handle.display_handle().ok()?.as_raw() {
        RawDisplayHandle::Wayland(w) => Some(mpv::Display::wayland(w.display.as_ptr())),
        RawDisplayHandle::Xlib(x) => x.display.map(|d| mpv::Display::x11(d.as_ptr())),
        _ => None,
    }
}

fn main() {
    // NVIDIA's EGL on Wayland halves a 240 Hz display to 120 unless the swap interval is off
    // (measured on a 240 Hz display with an RTX 3060). Wayland only: on X11 nothing would pace the loop.
    // BLOOM_VSYNC=1 leaves the driver's own swap interval alone, to tell it from a picture problem.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("__GL_SYNC_TO_VBLANK").is_none() && std::env::var_os("BLOOM_VSYNC").is_none() {
        // SAFETY: first statement of main, no other threads exist yet.
        unsafe { std::env::set_var("__GL_SYNC_TO_VBLANK", "0") };
    }

    let runtime = tokio::runtime::Runtime::new().expect("couldn't start the async runtime");
    bloom_core::rt::init(runtime.handle().clone());

    // What the desktop knows the window by: its icon and launcher entry (dev.bloom.app.desktop) are found by it.
    let _ = slint::set_xdg_app_id(APP_ID);
    let app = App::new().expect("couldn't create the window");
    let weak = app.as_weak();

    // The engine, the shell that wraps it, and the core on top. The engine is never dropped
    // (the core's listener holds the core, which holds the shell, which holds the engine): mpv is
    // torn down by the process ending, after the window's video output has been freed below.
    let engine = mpv::Engine::new().expect("couldn't start mpv");
    // The shell is built before the `Ctx` that holds it, yet its event handler needs the `Ctx`
    // (to show the page behind the player when playback ends): so it's filled in just after.
    let late_ctx: Arc<OnceLock<Ctx>> = Arc::default();
    let ctx = Ctx::new(Arc::new(shell::SlintShell::new(engine.clone(), weak.clone(), {
        let (weak, late_ctx) = (weak.clone(), late_ctx.clone());
        move |event, payload| events::on_event(late_ctx.get().cloned(), &weak, event, payload)
    })));
    let _ = late_ctx.set(ctx.clone());
    let videos = dirs::video_dir().or_else(|| dirs::home_dir().map(|h| h.join("Videos"))).unwrap_or_else(std::env::temp_dir);
    let data_folder = folder("BLOOM_DATA_DIR", dirs::data_dir());
    let paths = Paths { data: data_folder.clone(), cache: folder("BLOOM_CACHE_DIR", dirs::cache_dir()), videos };
    for dir in [&paths.data, &paths.cache] {
        std::fs::create_dir_all(dir).expect("couldn't create the data folders");
    }
    let saved = bloom_core::start(&ctx, paths, env!("CARGO_PKG_VERSION").to_string()).expect("couldn't start Bloom's core");
    // Only now: mpv reports its properties as soon as it exists, and the core's handler needs the
    // services `start` has just registered. (An event that arrives earlier is dropped.)
    engine.set_listener({
        let ctx = ctx.clone();
        let log = std::env::var_os("BLOOM_LOG_EVENTS").is_some();
        move |event| {
            if log {
                use bloom_core::shell::Event;
                let name = match &event {
                    Event::StartFile { entry } => format!("StartFile({entry})"),
                    Event::FileLoaded => "FileLoaded".into(),
                    Event::PlaybackRestart => "PlaybackRestart".into(),
                    Event::EndFile { entry, .. } => format!("EndFile({entry})"),
                    Event::Position(p) => format!("Position({p:.1})"),
                    Event::Duration(d) => format!("Duration({d:.1})"),
                    Event::Paused(p) => format!("Paused({p})"),
                    Event::Buffering(b) => format!("Buffering({b})"),
                    Event::Decoder(d) => format!("Decoder({d:?})"),
                    Event::Volume(v) => format!("Volume({v})"),
                    Event::Speed(s) => format!("Speed({s})"),
                    Event::Muted(m) => format!("Muted({m})"),
                    Event::CacheEnd(c) => format!("CacheEnd({c:.1})"),
                    Event::Tracks(t) => format!("Tracks({})", t.len()),
                };
                eprintln!("mpv {name}");
            }
            bloom_core::playback::on_player_event(&ctx, event)
        }
    });

    // What the core leaves to the interface: the player's remembered volume.
    // BLOOM_SILENT=1 (development): no sound, and the remembered volume is left alone.
    let volume = if std::env::var_os("BLOOM_SILENT").is_some() { 0.0 } else { saved.volume };
    if let Err(e) = engine.set_volume(volume) {
        eprintln!("bloom: couldn't restore the volume: {e}");
    }

    settings::set_data_folder(&data_folder);
    wall::init(&data_folder);
    theme::apply_settings(&app, &saved, false);
    app.global::<Theme>().set_reduce_motion(saved.reduce_motion);
    themes_editor::wire(&app, &ctx);
    theme::follow_desktop(ctx.clone(), app.as_weak());
    zoom::restore(&app, saved.zoom);
    session::init(&ctx);

    app.on_check_server({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |address| flow::check_server(ctx.clone(), weak.clone(), address.to_string())
    });
    app.on_sign_in({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |user, pass| flow::sign_in(ctx.clone(), weak.clone(), user.to_string(), pass.to_string())
    });
    app.on_change_server({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::change_server(ctx.clone(), weak.clone())
    });
    app.on_resume_account({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |server, user| flow::resume(ctx.clone(), weak.clone(), server.to_string(), user.to_string())
    });
    app.on_forget_account({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |server, user| flow::forget(ctx.clone(), weak.clone(), server.to_string(), user.to_string())
    });
    app.on_connect_to({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |address| flow::connect_to(ctx.clone(), weak.clone(), address.to_string())
    });
    app.on_search_again({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::search_again(ctx.clone(), weak.clone())
    });
    app.on_new_code({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::qc_start(ctx.clone(), weak.clone())
    });
    app.on_cancel_sign_in({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::cancel_sign_in(ctx.clone(), weak.clone())
    });
    app.on_account_menu_opened({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::account_menu_opened(ctx.clone(), weak.clone())
    });
    app.on_switch_account({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |server, user| flow::switch_account(ctx.clone(), weak.clone(), server.to_string(), user.to_string())
    });
    app.on_add_account({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::add_account(ctx.clone(), weak.clone())
    });
    app.on_sign_out({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || flow::sign_out(ctx.clone(), weak.clone())
    });

    // Navigation and pages.
    app.on_activate({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, kind, name| nav::activate(ctx.clone(), weak.clone(), id.to_string(), kind.to_string(), name.to_string())
    });
    app.on_open_title({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| nav::go(ctx.clone(), weak.clone(), nav::Route::Title(id.to_string()))
    });
    app.on_watch_left({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::left_screen(ctx.clone(), weak.clone())
    });
    app.on_go_back({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::back(ctx.clone(), weak.clone())
    });
    app.on_go_home({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::reset(ctx.clone(), weak.clone(), nav::Route::Home)
    });
    app.on_go_downloads({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::section(ctx.clone(), weak.clone(), nav::Route::Downloads)
    });
    app.on_download_play({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |item| watch::play(ctx.clone(), weak.clone(), item.to_string())
    });
    app.on_download_pause({
        let ctx = ctx.clone();
        move |id| downloads::pause(&ctx, id.to_string())
    });
    app.on_download_resume({
        let ctx = ctx.clone();
        move |id| downloads::resume(&ctx, id.to_string())
    });
    app.on_download_remove({
        let ctx = ctx.clone();
        move |id| downloads::remove(&ctx, id.to_string())
    });
    app.on_download_toggle_group({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |group| downloads::toggle_group(ctx.clone(), weak.clone(), group.to_string())
    });
    app.on_download_quality({
        let ctx = ctx.clone();
        move |id, index| downloads::set_quality(&ctx, id.to_string(), index)
    });
    app.on_open_person({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, name| nav::go(ctx.clone(), weak.clone(), nav::Route::Person { id: id.to_string(), name: name.to_string() })
    });
    app.on_title_download({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::download_title(ctx.clone(), weak.clone())
    });
    app.global::<Music>().on_play_all({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |shuffle| music::play_all(ctx.clone(), weak.clone(), shuffle)
    });
    app.global::<Music>().on_play_song({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |index| music::play_song(ctx.clone(), weak.clone(), index)
    });
    app.global::<Music>().on_open_artist({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || music::open_artist(ctx.clone(), weak.clone())
    });
    app.global::<Music>().on_open_playing({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || music::open_playing(ctx.clone(), weak.clone())
    });
    app.global::<Music>().on_step({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |delta| music::step(ctx.clone(), weak.clone(), delta)
    });
    app.global::<Music>().on_stop({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || music::stop(ctx.clone(), weak.clone())
    });
    app.global::<Music>().on_toggle({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || music::toggle(ctx.clone(), weak.clone())
    });
    app.on_lib_music_kind_chosen({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |kind| nav::music_kind_chosen(ctx.clone(), weak.clone(), kind)
    });
    app.on_title_edit_collections({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || collections::open(ctx.clone(), weak.clone())
    });
    app.on_collections_toggle({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, contains| collections::toggle(ctx.clone(), weak.clone(), id.to_string(), contains)
    });
    app.on_collections_create({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |name| collections::create(ctx.clone(), weak.clone(), name.to_string())
    });
    app.on_collections_closed({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || collections::closed(ctx.clone(), weak.clone())
    });
    app.on_collection_remove({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| collections::remove_from_page(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_title_download_all({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::download_all(ctx.clone(), weak.clone())
    });
    app.on_title_download_season({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::download_season(ctx.clone(), weak.clone())
    });
    {
        let hover = app.global::<Hover>();
        hover.on_open({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id, _kind, title, x, y, w, h| hover::open(ctx.clone(), weak.clone(), id.to_string(), title.to_string(), (x, y, w, h))
        });
        hover.on_activate({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id, kind, title| nav::activate(ctx.clone(), weak.clone(), id.to_string(), kind.to_string(), title.to_string())
        });
        hover.on_play({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id| hover::play(ctx.clone(), weak.clone(), id.to_string())
        });
        hover.on_toggle_favorite({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || hover::toggle_favorite(ctx.clone(), weak.clone())
        });
        hover.on_toggle_played({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || hover::toggle_played(ctx.clone(), weak.clone())
        });
    }
    app.on_home_play({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::play(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_home_favorite({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| home::spotlight_favorite(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_home_retry({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::reset(ctx.clone(), weak.clone(), nav::Route::Home)
    });
    app.on_palette_opened({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || palette::opened(ctx.clone(), weak.clone())
    });
    app.on_palette_edited({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |text| palette::edited(ctx.clone(), weak.clone(), text.to_string())
    });
    app.on_palette_accept({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |i| palette::accept(ctx.clone(), weak.clone(), i)
    });
    app.on_palette_move({
        let weak = weak.clone();
        move |delta| {
            if let Some(app) = weak.upgrade() {
                palette::move_active(&app, delta)
            }
        }
    });
    stats::install(&app, engine.clone());
    pointer::install(&app, &ctx, &weak);
    app.on_zoom_step({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |direction| zoom::step(&ctx, &weak, direction)
    });
    app.on_server_menu_opened({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || session::server_menu_opened(ctx.clone(), weak.clone())
    });
    app.on_open_server({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| servers::open(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_retry_server({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || session::retry(ctx.clone(), weak.clone())
    });
    {
        let player = app.global::<Player>();
        player.on_set_mode({
            let weak = weak.clone();
            move |mode| playpage::set_mode(&weak, mode.as_str())
        });
        player.on_viewport({
            let ctx = ctx.clone();
            move |x, y, w, h, ww, wh| playpage::viewport(&ctx, x, y, w, h, ww, wh)
        });
        player.on_toggle_subtitles({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || watch::toggle_subtitles(ctx.clone(), weak.clone())
        });
        player.on_scrub_hover({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |f| playpage::scrub_hover(ctx.clone(), weak.clone(), f)
        });
        player.on_toggle_favorite({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || playpage::toggle_favorite(ctx.clone(), weak.clone())
        });
        player.on_toggle_played({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || playpage::toggle_played(ctx.clone(), weak.clone())
        });
        player.on_download({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || playpage::download(ctx.clone(), weak.clone())
        });
        player.on_open_owner({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || playpage::open_owner(ctx.clone(), weak.clone())
        });
        player.on_open_person({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id, name| playpage::open_person(ctx.clone(), weak.clone(), id.to_string(), name.to_string())
        });
        player.on_queue_play({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id| playpage::queue_play(ctx.clone(), weak.clone(), id.to_string())
        });
        player.on_queue_toggle_played({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |id| playpage::queue_toggle_played(ctx.clone(), weak.clone(), id.to_string())
        });
        player.on_queue_season({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move |i| playpage::queue_season(ctx.clone(), weak.clone(), i)
        });
        player.on_queue_next_season({
            let (ctx, weak) = (ctx.clone(), weak.clone());
            move || playpage::queue_next_season(ctx.clone(), weak.clone())
        });
    }
    app.on_title_trailer({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::trailer(ctx.clone(), weak.clone())
    });
    app.on_title_retry({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::retry(ctx.clone(), weak.clone())
    });
    app.on_title_toggle_sort({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::toggle_sort(ctx.clone(), weak.clone())
    });
    app.on_title_toggle_season_played({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::toggle_season_played(ctx.clone(), weak.clone())
    });
    app.on_title_episode_menu({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, action| title::episode_menu(ctx.clone(), weak.clone(), id.to_string(), action.to_string())
    });
    app.on_open_collection({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, name| nav::go(ctx.clone(), weak.clone(), nav::Route::Collection { id: id.to_string(), name: name.to_string() })
    });
    app.on_location_submitted({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |path| settings::set_location(ctx.clone(), weak.clone(), path.to_string())
    });
    app.on_go_servers({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::section(ctx.clone(), weak.clone(), nav::Route::Servers)
    });
    app.on_server_open({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| servers::open(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_server_check({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| servers::check(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_server_ask_remove({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| servers::ask_remove(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_server_cancel_remove({
        let weak = weak.clone();
        move || servers::cancel_remove(weak.clone())
    });
    app.on_server_remove({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id, delete| servers::remove(ctx.clone(), weak.clone(), id.to_string(), delete)
    });
    app.on_server_add({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |address| servers::add(ctx.clone(), weak.clone(), address.to_string())
    });
    app.on_go_settings({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::section(ctx.clone(), weak.clone(), nav::Route::Settings)
    });
    app.on_setting_changed({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |key, value| settings::change(ctx.clone(), weak.clone(), key.to_string(), value)
    });
    app.on_go_libraries({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::section(ctx.clone(), weak.clone(), nav::Route::Libraries)
    });
    app.on_search_edited({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |text| search::edited(ctx.clone(), weak.clone(), text.to_string())
    });
    app.on_search_accepted({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |text| search::accepted(ctx.clone(), weak.clone(), text.to_string())
    });
    app.on_suggest_down({
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                search::down(&app)
            }
        }
    });
    app.on_suggest_up({
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                search::up(&app)
            }
        }
    });
    app.on_suggest_escape({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || search::escape(ctx.clone(), weak.clone())
    });
    app.on_suggest_pick({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |i| search::pick(ctx.clone(), weak.clone(), i)
    });
    app.on_suggest_see_all({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || {
            if let Some(app) = weak.upgrade() {
                let typed = app.get_search_text().to_string();
                search::see_all(ctx.clone(), weak.clone(), typed)
            }
        }
    });
    app.on_search_retry({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || search::retry(ctx.clone(), weak.clone())
    });
    app.on_search_clear({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || search::clear(ctx.clone(), weak.clone())
    });
    app.on_lib_sort_chosen({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |index| nav::sort_chosen(ctx.clone(), weak.clone(), index)
    });
    app.on_lib_toggle_downloaded({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::toggle_downloaded(ctx.clone(), weak.clone())
    });
    app.on_lib_page_chosen({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |page| nav::page_chosen(ctx.clone(), weak.clone(), page)
    });
    app.on_lib_retry({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || nav::retry_list(ctx.clone(), weak.clone())
    });
    app.on_title_play({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::play(ctx.clone(), weak.clone(), id.to_string())
    });
    app.on_title_toggle_favorite({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::toggle_favorite(ctx.clone(), weak.clone())
    });
    app.on_title_toggle_played({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || title::toggle_played(ctx.clone(), weak.clone())
    });
    app.on_title_season_chosen({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |season| title::season_chosen(ctx.clone(), weak.clone(), season.to_string())
    });

    // The player.
    app.on_back({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::back(ctx.clone(), weak.clone())
    });
    app.on_toggle_pause({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::toggle_pause(ctx.clone(), weak.clone())
    });
    app.on_seek_by({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |seconds| watch::seek_by(ctx.clone(), weak.clone(), seconds)
    });
    app.on_seek_to({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |fraction| {
            if let Some(app) = weak.upgrade() {
                watch::seek_to(ctx.clone(), weak.clone(), fraction as f64 * app.get_duration() as f64);
            }
        }
    });
    app.on_set_volume({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |volume| watch::set_volume(ctx.clone(), weak.clone(), volume)
    });
    app.on_toggle_mute({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || {
            if let Some(app) = weak.upgrade() {
                watch::set_muted(ctx.clone(), weak.clone(), !app.get_muted());
            }
        }
    });
    app.on_set_audio({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::set_track(ctx.clone(), weak.clone(), true, id)
    });
    app.on_set_subtitle({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::set_track(ctx.clone(), weak.clone(), false, id)
    });
    app.on_set_speed({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |speed| watch::set_speed(ctx.clone(), weak.clone(), speed)
    });
    app.on_set_quality({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |key| watch::set_quality(ctx.clone(), weak.clone(), key.to_string())
    });
    app.on_skip({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::skip(ctx.clone(), weak.clone())
    });
    app.on_play_next({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::play_next(ctx.clone(), weak.clone())
    });
    app.on_cancel_up_next({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::cancel_up_next(ctx.clone(), weak.clone())
    });
    app.on_jump_chapter({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |direction| watch::jump_chapter(ctx.clone(), weak.clone(), direction)
    });
    app.on_toggle_mute({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || {
            if let Some(app) = weak.upgrade() {
                watch::set_muted(ctx.clone(), weak.clone(), !app.get_muted());
            }
        }
    });
    app.on_set_audio({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::set_track(ctx.clone(), weak.clone(), true, id)
    });
    app.on_set_subtitle({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |id| watch::set_track(ctx.clone(), weak.clone(), false, id)
    });
    app.on_set_speed({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |speed| watch::set_speed(ctx.clone(), weak.clone(), speed)
    });
    app.on_set_quality({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |key| watch::set_quality(ctx.clone(), weak.clone(), key.to_string())
    });
    app.on_skip({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::skip(ctx.clone(), weak.clone())
    });
    app.on_play_next({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::play_next(ctx.clone(), weak.clone())
    });
    app.on_cancel_up_next({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move || watch::cancel_up_next(ctx.clone(), weak.clone())
    });
    app.on_jump_chapter({
        let (ctx, weak) = (ctx.clone(), weak.clone());
        move |direction| watch::jump_chapter(ctx.clone(), weak.clone(), direction)
    });

    // The controls fade out after a few idle seconds of a playing video, and return on movement.
    let last_activity = Rc::new(Cell::new(Instant::now()));
    app.on_activity({
        let (weak, last) = (weak.clone(), last_activity.clone());
        move || {
            last.set(Instant::now());
            if let Some(app) = weak.upgrade() {
                app.set_controls_visible(true);
            }
        }
    });
    let idle_check = slint::Timer::default();
    idle_check.start(slint::TimerMode::Repeated, Duration::from_millis(500), {
        let (weak, last) = (weak.clone(), last_activity.clone());
        move || {
            if let Some(app) = weak.upgrade() {
                let watching = app.get_screen() == Screen::Watch;
                if watching && !app.get_paused() && !app.get_menu_open() && app.get_controls_visible() && last.get().elapsed() >= CONTROLS_IDLE {
                    app.set_controls_visible(false);
                }
            }
        }
    });

    // Closing mid-film: hold the window until the server has heard where playback stopped, so the
    // resume point is right (the core waits a few seconds at most).
    app.window().on_close_requested({
        let ctx = ctx.clone();
        move || {
            if !ctx.state::<bloom_core::playback::Playback>().is_playing() {
                return slint::CloseRequestResponse::HideWindow;
            }
            let ctx = ctx.clone();
            let asked = Instant::now();
            eprintln!("bloom: closing mid-playback; telling the server where it stopped");
            bloom_core::rt::spawn(async move {
                bloom_core::playback::report_stop_before_exit(&ctx.state::<bloom_core::playback::Playback>()).await;
                eprintln!("bloom: the server heard, after {:.1}s", asked.elapsed().as_secs_f64());
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            });
            slint::CloseRequestResponse::KeepWindowShown
        }
    });

    flow::start(ctx.clone(), weak.clone());

    // One rendering notifier does everything that happens around a frame: it makes mpv's video
    // output as the window's GL context comes up, draws the picture under the UI while watching,
    // frees it with the context, and (when asked) prints frame statistics.
    let stats = std::env::var_os("BLOOM_STATS").is_some();
    {
        let (engine, weak) = (engine.clone(), weak.clone());
        let (mut frames, mut worst, mut last, mut start) = (0u32, Duration::ZERO, Instant::now(), Instant::now());
        app.window()
            .set_rendering_notifier(move |state, _| match state {
                RenderingState::RenderingSetup => {
                    let display = weak.upgrade().and_then(|app| display_of(&app));
                    let wake_weak = weak.clone();
                    let made = engine.create_render(display, move || {
                        let _ = wake_weak.upgrade_in_event_loop(|app| app.window().request_redraw());
                    });
                    if let Err(e) = made {
                        eprintln!("bloom: no video output: {e}");
                    }
                }
                RenderingState::BeforeRendering => {
                    let now = Instant::now();
                    if let Some(app) = weak.upgrade() {
                        if app.get_screen() == Screen::Watch {
                            let size = app.window().size();
                            engine.render(size.width as i32, size.height as i32);
                        }
                    }
                    if stats {
                        worst = worst.max(now - last);
                        last = now;
                        frames += 1;
                        if now - start >= Duration::from_secs(1) {
                            let secs = (now - start).as_secs_f64();
                            let video = engine
                                .property("hwdec-current")
                                .map(|d| {
                                    format!(
                                        " hwdec={d} vf-fps={} drops(decoder/vo)={}/{}",
                                        engine.property("estimated-vf-fps").unwrap_or_default(),
                                        engine.property("decoder-frame-drop-count").unwrap_or_default(),
                                        engine.property("frame-drop-count").unwrap_or_default()
                                    )
                                })
                                .unwrap_or_default();
                            eprintln!("frames/s={:.1} worst={:.1}ms{video}", frames as f64 / secs, worst.as_secs_f64() * 1e3);
                            (frames, worst, start) = (0, Duration::ZERO, now);
                        }
                    }
                }
                RenderingState::RenderingTeardown => engine.destroy_render(),
                _ => {}
            })
            .expect("needs an OpenGL renderer");
    }

    let wheel = slint::Timer::default();
    if std::env::var("BENCH").as_deref() == Ok("wheel") {
        use slint::platform::WindowEvent;
        let weak = weak.clone();
        let begun = Instant::now();
        // Let Home load, then one wheel notch per 4 ms, reversing every 2 s so both ends are hit.
        wheel.start(slint::TimerMode::Repeated, Duration::from_millis(4), move || {
            let Some(app) = weak.upgrade() else { return };
            if begun.elapsed() < Duration::from_secs(6) {
                return;
            }
            let sign = if (begun.elapsed().as_secs() / 2).is_multiple_of(2) { 1.0 } else { -1.0 };
            let position = slint::LogicalPosition::new(800.0, 500.0);
            app.window().dispatch_event(WindowEvent::PointerMoved { position });
            app.window().dispatch_event(WindowEvent::PointerScrolled { position, delta_x: -18.0 * sign, delta_y: -10.0 * sign });
        });
    }
    // Drives the player's controls the way a person would, through Slint's input path, so they can
    // be tried without a keyboard or pointer: a pointer move at 10 s, space (pause) at 12 s, space
    // (resume) at 15 s, "m" (mute) at 17 s, then silence so the controls fade.
    let keys = slint::Timer::default();
    if std::env::var("BENCH").as_deref() == Ok("keys") {
        use slint::platform::WindowEvent;
        let weak = weak.clone();
        let begun = Instant::now();
        let mut done = 0;
        keys.start(slint::TimerMode::Repeated, Duration::from_millis(100), move || {
            let Some(app) = weak.upgrade() else { return };
            let t = begun.elapsed().as_secs_f64();
            let steps: [(f64, &str); 4] = [(10.0, "move"), (12.0, " "), (15.0, " "), (17.0, "m")];
            if done < steps.len() && t >= steps[done].0 {
                let window = app.window();
                if steps[done].1 == "move" {
                    window.dispatch_event(WindowEvent::PointerMoved { position: slint::LogicalPosition::new(400.0, 300.0) });
                } else {
                    let text: slint::SharedString = steps[done].1.into();
                    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
                    window.dispatch_event(WindowEvent::KeyReleased { text });
                }
                eprintln!("bench: step {} ({:?}) at {t:.1}s", done, steps[done].1);
                done += 1;
            }
        });
    }
    // BLOOM_SCRIPT: "seconds:action;..." with actions click:x,y, move:x,y, scroll:dy, type:text, key:Name
    // and activate:id:Kind:Name
    // (Escape, Return), sent the way a person's input would be, to try flows without a pointer.
    let script = slint::Timer::default();
    if let Ok(spec) = std::env::var("BLOOM_SCRIPT") {
        use slint::platform::{Key, WindowEvent};
        let mut steps: Vec<(f64, String)> = spec
            .split(';')
            .filter_map(|s| s.split_once(':').and_then(|(t, a)| Some((t.trim().parse().ok()?, a.trim().to_string()))))
            .collect();
        steps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let weak = weak.clone();
        let begun = Instant::now();
        let mut done = 0;
        script.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(app) = weak.upgrade() else { return };
            while done < steps.len() && begun.elapsed().as_secs_f64() >= steps[done].0 {
                let (_, action) = &steps[done];
                let window = app.window();
                let point = |s: &str| {
                    let (x, y) = s.split_once(',')?;
                    Some(slint::LogicalPosition::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
                };
                match action.split_once(':') {
                    Some(("click", xy)) | Some(("move", xy)) => {
                        if let Some(position) = point(xy) {
                            window.dispatch_event(WindowEvent::PointerMoved { position });
                            if action.starts_with("click") {
                                window.dispatch_event(WindowEvent::PointerPressed { position, button: slint::platform::PointerEventButton::Left });
                                window.dispatch_event(WindowEvent::PointerReleased { position, button: slint::platform::PointerEventButton::Left });
                            }
                        }
                    }
                    // signin opens the sign-in screen, as "Add another account" does.
                    Some(("signin", _)) | None if action == "signin" => app.invoke_add_account(),
                    // seekto:0.99 presses the seek bar at that fraction.
                    Some(("seekto", fraction)) => {
                        app.invoke_seek_to(fraction.trim().parse().unwrap_or(0.0));
                    }
                    // activate:id:Kind:Name presses a card, as a click on it would.
                    Some(("activate", rest)) => {
                        let mut parts = rest.splitn(3, ':');
                        if let (Some(id), Some(kind), Some(name)) = (parts.next(), parts.next(), parts.next()) {
                            app.invoke_activate(id.into(), kind.into(), name.into());
                        }
                    }
                    Some(("scroll", dy)) => {
                        let position = slint::LogicalPosition::new(800.0, 500.0);
                        window.dispatch_event(WindowEvent::PointerMoved { position });
                        window.dispatch_event(WindowEvent::PointerScrolled { position, delta_x: 0.0, delta_y: dy.trim().parse().unwrap_or(0.0) });
                    }
                    Some(("type", text)) => {
                        for c in text.chars() {
                            let t: slint::SharedString = c.to_string().into();
                            window.dispatch_event(WindowEvent::KeyPressed { text: t.clone() });
                            window.dispatch_event(WindowEvent::KeyReleased { text: t });
                        }
                    }
                    // chord:ctrl+k, chord:alt+Left: modifiers pressed, the key, then released.
                    Some(("chord", spec)) => {
                        let mut mods: Vec<slint::SharedString> = Vec::new();
                        let mut key: slint::SharedString = "".into();
                        for part in spec.split('+') {
                            match part {
                                "ctrl" => mods.push(Key::Control.into()),
                                "alt" => mods.push(Key::Alt.into()),
                                "shift" => mods.push(Key::Shift.into()),
                                "Left" => key = Key::LeftArrow.into(),
                                "Escape" => key = Key::Escape.into(),
                                "plus" => key = "+".into(),
                                "minus" => key = "-".into(),
                                other => key = other.into(),
                            }
                        }
                        for m in &mods {
                            window.dispatch_event(WindowEvent::KeyPressed { text: m.clone() });
                        }
                        window.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
                        window.dispatch_event(WindowEvent::KeyReleased { text: key });
                        for m in mods.iter().rev() {
                            window.dispatch_event(WindowEvent::KeyReleased { text: m.clone() });
                        }
                    }
                    Some(("key", name)) => {
                        let key: slint::SharedString = match name {
                            "Escape" => Key::Escape.into(),
                            "Return" => Key::Return.into(),
                            "BackSpace" => Key::Backspace.into(),
                            "Tab" => Key::Tab.into(),
                            "Space" => " ".into(),
                            "Down" => Key::DownArrow.into(),
                            "Up" => Key::UpArrow.into(),
                            "Left" => Key::LeftArrow.into(),
                            "Right" => Key::RightArrow.into(),
                            other => other.into(),
                        };
                        window.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
                        window.dispatch_event(WindowEvent::KeyReleased { text: key });
                    }
                    _ => eprintln!("bloom: unknown script step {action:?}"),
                }
                // What is typed may be a password: not echoed.
                eprintln!("script: {}", if action.starts_with("type:") { "type:(text)" } else { action });
                done += 1;
            }
        });
    }
    if let Some(seconds) = std::env::var("SPIKE_SECONDS").ok().and_then(|s| s.parse().ok()) {
        slint::Timer::single_shot(Duration::from_secs(seconds), || {
            let _ = slint::quit_event_loop();
        });
    }

    app.run().expect("the window closed with an error");
    drop(runtime);
}
