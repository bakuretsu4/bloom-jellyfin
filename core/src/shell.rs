//! Everything the core needs from the user interface it runs under.
//!
//! The core owns the Jellyfin client, the playback session, downloads, settings and the rest.
//! What it can't own is the screen: the video output, the window it sits in, telling the desktop to
//! stay awake, and getting messages to whatever draws the UI. A UI implements `Shell` for those
//! and gives the core an instance in `Ctx::new`. Player events come back the other way: the UI
//! calls `playback::on_player_event` with what mpv reports.
//!
//! Every method may be called from any thread.

use crate::playback::Track;
use std::future::Future;
use std::pin::Pin;

/// How a file stopped playing.
pub enum EndReason {
    /// Reached the end.
    Finished,
    /// Stopped or replaced on purpose.
    Stopped,
    /// mpv's own description of what failed.
    Error(String),
}

/// What the player reports. The UI turns mpv's events into these.
pub enum Event {
    /// A new file started loading. The id tells its end apart from a file it replaced.
    StartFile { entry: i64 },
    FileLoaded,
    /// The first frame after loading or seeking is on its way to the screen.
    PlaybackRestart,
    EndFile { entry: i64, reason: EndReason },
    Position(f64),
    Duration(f64),
    Paused(bool),
    Buffering(bool),
    /// The hardware decoder in use, or None when decoding in software.
    Decoder(Option<String>),
    /// 0 to 100.
    Volume(f64),
    /// 1 is normal speed.
    Speed(f64),
    Muted(bool),
    /// The media time the demuxer has buffered up to.
    CacheEnd(f64),
    /// The file's tracks, including which are selected. Sent again on every track switch.
    Tracks(Vec<Track>),
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub trait Shell: Send + Sync + 'static {
    /// Delivers an event to the UI. `event` names it ("player:state", "sync:library", ...) and
    /// `payload` is its JSON.
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Makes the video output, the first time anything plays.
    fn start_video(&self) -> BoxFuture<Result<(), String>>;
    /// Gives the video output's memory back once playback has been over for a while, if
    /// `still_idle` still says nothing has started since.
    fn release_video(&self, still_idle: Box<dyn FnOnce() -> bool + Send>);

    /// Loads a file. Returns the playlist entry it became, when the player says.
    fn load(&self, url: &str, start: f64, headers: &[String], audio_file: Option<&str>) -> Result<Option<i64>, String>;
    fn stop(&self) -> Result<(), String>;
    fn toggle_pause(&self) -> Result<(), String>;
    fn seek(&self, seconds: f64, relative: bool) -> Result<(), String>;
    fn set_volume(&self, volume: f64) -> Result<(), String>;
    fn set_speed(&self, speed: f64) -> Result<(), String>;
    fn set_muted(&self, muted: bool) -> Result<(), String>;
    /// mpv's `hwdec`: "auto-safe", "nvdec", "vaapi" or "no".
    fn set_hwdec(&self, value: &str) -> Result<(), String>;
    fn set_subtitle_style(&self, scale: &str, border_style: &str) -> Result<(), String>;
    fn set_track(&self, audio: bool, id: Option<i64>) -> Result<(), String>;
    fn add_subtitle(&self, url: &str, title: &str, lang: &str) -> Result<(), String>;
    fn tracks(&self) -> Vec<Track>;
    /// Where the picture goes in the window, as edges of the box the UI leaves clear and that
    /// box's aspect ratio. All zeros gives the picture the whole window.
    fn set_viewport(&self, left: f64, top: f64, right: f64, bottom: f64, aspect: f64) -> Result<(), String>;

    /// Asks the desktop not to dim, lock or sleep while `on`.
    fn keep_awake(&self, on: bool);
}
