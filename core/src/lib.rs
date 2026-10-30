//! Bloom without a user interface: talking to Jellyfin, the playback session, downloads,
//! settings, live sync and notifications. A UI supplies a `Shell` (the player, the desktop, the
//! channel to its own screen) and a runtime, then calls `start`.

pub mod ctx;
pub mod discord;
pub mod discovery;
pub mod downloads;
pub mod home;
mod host;
pub mod images;
pub mod jellyfin;
pub mod library;
pub mod live;
pub mod music;
pub mod notify;
pub mod playback;
pub mod rt;
pub mod settings;
pub mod shell;
pub mod trailer;

pub use ctx::Ctx;

use std::path::PathBuf;

/// Where the core keeps things.
pub struct Paths {
    /// Settings, accounts, servers, the device id and the downloads list.
    pub data: PathBuf,
    /// The artwork cache.
    pub cache: PathBuf,
    /// The user's videos folder; downloads go to a `Bloom` folder inside it unless moved.
    pub videos: PathBuf,
}

/// Builds every service, registers it in `ctx`, and starts the background work (downloads, live
/// sync). `rt::init` must have been called. Returns the settings as saved, for the UI to apply
/// what is its own (zoom, the player's volume and subtitle style).
pub fn start(ctx: &Ctx, paths: Paths, version: String) -> Result<settings::Settings, jellyfin::Error> {
    let store = settings::SettingsStore::load(&paths.data);
    let saved = store.get();
    ctx.manage(discord::Presence::new(ctx.clone(), saved.discord_presence, saved.discord_show_title));
    ctx.manage(store);
    ctx.manage(playback::Playback::new(ctx.clone(), paths.data.join("track-choices.json")));
    // The artwork cache is trimmed once a launch, off the startup path.
    let prune_from = paths.cache.clone();
    rt::spawn_blocking(move || images::prune_cache(&prune_from));
    ctx.manage(downloads::Downloads::new(ctx.clone(), &paths.data, paths.videos.join("Bloom")));
    ctx.manage(jellyfin::Jellyfin::new(paths.data, paths.cache, version)?);
    downloads::start(ctx.clone());
    live::start(ctx.clone());
    Ok(saved)
}
