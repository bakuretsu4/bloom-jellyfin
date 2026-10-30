//! What `bloom_core` needs from this interface: the player (the mpv engine, drawn under the
//! window), staying awake, and a way for the core's events to reach the screen.

use crate::awake::Awake;
use crate::mpv::Engine;
use bloom_core::playback::Track;
use bloom_core::shell::{BoxFuture, Shell};
use std::sync::Arc;

/// Called for every event the core emits ("player:state", "sync:library", ...).
type OnEvent = Box<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub struct SlintShell {
    engine: Arc<Engine>,
    awake: Awake,
    on_event: OnEvent,
}

impl SlintShell {
    pub fn new(engine: Arc<Engine>, window: slint::Weak<crate::App>, on_event: impl Fn(&str, serde_json::Value) + Send + Sync + 'static) -> Self {
        Self { engine, awake: Awake::new(window), on_event: Box::new(on_event) }
    }
}

impl Shell for SlintShell {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        (self.on_event)(event, payload);
    }

    fn start_video(&self) -> BoxFuture<Result<(), String>> {
        // The video output is made with the window's GL context, as the window comes up (see
        // main.rs), so by the time anything plays it exists.
        let ready = self.engine.has_render();
        Box::pin(async move { if ready { Ok(()) } else { Err("the video output isn't ready".to_string()) } })
    }

    // Five minutes after playback ends (the core's timer), when nothing has played since: what
    // playback freed is handed back to the system. The video output itself stays for the life of
    // the window (the Tauri shell frees and remakes it; here it is made with the window's GL
    // context, which isn't ours to remake).
    fn release_video(&self, still_idle: Box<dyn FnOnce() -> bool + Send>) {
        if still_idle() {
            give_back_memory();
        }
    }

    fn load(&self, url: &str, start: f64, headers: &[String], audio_file: Option<&str>) -> Result<Option<i64>, String> {
        self.engine.load(url, start, headers, audio_file)
    }
    fn stop(&self) -> Result<(), String> {
        self.engine.stop()
    }
    fn toggle_pause(&self) -> Result<(), String> {
        self.engine.toggle_pause()
    }
    fn seek(&self, seconds: f64, relative: bool) -> Result<(), String> {
        self.engine.seek(seconds, relative)
    }
    fn set_volume(&self, volume: f64) -> Result<(), String> {
        self.engine.set_volume(volume)
    }
    fn set_speed(&self, speed: f64) -> Result<(), String> {
        self.engine.set_speed(speed)
    }
    fn set_muted(&self, muted: bool) -> Result<(), String> {
        self.engine.set_muted(muted)
    }
    fn set_hwdec(&self, value: &str) -> Result<(), String> {
        self.engine.set_hwdec(value)
    }
    fn set_subtitle_style(&self, scale: &str, border_style: &str) -> Result<(), String> {
        self.engine.set_subtitle_style(scale, border_style)
    }
    fn set_track(&self, audio: bool, id: Option<i64>) -> Result<(), String> {
        self.engine.set_track(audio, id)
    }
    fn add_subtitle(&self, url: &str, title: &str, lang: &str) -> Result<(), String> {
        self.engine.add_subtitle(url, title, lang)
    }
    fn tracks(&self) -> Vec<Track> {
        self.engine.tracks()
    }
    fn set_viewport(&self, left: f64, top: f64, right: f64, bottom: f64, aspect: f64) -> Result<(), String> {
        self.engine.set_viewport(left, top, right, bottom, aspect)
    }

    fn keep_awake(&self, on: bool) {
        self.awake.set(on);
    }
}

/// The resident memory of this process in megabytes, from /proc.
fn resident_mb() -> u64 {
    let pages = std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1).and_then(|p| p.parse::<u64>().ok())).unwrap_or(0);
    // SAFETY: sysconf with a valid name only reads a constant.
    let page = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap_or(4096);
    pages * page / (1024 * 1024)
}

/// glibc keeps what playback freed (mpv's demuxer cache, decoder buffers) in its arenas rather
/// than returning it: measured with the Tauri app, the process stayed about 170 MB above where
/// it started. Trimming returns it. `BLOOM_STATS` reports what it gave back.
fn give_back_memory() {
    let before = resident_mb();
    #[cfg(target_env = "gnu")]
    // SAFETY: malloc_trim only asks the allocator to return free pages; any thread may call it.
    unsafe {
        libc::malloc_trim(0);
    }
    if std::env::var_os("BLOOM_STATS").is_some() {
        eprintln!("bloom: memory given back after playback: {before} MB -> {} MB resident", resident_mb());
    }
}
