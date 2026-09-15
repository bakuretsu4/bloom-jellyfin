//! The player layer: mpv renders into a GtkGLArea placed *underneath* Tauri's own webview.
//!
//! First proven in a standalone Tauri + mpv spike before the app was built:
//!
//!   Tauri's GtkApplicationWindow (opaque)
//!   └── GtkOverlay                      <- inserted as the window's child
//!       ├── Tauri's GtkBox              <- moved inside the overlay
//!       │   └── GtkGLArea               <- mpv, via the libmpv render API
//!       └── Tauri's WebKitWebView       <- moved here, transparent background
//!
//! The webview's grandparent must remain the window; see `attach_inner` for why.
//!
//! Repaints are driven by mpv's update callback rather than a fixed timer, so an idle player
//! costs nothing. The picture fills the whole window unless `set_viewport` confines it to the
//! box the page leaves unpainted.
//!
//! Control functions may be called from any thread: the mpv client API is thread-safe. mpv's
//! events are read on a thread of their own and handed to the one listener registered with
//! `set_listener`, which must happen before `attach`.

use crate::playback::Track;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use webkit2gtk::WebViewExt;

#[repr(C)]
struct RenderParam {
    type_: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct GlInitParams {
    get_proc_address: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    get_proc_address_ctx: *mut c_void,
}
#[repr(C)]
struct GlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

// Layouts from <mpv/client.h>, client API 2.5 (mpv 0.41).
#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct MpvEventStartFile {
    playlist_entry_id: i64,
}
#[repr(C)]
struct MpvEventEndFile {
    reason: c_int,
    error: c_int,
    playlist_entry_id: i64,
    playlist_insert_id: i64,
    playlist_insert_num_entries: c_int,
}

const P_INVALID: c_int = 0;
const P_API_TYPE: c_int = 1;
const P_GL_INIT: c_int = 2;
const P_GL_FBO: c_int = 3;
const P_FLIP_Y: c_int = 4;
const P_X11_DISPLAY: c_int = 8;
const P_WL_DISPLAY: c_int = 9;

const FORMAT_NONE: c_int = 0;
const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_DOUBLE: c_int = 5;

const EVENT_SHUTDOWN: c_int = 1;
const EVENT_START_FILE: c_int = 6;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const END_FILE_EOF: c_int = 0;
const END_FILE_ERROR: c_int = 4;

const GL_RENDERER_ENUM: c_int = 0x1F01;
const GL_FRAMEBUFFER_BINDING: c_int = 0x8CA6;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const LC_NUMERIC: c_int = 1;

#[link(name = "mpv")]
unsafe extern "C" {
    fn mpv_create() -> *mut c_void;
    fn mpv_initialize(ctx: *mut c_void) -> c_int;
    fn mpv_set_option_string(ctx: *mut c_void, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_set_property_string(ctx: *mut c_void, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_get_property_string(ctx: *mut c_void, name: *const c_char) -> *mut c_char;
    fn mpv_free(data: *mut c_void);
    fn mpv_command(ctx: *mut c_void, args: *const *const c_char) -> c_int;
    fn mpv_observe_property(ctx: *mut c_void, reply_userdata: u64, name: *const c_char, format: c_int) -> c_int;
    fn mpv_wait_event(ctx: *mut c_void, timeout: f64) -> *mut MpvEvent;
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_render_context_create(res: *mut *mut c_void, mpv: *mut c_void, p: *mut RenderParam) -> c_int;
    fn mpv_render_context_render(ctx: *mut c_void, p: *mut RenderParam) -> c_int;
    fn mpv_render_context_update(ctx: *mut c_void) -> u64;
    fn mpv_render_context_set_update_callback(ctx: *mut c_void, cb: extern "C" fn(*mut c_void), data: *mut c_void);
    fn mpv_render_context_free(ctx: *mut c_void);
    fn mpv_wakeup(ctx: *mut c_void);
    fn mpv_terminate_destroy(ctx: *mut c_void);
}
unsafe extern "C" {
    fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
}
// GDK's own, from the libgdk-3 GTK already links. Each takes a display of its kind only.
unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut c_void) -> *mut c_void;
    fn gdk_x11_display_get_xdisplay(display: *mut c_void) -> *mut c_void;
}

/// How a file stopped playing.
pub enum EndReason {
    /// Reached the end.
    Finished,
    /// Stopped or replaced on purpose.
    Stopped,
    /// mpv's own description of what failed.
    Error(String),
}

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

/// Properties mpv reports changes for. The index is the observation's reply id.
///
/// Selected tracks come from `track-list`, not from `aid` and `sid`: mpv skips a change
/// notification for a value observed as a string when it matches the last one sent, so a new
/// file that keeps the same track numbers would never report its selection.
const OBSERVED: [(&str, c_int); 10] = [
    ("time-pos", FORMAT_DOUBLE),
    ("duration", FORMAT_DOUBLE),
    ("pause", FORMAT_FLAG),
    ("paused-for-cache", FORMAT_FLAG),
    ("hwdec-current", FORMAT_STRING),
    ("volume", FORMAT_DOUBLE),
    ("mute", FORMAT_FLAG),
    ("speed", FORMAT_DOUBLE),
    ("demuxer-cache-time", FORMAT_DOUBLE),
    // Only a change notice, sent on every change; the list is read field by field when it arrives.
    ("track-list", FORMAT_NONE),
];

type Listener = Box<dyn Fn(Event) + Send + Sync>;

static MPV: AtomicUsize = AtomicUsize::new(0);
static LISTENER: OnceLock<Listener> = OnceLock::new();
static EGL_GET_PROC: OnceLock<usize> = OnceLock::new();
static GL_RENDERER: OnceLock<String> = OnceLock::new();
static READY: AtomicBool = AtomicBool::new(false);
/// Held for reading by every call into mpv, and for writing while `restart_core` destroys one.
static CORE_LOCK: RwLock<()> = RwLock::new(());
static EVENT_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
/// Properties set for the player as a whole (volume, mute, subtitle style), set again on a fresh
/// mpv by `restart_core`.
static STICKY: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

thread_local! {
    // The GL area lives on the GTK main thread; mpv's update callback reaches it from there.
    static GL_AREA: RefCell<Option<gtk::GLArea>> = const { RefCell::new(None) };
    // mpv's render context, made by `start_video` and used only on the main thread.
    static RENDER_CTX: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) };
}

/// Makes mpv's video output, the first time something is about to play. Creating the render
/// context is when mpv loads its hardware-decoding interop, which on NVIDIA makes a CUDA context:
/// measured on the dev machine, about 100 MB (79 MB of driver mappings) held from launch when it
/// was made up front, though most of a session is browsing. Runs on GTK's main thread, which owns
/// the GL context. Always through the main thread, even once it's made: `release_video` runs
/// there too, so a play and a release happen one after the other, never across each other.
pub async fn start_video() -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    glib::MainContext::default().invoke(move || {
        let made = GL_AREA.with(|a| match a.borrow().as_ref() {
            Some(area) => create_render_context(area),
            None => Err("the player isn't running".to_string()),
        });
        let _ = tx.send(made);
    });
    rx.await.map_err(|_| "the player's window has closed".to_string())?
}

/// The window system's display, as mpv's render API takes it. VA-API (Intel and AMD) needs it to
/// hand decoded frames to GL; without it mpv's `auto-safe` quietly decodes in software. NVDEC's
/// CUDA interop doesn't use it, which is why the NVIDIA dev machine never showed the gap: found on
/// an Iris Xe laptop, where every file decoded in software (Iron Man in 4K at 250-495% CPU).
fn native_display(area: &gtk::GLArea) -> (c_int, *mut c_void) {
    use glib::object::ObjectType;
    let display = WidgetExt::display(area);
    let raw = display.as_ptr() as *mut c_void;
    match display.type_().name() {
        // SAFETY: each getter is only called on a display of its own type, which GTK keeps alive
        // for as long as the window (and so mpv's render context) exists.
        "GdkWaylandDisplay" => (P_WL_DISPLAY, unsafe { gdk_wayland_display_get_wl_display(raw) }),
        "GdkX11Display" => (P_X11_DISPLAY, unsafe { gdk_x11_display_get_xdisplay(raw) }),
        _ => (P_INVALID, std::ptr::null_mut()),
    }
}

/// On the main thread, with the GL area realized.
fn create_render_context(area: &gtk::GLArea) -> Result<(), String> {
    if !RENDER_CTX.with(Cell::get).is_null() {
        return Ok(());
    }
    let core = handle()?;
    let mpv = core.mpv;
    area.make_current();
    if let Some(e) = area.error() {
        return Err(format!("GL context error: {e}"));
    }
    let init = GlInitParams { get_proc_address, get_proc_address_ctx: std::ptr::null_mut() };
    let api = CString::new("opengl").expect("static");
    let (display_type, display) = native_display(area);
    let mut params = [
        RenderParam { type_: P_API_TYPE, data: api.as_ptr() as *mut c_void },
        RenderParam { type_: P_GL_INIT, data: &init as *const _ as *mut c_void },
        // On another kind of display this is the end of the list, as the entry after it.
        RenderParam { type_: display_type, data: display },
        RenderParam { type_: P_INVALID, data: std::ptr::null_mut() },
    ];
    let mut ctx = std::ptr::null_mut();
    if unsafe { mpv_render_context_create(&mut ctx, mpv, params.as_mut_ptr()) } != 0 {
        return Err("mpv_render_context_create failed".into());
    }
    RENDER_CTX.with(|c| c.set(ctx));
    unsafe { mpv_render_context_set_update_callback(ctx, on_mpv_update, std::ptr::null_mut()) };
    area.queue_render();
    Ok(())
}

/// Frees mpv's video output, and with it the hardware decoder's device (the CUDA context on
/// NVIDIA), once nothing has played for a while: `still_idle`, checked on the main thread just
/// before, says whether that's still so. mpv itself is replaced by a fresh one too (see
/// `restart_core`). The next play makes the video output again with `start_video`.
pub fn release_video(still_idle: impl FnOnce() -> bool + Send + 'static) {
    glib::MainContext::default().invoke(move || {
        let ctx = RENDER_CTX.with(Cell::get);
        if ctx.is_null() || !still_idle() {
            return;
        }
        GL_AREA.with(|a| {
            if let Some(area) = a.borrow().as_ref() {
                area.make_current();
            }
        });
        RENDER_CTX.with(|c| c.set(std::ptr::null_mut()));
        unsafe { mpv_render_context_free(ctx) };
        let restarting = std::time::Instant::now();
        restart_core();
        eprintln!("bloom: player restarted after playback in {:?}", restarting.elapsed());
        // What playback used (mpv's demuxer cache, decoder buffers) is freed by now, but glibc
        // keeps freed memory in its arenas rather than giving it back: measured on the dev machine,
        // the main process stayed about 170 MB above where it started. Trimming returns it.
        #[cfg(target_env = "gnu")]
        unsafe {
            libc::malloc_trim(0);
        }
        GL_AREA.with(|a| {
            if let Some(area) = a.borrow().as_ref() {
                area.queue_render();
            }
        });
    });
}

pub fn gl_renderer() -> Option<String> {
    GL_RENDERER.get().cloned()
}

pub fn ready() -> bool {
    READY.load(Ordering::Relaxed)
}

pub fn set_listener(listener: impl Fn(Event) + Send + Sync + 'static) {
    let _ = LISTENER.set(Box::new(listener));
}

/// The running mpv, held for the length of a call: a restart waits for calls in progress, and a
/// call made while one happens fails as if the player weren't running.
struct Core {
    _held: RwLockReadGuard<'static, ()>,
    mpv: *mut c_void,
}

fn handle() -> Result<Core, String> {
    let held = CORE_LOCK.read().unwrap_or_else(|e| e.into_inner());
    match MPV.load(Ordering::Relaxed) {
        0 => Err("the player isn't running".into()),
        raw => Ok(Core { _held: held, mpv: raw as *mut c_void }),
    }
}

/// Remembers a player-wide property for `restart_core`.
fn sticky(name: &str, value: &str) {
    let mut kept = STICKY.lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|(kept_name, _)| kept_name != name);
    kept.push((name.to_string(), value.to_string()));
}

fn error_text(code: c_int) -> String {
    unsafe { CStr::from_ptr(mpv_error_string(code)) }.to_string_lossy().into_owned()
}

fn check(rc: c_int) -> Result<(), String> {
    if rc >= 0 {
        Ok(())
    } else {
        Err(error_text(rc))
    }
}

fn c_text(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| "text for the player contained a NUL byte".to_string())
}

fn command(mpv: *mut c_void, args: &[&str]) -> Result<(), String> {
    let owned = args.iter().map(|a| c_text(a)).collect::<Result<Vec<_>, _>>()?;
    let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    check(unsafe { mpv_command(mpv, ptrs.as_ptr()) })
}

fn set_property(mpv: *mut c_void, name: &str, value: &str) -> Result<(), String> {
    let (name, value) = (c_text(name)?, c_text(value)?);
    check(unsafe { mpv_set_property_string(mpv, name.as_ptr(), value.as_ptr()) })
}

fn get_property(mpv: *mut c_void, name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    let raw = unsafe { mpv_get_property_string(mpv, name.as_ptr()) };
    if raw.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    unsafe { mpv_free(raw as *mut c_void) };
    Some(value)
}

/// Play `url` from `start` seconds, sending `headers` ("Name: value") with every HTTP request mpv
/// makes for it. Replaces whatever was playing.
/// `audio_file`, when given, plays alongside as the audio (a trailer's separate audio stream).
/// Returns mpv's playlist entry id for the file, which its StartFile and EndFile events carry.
pub fn load(url: &str, start: f64, headers: &[String], audio_file: Option<&str>) -> Result<Option<i64>, String> {
    let core = handle()?;
    let mpv = core.mpv;
    // One entry at a time: an address holds the list separator.
    command(mpv, &["change-list", "audio-files", "clr", ""])?;
    if let Some(audio) = audio_file {
        command(mpv, &["change-list", "audio-files", "append", audio])?;
    }
    // change-list appends one entry verbatim. Setting the list in one go would split a header
    // value on its commas.
    command(mpv, &["change-list", "http-header-fields", "clr", ""])?;
    for header in headers {
        command(mpv, &["change-list", "http-header-fields", "append", header])?;
    }
    set_property(mpv, "start", &format!("{:.3}", start.max(0.0)))?;
    set_property(mpv, "pause", "no")?;
    // aid and sid are options, so a track picked for the last file would carry over by number:
    // the wrong track, or none at all in a file with fewer tracks. Each file starts from mpv's
    // own pick, and Bloom's choice is applied once the file's tracks are known.
    set_property(mpv, "aid", "auto")?;
    set_property(mpv, "sid", "auto")?;
    command(mpv, &["loadfile", url, "replace"])?;
    // `replace` leaves the new file as the playlist's only entry, already numbered.
    Ok(get_property(mpv, "playlist/0/id").and_then(|id| id.parse().ok()))
}

pub fn stop() -> Result<(), String> {
    command(handle()?.mpv, &["stop"])
}

pub fn toggle_pause() -> Result<(), String> {
    command(handle()?.mpv, &["cycle", "pause"])
}

/// Exact seeks either way: a keyframe seek can land seconds away from where someone clicked.
pub fn seek(seconds: f64, relative: bool) -> Result<(), String> {
    let target = format!("{seconds:.3}");
    command(handle()?.mpv, &["seek", &target, if relative { "relative+exact" } else { "absolute+exact" }])
}

pub fn set_volume(volume: f64) -> Result<(), String> {
    let value = format!("{:.1}", volume.clamp(0.0, 100.0));
    sticky("volume", &value);
    set_property(handle()?.mpv, "volume", &value)
}

/// 1 is normal. mpv keeps the audio's pitch while the speed changes.
pub fn set_speed(speed: f64) -> Result<(), String> {
    set_property(handle()?.mpv, "speed", &format!("{:.2}", speed.clamp(0.25, 4.0)))
}

/// mpv's `hwdec`: "auto-safe", "nvdec", "vaapi" or "no". Applies to the playing file at once.
pub fn set_hwdec(value: &str) -> Result<(), String> {
    set_property(handle()?.mpv, "hwdec", value)
}

/// mpv's `sub-scale` and `sub-border-style`, from Settings. Styled (ASS) subtitles take the scale
/// (mpv's `sub-ass-override=scale`) but keep their own borders.
pub fn set_subtitle_style(scale: &str, border_style: &str) -> Result<(), String> {
    sticky("sub-scale", scale);
    sticky("sub-border-style", border_style);
    let core = handle()?;
    let mpv = core.mpv;
    set_property(mpv, "sub-scale", scale)?;
    set_property(mpv, "sub-border-style", border_style)
}

pub fn set_muted(muted: bool) -> Result<(), String> {
    let value = if muted { "yes" } else { "no" };
    sticky("mute", value);
    set_property(handle()?.mpv, "mute", value)
}

/// Select an audio (`audio`) or subtitle track by mpv id, or turn it off with None.
pub fn set_track(audio: bool, id: Option<i64>) -> Result<(), String> {
    let value = id.map_or_else(|| "no".to_string(), |id| id.to_string());
    set_property(handle()?.mpv, if audio { "aid" } else { "sid" }, &value)
}

/// Load a subtitle file into the playing file as another track, without selecting it. mpv fetches
/// it with the same HTTP headers as the video.
pub fn add_subtitle(url: &str, title: &str, lang: &str) -> Result<(), String> {
    command(handle()?.mpv, &["sub-add", url, "auto", title, lang])
}

/// The playing file's tracks, read now.
pub fn tracks() -> Vec<Track> {
    handle().map(|core| read_tracks(core.mpv)).unwrap_or_default()
}

/// Confine the picture to a box, given as fractions of the window left free on each side.
/// All zeros fills the window. A negative side is a box running past the window's edge (the
/// watch page scrolled); `aspect` is the window's width over its height. See `crop`.
pub fn set_viewport(left: f64, top: f64, right: f64, bottom: f64, aspect: f64) -> Result<(), String> {
    let core = handle()?;
    let mpv = core.mpv;
    let video_aspect = get_property(mpv, "video-params/aspect").and_then(|v| v.parse::<f64>().ok());
    let view = crop([left, top, right, bottom], aspect, video_aspect);
    let [l, t, r, b] = view.margins;
    for (name, value) in [
        ("video-margin-ratio-left", l),
        ("video-margin-ratio-top", t),
        ("video-margin-ratio-right", r),
        ("video-margin-ratio-bottom", b),
        ("video-zoom", view.zoom),
        ("video-pan-x", view.pan_x),
        ("video-pan-y", view.pan_y),
    ] {
        set_property(mpv, name, &format!("{value:.5}"))?;
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
struct Crop {
    /// Left, top, right, bottom, as mpv takes them: within the window.
    margins: [f64; 4],
    /// log2 of the scale, as mpv's video-zoom.
    zoom: f64,
    /// Fractions of the scaled picture's size, as mpv's video-pan.
    pan_x: f64,
    pan_y: f64,
}

/// mpv's margins stop at the window's edge, so a box partly outside the window is drawn by
/// fitting the picture to the part inside, then zooming and panning it back to the size and
/// place of the whole box; the rest falls outside the window. A box wholly outside shrinks the
/// picture to nothing, so it can't show anywhere the page isn't painted.
fn crop(sides: [f64; 4], window_aspect: f64, video_aspect: Option<f64>) -> Crop {
    let [l, t, r, b] = sides.map(|v| if v.is_finite() { v } else { 0.0 });
    let margins = [l, t, r, b].map(|v| v.clamp(0.0, 0.95));
    let plain = Crop { margins, zoom: 0.0, pan_x: 0.0, pan_y: 0.0 };
    if margins == [l, t, r, b] {
        return plain;
    }
    if 1.0 - l.max(0.0) - r.max(0.0) <= 0.0 || 1.0 - t.max(0.0) - b.max(0.0) <= 0.0 {
        return Crop { zoom: -20.0, ..plain };
    }
    let Some(video) = video_aspect.filter(|a| a.is_finite() && *a > 0.0) else { return plain };
    let w = if window_aspect.is_finite() && window_aspect > 0.0 { window_aspect } else { 16.0 / 9.0 };
    // Measured in a window `w` wide and 1 high.
    let (box_w, box_h) = ((1.0 - l - r) * w, 1.0 - t - b);
    let (area_w, area_h) = ((1.0 - margins[0] - margins[2]) * w, 1.0 - margins[1] - margins[3]);
    if box_w <= 0.0 || box_h <= 0.0 || area_w <= 0.0 || area_h <= 0.0 {
        return plain;
    }
    // The picture's width when fitted into an area, keeping its shape.
    let fitted = |aw: f64, ah: f64| if aw / ah > video { ah * video } else { aw };
    let (picture_w, area_picture_w) = (fitted(box_w, box_h), fitted(area_w, area_h));
    let picture_h = picture_w / video;
    let centre = |start: f64, size: f64| start + size / 2.0;
    Crop {
        margins,
        zoom: (picture_w / area_picture_w).log2(),
        pan_x: ((centre(l * w, box_w) - centre(margins[0] * w, area_w)) / picture_w).clamp(-3.0, 3.0),
        pan_y: ((centre(t, box_h) - centre(margins[1], area_h)) / picture_h).clamp(-3.0, 3.0),
    }
}

fn read_tracks(mpv: *mut c_void) -> Vec<Track> {
    let count = get_property(mpv, "track-list/count").and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
    (0..count)
        .filter_map(|i| {
            let field = |name: &str| get_property(mpv, &format!("track-list/{i}/{name}")).filter(|v| !v.is_empty());
            let flag = |name: &str| field(name).as_deref() == Some("yes");
            Some(Track {
                id: field("id")?.parse().ok()?,
                kind: field("type")?,
                ff_index: field("ff-index").and_then(|v| v.parse().ok()),
                title: field("title"),
                lang: field("lang"),
                codec: field("codec"),
                channels: field("demux-channel-count").and_then(|v| v.parse().ok()),
                width: field("demux-w").and_then(|v| v.parse().ok()),
                height: field("demux-h").and_then(|v| v.parse().ok()),
                fps: field("demux-fps").and_then(|v| v.parse().ok()),
                default: flag("default"),
                forced: flag("forced"),
                external: flag("external"),
                external_filename: field("external-filename"),
                hearing_impaired: flag("hearing-impaired"),
                selected: flag("selected"),
            })
        })
        .collect()
}

fn event_loop(mpv: usize) {
    let mpv = mpv as *mut c_void;
    loop {
        // Blocks until mpv has something; mpv owns the event until the next wait.
        let ev = unsafe { &*mpv_wait_event(mpv, -1.0) };
        // Retired by `restart_core`, which woke this wait: this mpv is about to go.
        if MPV.load(Ordering::Relaxed) != mpv as usize {
            return;
        }
        let event = match ev.event_id {
            EVENT_SHUTDOWN => return,
            EVENT_START_FILE => {
                let data = unsafe { &*(ev.data as *const MpvEventStartFile) };
                Event::StartFile { entry: data.playlist_entry_id }
            }
            EVENT_FILE_LOADED => Event::FileLoaded,
            EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
            EVENT_END_FILE => {
                let data = unsafe { &*(ev.data as *const MpvEventEndFile) };
                let reason = match data.reason {
                    END_FILE_EOF => EndReason::Finished,
                    END_FILE_ERROR => EndReason::Error(error_text(data.error)),
                    _ => EndReason::Stopped,
                };
                Event::EndFile { entry: data.playlist_entry_id, reason }
            }
            EVENT_PROPERTY_CHANGE => match property_event(mpv, ev) {
                Some(event) => event,
                None => continue,
            },
            _ => continue,
        };
        if let Some(listener) = LISTENER.get() {
            listener(event);
        }
    }
}

fn string_value(prop: &MpvEventProperty) -> Option<String> {
    (prop.format == FORMAT_STRING)
        .then(|| unsafe { CStr::from_ptr(*(prop.data as *const *const c_char)) }.to_string_lossy().into_owned())
}

fn property_event(mpv: *mut c_void, ev: &MpvEvent) -> Option<Event> {
    let (name, _) = OBSERVED.get(ev.reply_userdata as usize)?;
    let prop = unsafe { &*(ev.data as *const MpvEventProperty) };
    let double = || unsafe { *(prop.data as *const f64) };
    let flag = || unsafe { *(prop.data as *const c_int) != 0 };
    match (*name, prop.format) {
        ("track-list", _) => Some(Event::Tracks(read_tracks(mpv))),
        // Unavailable: no file loaded, or no hardware decoder in use.
        ("hwdec-current", FORMAT_NONE) => Some(Event::Decoder(None)),
        (_, FORMAT_NONE) => None,
        ("time-pos", FORMAT_DOUBLE) => Some(Event::Position(double())),
        ("duration", FORMAT_DOUBLE) => Some(Event::Duration(double())),
        ("demuxer-cache-time", FORMAT_DOUBLE) => Some(Event::CacheEnd(double())),
        ("volume", FORMAT_DOUBLE) => Some(Event::Volume(double())),
        ("speed", FORMAT_DOUBLE) => Some(Event::Speed(double())),
        ("pause", FORMAT_FLAG) => Some(Event::Paused(flag())),
        ("mute", FORMAT_FLAG) => Some(Event::Muted(flag())),
        ("paused-for-cache", FORMAT_FLAG) => Some(Event::Buffering(flag())),
        ("hwdec-current", FORMAT_STRING) => {
            let value = string_value(prop).unwrap_or_default();
            Some(Event::Decoder((!value.is_empty() && value != "no").then_some(value)))
        }
        _ => None,
    }
}

extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let Some(&raw) = EGL_GET_PROC.get() else { return std::ptr::null_mut() };
    let f: unsafe extern "C" fn(*const c_char) -> *mut c_void = unsafe { std::mem::transmute(raw) };
    unsafe { f(name) }
}

fn gl_fn(name: &str) -> *mut c_void {
    let n = CString::new(name).expect("static GL symbol name");
    get_proc_address(std::ptr::null_mut(), n.as_ptr())
}

extern "C" fn on_mpv_update(_data: *mut c_void) {
    // Called on mpv's own thread. Never touch GTK here: hop to the main thread first.
    glib::idle_add_once(|| {
        GL_AREA.with(|a| {
            if let Some(area) = a.borrow().as_ref() {
                area.queue_render();
            }
        });
    });
}

/// Creates mpv with Bloom's options, observes its properties and starts its event thread.
fn create_core() -> Result<(), String> {
    let mpv = unsafe { mpv_create() };
    if mpv.is_null() {
        return Err("mpv_create failed".into());
    }
    let user_agent = format!("Bloom/{}", env!("CARGO_PKG_VERSION"));
    for (k, v) in [
        ("vo", "libmpv"),
        // Playback sets the saved decoder before each file loads (see `start_video` for why
        // nothing about video is set up before then).
        ("hwdec", "no"),
        ("terminal", "no"),
        // Stay alive between files; Bloom decides what plays next.
        ("idle", "yes"),
        ("keep-open", "no"),
        // Nothing from the user's own mpv setup: no config, scripts, key bindings or yt-dlp.
        ("config", "no"),
        ("load-scripts", "no"),
        ("input-default-bindings", "no"),
        ("ytdl", "no"),
        // And none of mpv's built-in Lua scripts, which load-scripts doesn't cover: each runs
        // its own interpreter thread, and Bloom draws every control itself.
        ("osc", "no"),
        ("load-stats-overlay", "no"),
        ("load-console", "no"),
        ("load-commands", "no"),
        ("load-select", "no"),
        ("load-positioning", "no"),
        ("load-context-menu", "no"),
        ("load-auto-profiles", "no"),
        ("input-builtin-bindings", "no"),
        ("input-builtin-dragging", "no"),
        // No on-screen messages or seek bar of mpv's own; subtitles are unaffected.
        ("osd-level", "0"),
        ("osd-bar", "no"),
        // Subtitles stay inside the picture's box rather than spilling into the page around it.
        ("sub-use-margins", "no"),
        ("volume-max", "100"),
        // Rendering happens on GTK's main thread. By default mpv asks for each frame up to 50ms
        // early and then blocks the render call until the frame's display time, which stalls the
        // whole window for most of every video frame. With no offset the update callback arrives
        // at display time and nothing waits; A/V sync is unaffected (see <mpv/render.h>).
        ("video-timing-offset", "0"),
        ("user-agent", user_agent.as_str()),
        ("audio-client-name", "Bloom"),
    ] {
        let (k, v) = (CString::new(k).expect("static"), CString::new(v).expect("no NUL"));
        unsafe { mpv_set_option_string(mpv, k.as_ptr(), v.as_ptr()) };
    }
    if unsafe { mpv_initialize(mpv) } != 0 {
        return Err("mpv_initialize failed".into());
    }

    for (i, (name, format)) in OBSERVED.iter().enumerate() {
        let name = CString::new(*name).expect("static");
        unsafe { mpv_observe_property(mpv, i as u64, name.as_ptr(), *format) };
    }
    MPV.store(mpv as usize, Ordering::Relaxed);
    let raw = mpv as usize;
    let thread = std::thread::Builder::new()
        .name("mpv-events".into())
        .spawn(move || event_loop(raw))
        .map_err(|e| format!("mpv event thread: {e}"))?;
    *EVENT_THREAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(thread);
    Ok(())
}

/// Replaces mpv with a fresh one, once nothing is playing. mpv keeps a finished file's demuxer
/// cache allocated (measured on the dev machine: 189 MB still in use after a release with the
/// default 150 MB cache, 52 MB with a 16 MB one), and only destroying it gives that back. New
/// calls fail while it's replaced, calls in progress finish first (`CORE_LOCK`), and the event
/// thread is woken and joined before the old mpv goes. On GTK's main thread, like `start_video`.
fn restart_core() {
    let old = MPV.swap(0, Ordering::Relaxed) as *mut c_void;
    if old.is_null() {
        return;
    }
    unsafe { mpv_wakeup(old) };
    let thread = EVENT_THREAD.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(thread) = thread {
        let _ = thread.join();
    }
    {
        let _exclusive = CORE_LOCK.write().unwrap_or_else(|e| e.into_inner());
        unsafe { mpv_terminate_destroy(old) };
    }
    if let Err(e) = create_core() {
        eprintln!("bloom: couldn't start the player again: {e}");
        return;
    }
    let kept = STICKY.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Ok(core) = handle() {
        for (name, value) in &kept {
            let _ = set_property(core.mpv, name, value);
        }
    }
}

/// Insert the player beneath the main window's webview. Must be called from `setup`.
pub fn attach(win: &tauri::WebviewWindow) -> tauri::Result<()> {
    win.with_webview(|pw| {
        if let Err(e) = attach_inner(pw.inner()) {
            eprintln!("bloom: player unavailable: {e}");
        }
    })
}

fn attach_inner(webview: webkit2gtk::WebView) -> Result<(), String> {
    // libmpv refuses to run under a non-C numeric locale, and GTK sets the user's.
    unsafe {
        let c = CString::new("C").expect("static");
        setlocale(LC_NUMERIC, c.as_ptr());
    }

    let egl = unsafe { libloading::Library::new("libEGL.so.1") }.map_err(|e| format!("libEGL: {e}"))?;
    let sym: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_void> =
        unsafe { egl.get(b"eglGetProcAddress\0") }.map_err(|e| format!("eglGetProcAddress: {e}"))?;
    let _ = EGL_GET_PROC.set(unsafe { sym.into_raw().into_raw() } as usize);
    std::mem::forget(egl); // the resolved symbol must outlive this function

    // Put the GL area beneath the webview WITHOUT changing how far up the window is.
    // tauri-runtime-wry attaches click and touch handlers to the webview on Linux
    // (undecorated_resizing.rs) that walk webview -> parent -> parent and force-unwrap the
    // result as a gtk::Window. They run inside a GTK signal, so a deeper hierarchy aborts the
    // whole process on the first left click.
    //
    //   before: Window -> tao's GtkBox -> WebView
    //   after:  Window -> GtkOverlay -> { main: tao's GtkBox -> GLArea, overlay: WebView }
    let vbox = webview
        .parent()
        .and_then(|p| p.downcast::<gtk::Box>().ok())
        .ok_or("webview is not inside tao's GtkBox")?;
    let window = vbox
        .parent()
        .and_then(|p| p.downcast::<gtk::Window>().ok())
        .ok_or("tao's GtkBox is not the window's direct child")?;

    vbox.remove(&webview);
    window.remove(&vbox);

    let overlay = gtk::Overlay::new();
    let area = gtk::GLArea::new();
    area.set_has_depth_buffer(false);
    area.set_has_stencil_buffer(false);
    // Render only when mpv's update callback queues it. Left on, GTK would also render the video
    // every time the page above it draws, doubling the main thread's work during playback.
    area.set_auto_render(false);
    vbox.pack_start(&area, true, true, 0);
    overlay.add(&vbox);
    overlay.add_overlay(&webview);
    window.add(&overlay);

    // Re-check the invariant Tauri relies on, so a future change fails here with a message
    // rather than aborting the app on the user's first click.
    let grandparent_is_window = webview
        .parent()
        .and_then(|p| p.parent())
        .map(|w| w.is::<gtk::Window>())
        .unwrap_or(false);
    if !grandparent_is_window {
        return Err("webview's grandparent is no longer the window; Tauri's resize handlers would abort".into());
    }
    // Widget-level transparency only. The window itself stays opaque, which avoids the
    // stale-repaint bug that OS-window transparency hits on Linux.
    webview.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));

    create_core()?;

    // Only the GL context and the GPU's name here; mpv's render context waits for `start_video`.
    area.connect_realize(move |area| {
        area.make_current();
        if let Some(e) = area.error() {
            eprintln!("bloom: GL context error: {e}");
            return;
        }
        let get_string: unsafe extern "C" fn(c_int) -> *const c_char = unsafe { std::mem::transmute(gl_fn("glGetString")) };
        let renderer = unsafe { get_string(GL_RENDERER_ENUM) };
        if !renderer.is_null() {
            let _ = GL_RENDERER.set(unsafe { CStr::from_ptr(renderer) }.to_string_lossy().into_owned());
        }
        READY.store(true, Ordering::Relaxed);
    });

    {
        area.connect_render(move |area, _| {
            let ctx = RENDER_CTX.with(Cell::get);
            if ctx.is_null() {
                // Nothing has played yet: black, rather than whatever the buffer held.
                let clear_color = gl_fn("glClearColor");
                let clear = gl_fn("glClear");
                if !clear_color.is_null() && !clear.is_null() {
                    let clear_color: unsafe extern "C" fn(f32, f32, f32, f32) = unsafe { std::mem::transmute(clear_color) };
                    let clear: unsafe extern "C" fn(u32) = unsafe { std::mem::transmute(clear) };
                    unsafe {
                        clear_color(0.0, 0.0, 0.0, 1.0);
                        clear(GL_COLOR_BUFFER_BIT);
                    }
                }
                return glib::Propagation::Proceed;
            }
            // Acknowledge the update callback, as the render API requires before rendering.
            unsafe { mpv_render_context_update(ctx) };
            area.attach_buffers();
            let get_int: unsafe extern "C" fn(c_int, *mut c_int) = unsafe { std::mem::transmute(gl_fn("glGetIntegerv")) };
            let mut fbo_id = 0;
            unsafe { get_int(GL_FRAMEBUFFER_BINDING, &mut fbo_id) };
            let scale = area.scale_factor();
            let fbo = GlFbo {
                fbo: fbo_id,
                w: area.allocated_width() * scale,
                h: area.allocated_height() * scale,
                internal_format: 0,
            };
            let flip: c_int = 1;
            let mut params = [
                RenderParam { type_: P_GL_FBO, data: &fbo as *const _ as *mut c_void },
                RenderParam { type_: P_FLIP_Y, data: &flip as *const _ as *mut c_void },
                RenderParam { type_: P_INVALID, data: std::ptr::null_mut() },
            ];
            unsafe { mpv_render_context_render(ctx, params.as_mut_ptr()) };
            glib::Propagation::Proceed
        });
    }

    GL_AREA.with(|a| *a.borrow_mut() = Some(area.clone()));

    // Only now: the window is already mapped, so show_all() realizes the GL area on the spot,
    // and `realize` must already have a handler or mpv never gets a context.
    overlay.show_all();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f64 = 16.0 / 9.0;

    #[test]
    fn a_box_inside_the_window_is_only_margins() {
        let view = crop([0.1, 0.1, 0.1, 0.2], WIDE, Some(WIDE));
        assert_eq!(view, Crop { margins: [0.1, 0.1, 0.1, 0.2], zoom: 0.0, pan_x: 0.0, pan_y: 0.0 });
    }

    #[test]
    fn a_box_scrolled_past_the_top_is_zoomed_and_panned_back_into_place() {
        // A full-width 16:9 box, a quarter of the window's height above the top edge.
        let view = crop([0.0, -0.25, 0.0, 0.25], WIDE, Some(WIDE));
        assert_eq!(view.margins, [0.0, 0.0, 0.0, 0.25]);
        // Fitted to the visible 0.75, the picture is 0.75 high; the box is 1.0 high.
        assert!((2f64.powf(view.zoom) - 1.0 / 0.75).abs() < 1e-9, "zoom {}", view.zoom);
        // Its centre moves from 0.375 (the visible part's) to 0.25 (the box's): an eighth of its height.
        assert!((view.pan_y + 0.125).abs() < 1e-9, "pan {}", view.pan_y);
        assert_eq!(view.pan_x, 0.0);
    }

    #[test]
    fn a_box_wholly_above_the_window_hides_the_picture() {
        assert_eq!(crop([0.0, -1.5, 0.0, 1.2], WIDE, Some(WIDE)).zoom, -20.0);
    }

    #[test]
    fn without_a_picture_yet_there_is_nothing_to_zoom() {
        assert_eq!(crop([0.0, -0.25, 0.0, 0.25], WIDE, None).zoom, 0.0);
    }
}
