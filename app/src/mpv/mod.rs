//! mpv, driven the way Bloom's Tauri player drives it (`app/src-tauri/src/player.rs`), without the
//! GTK parts: the options, the property observation, the event thread, the commands and the
//! viewport crop are the same; what differs is where the picture goes.
//!
//! The picture is drawn through mpv's render API into the window's own OpenGL framebuffer, before
//! the UI paints over it (Slint's rendering notifier). So the render calls
//! (`create_render`, `render`, `destroy_render`) belong to the thread that owns the GL context.
//! Everything else may be called from any thread.

mod sys;
mod view;

use bloom_core::playback::Track;
use bloom_core::shell::{EndReason, Event};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use sys::*;

type Listener = Box<dyn Fn(Event) + Send + Sync>;
type Wake = Box<dyn Fn() + Send + Sync>;

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

/// State the event thread needs, apart from the engine so the thread doesn't keep it alive.
struct Shared {
    listener: OnceLock<Listener>,
    stopping: AtomicBool,
}

pub struct Engine {
    mpv: Handle,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    /// mpv's render context: made and used on the GL thread only.
    render: AtomicPtr<c_void>,
    /// Asked for a redraw whenever mpv has a new frame (called on mpv's thread).
    wake: OnceLock<Wake>,
    gl: OnceLock<GlFns>,
}

/// The few GL calls that put the GL state back the way the UI toolkit expects after mpv drew.
struct GlFns {
    use_program: unsafe extern "C" fn(u32),
    bind_vertex_array: unsafe extern "C" fn(u32),
    bind_buffer: unsafe extern "C" fn(u32, u32),
    active_texture: unsafe extern "C" fn(u32),
    bind_framebuffer: unsafe extern "C" fn(u32, u32),
    // What it takes to hand mpv the neutral state it expects (see `Engine::render`).
    pixel_store: unsafe extern "C" fn(u32, i32),
    disable: unsafe extern "C" fn(u32),
    color_mask: unsafe extern "C" fn(u8, u8, u8, u8),
    depth_mask: unsafe extern "C" fn(u8),
    stencil_mask: unsafe extern "C" fn(u32),
    /// GL 3.3 and later; None where the driver has no sampler objects.
    bind_sampler: Option<unsafe extern "C" fn(u32, u32)>,
}

extern "C" fn on_update(data: *mut c_void) {
    // `data` is the engine, alive until `destroy_render` has freed the context that calls this.
    let engine = unsafe { &*(data as *const Engine) };
    if let Some(wake) = engine.wake.get() {
        wake();
    }
}

/// The graphics renderer's name (`GL_RENDERER`), once the window's context has come up.
static RENDERER: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn renderer() -> Option<&'static str> {
    RENDERER.get().map(String::as_str)
}

impl Engine {
    /// Creates mpv with Bloom's options, observes its properties and starts its event thread.
    /// No picture can show until `create_render`.
    pub fn new() -> Result<Arc<Self>, String> {
        let mpv = unsafe { mpv_create() };
        if mpv.is_null() {
            return Err("mpv_create failed".into());
        }
        let user_agent = format!("Bloom/{}", env!("CARGO_PKG_VERSION"));
        for (k, v) in [
            ("vo", "libmpv"),
            // The core sets the saved decoder before each file loads.
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
            // Subtitles stay inside the picture's box rather than spilling into the UI around it.
            ("sub-use-margins", "no"),
            ("volume-max", "100"),
            // Rendering happens on the UI thread. By default mpv asks for each frame up to 50ms
            // early and then blocks the render call until the frame's display time, which stalls
            // the whole window for most of every video frame. With no offset the update callback
            // arrives at display time and nothing waits; A/V sync is unaffected (see
            // <mpv/render.h>).
            ("video-timing-offset", "0"),
            ("user-agent", user_agent.as_str()),
            // BLOOM_MPV_LOG=/path writes mpv's own log there (it holds the stream's address, so it
            // is for chasing a problem, not for sharing).
            ("log-file", std::env::var("BLOOM_MPV_LOG").unwrap_or_default().as_str()),
            ("msg-level", if std::env::var_os("BLOOM_MPV_LOG").is_some() { "all=v" } else { "" }),
            ("audio-client-name", "Bloom"),
            // Development: BLOOM_SILENT=1 plays to no audio device at all, so it also works where
            // there is none (a container), instead of ending the file with an error.
            ("ao", if std::env::var_os("BLOOM_SILENT").is_some() { "null" } else { "" }),
        ] {
            if v.is_empty() {
                continue;
            }
            let (k, v) = (c_text(k)?, c_text(v)?);
            unsafe { mpv_set_option_string(mpv, k.as_ptr(), v.as_ptr()) };
        }
        // BLOOM_MPV_OPTS="name=value,name=value" sets more of mpv's options, last, to try a setting
        // while chasing a picture problem.
        for pair in std::env::var("BLOOM_MPV_OPTS").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            if let Some((k, v)) = pair.split_once('=') {
                let (k, v) = (c_text(k)?, c_text(v)?);
                unsafe { mpv_set_option_string(mpv, k.as_ptr(), v.as_ptr()) };
            }
        }
        if unsafe { mpv_initialize(mpv) } != 0 {
            return Err("mpv_initialize failed".into());
        }
        for (i, (name, format)) in OBSERVED.iter().enumerate() {
            let name = c_text(name)?;
            unsafe { mpv_observe_property(mpv, i as u64, name.as_ptr(), *format) };
        }

        let shared = Arc::new(Shared { listener: OnceLock::new(), stopping: AtomicBool::new(false) });
        let handle = Handle(mpv);
        let thread = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("mpv-events".into())
                .spawn(move || event_loop(handle, &shared))
                .map_err(|e| format!("mpv event thread: {e}"))?
        };
        Ok(Arc::new(Self {
            mpv: handle,
            shared,
            thread: Mutex::new(Some(thread)),
            render: AtomicPtr::new(std::ptr::null_mut()),
            wake: OnceLock::new(),
            gl: OnceLock::new(),
        }))
    }

    /// Where player events go. Set once, before anything plays.
    pub fn set_listener(&self, listener: impl Fn(Event) + Send + Sync + 'static) {
        let _ = self.shared.listener.set(Box::new(listener));
    }

    // ---- the picture (GL thread) ----

    /// Makes mpv's video output. Must run with the window's GL context current. `display` is the
    /// window system's display, which VA-API (Intel and AMD) needs to hand decoded frames to GL;
    /// without it mpv's `auto-safe` quietly decodes in software. `wake` is called on mpv's thread
    /// whenever a new frame wants drawing.
    pub fn create_render(self: &Arc<Self>, display: Option<Display>, wake: impl Fn() + Send + Sync + 'static) -> Result<(), String> {
        if !self.render.load(Ordering::Acquire).is_null() {
            return Ok(());
        }
        let gl = |name: &str| -> Result<*mut c_void, String> {
            let n = c_text(name)?;
            let p = unsafe { eglGetProcAddress(n.as_ptr()) };
            if p.is_null() {
                Err(format!("the GL driver has no {name}"))
            } else {
                Ok(p)
            }
        };
        // SAFETY: each pointer is the GL function of that name, which has that signature.
        let fns = unsafe {
            GlFns {
                use_program: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32)>(gl("glUseProgram")?),
                bind_vertex_array: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32)>(gl("glBindVertexArray")?),
                bind_buffer: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32, u32)>(gl("glBindBuffer")?),
                active_texture: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32)>(gl("glActiveTexture")?),
                bind_framebuffer: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32, u32)>(gl("glBindFramebuffer")?),
                pixel_store: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32, i32)>(gl("glPixelStorei")?),
                disable: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32)>(gl("glDisable")?),
                color_mask: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u8, u8, u8, u8)>(gl("glColorMask")?),
                depth_mask: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u8)>(gl("glDepthMask")?),
                stencil_mask: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32)>(gl("glStencilMask")?),
                bind_sampler: gl("glBindSampler").ok().map(|p| std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32, u32)>(p)),
            }
        };
        let _ = self.gl.set(fns);
        // What is drawing, for Settings → About: asked while the window's context is current.
        if let Ok(get_string) = gl("glGetString") {
            // SAFETY: glGetString(GL_RENDERER) returns a NUL-terminated string owned by the driver.
            let name = unsafe {
                let f = std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32) -> *const std::ffi::c_char>(get_string);
                let text = f(0x1F01);
                (!text.is_null()).then(|| std::ffi::CStr::from_ptr(text).to_string_lossy().into_owned())
            };
            if let Some(name) = name {
                let _ = RENDERER.set(name);
            }
        }
        let _ = self.wake.set(Box::new(wake));

        let api = c_text("opengl")?;
        let init = GlInitParams { get_proc_address, get_proc_address_ctx: std::ptr::null_mut() };
        let (display_type, display_ptr) = display.map_or((P_INVALID, std::ptr::null_mut()), |d| (d.kind, d.pointer));
        let mut params = [
            RenderParam { type_: P_API_TYPE, data: api.as_ptr() as *mut c_void },
            RenderParam { type_: P_GL_INIT, data: &init as *const _ as *mut c_void },
            // On another kind of display this is the end of the list, as the entry after it.
            RenderParam { type_: display_type, data: display_ptr },
            RenderParam { type_: P_INVALID, data: std::ptr::null_mut() },
        ];
        let mut ctx = std::ptr::null_mut();
        let rc = unsafe { mpv_render_context_create(&mut ctx, self.mpv.0, params.as_mut_ptr()) };
        if rc != 0 {
            return Err(format!("mpv_render_context_create failed: {}", error_text(rc)));
        }
        self.render.store(ctx, Ordering::Release);
        unsafe { mpv_render_context_set_update_callback(ctx, on_update, Arc::as_ptr(self) as *mut c_void) };
        Ok(())
    }

    pub fn has_render(&self) -> bool {
        !self.render.load(Ordering::Acquire).is_null()
    }

    /// Draws the current frame into the window's framebuffer, `w` by `h` physical pixels. Does
    /// nothing before `create_render`.
    pub fn render(&self, w: i32, h: i32) {
        let ctx = self.render.load(Ordering::Acquire);
        let Some(gl) = self.gl.get().filter(|_| !ctx.is_null()) else { return };
        unsafe {
            // Acknowledges the update callback, as the render API requires before rendering.
            mpv_render_context_update(ctx);
            (gl.bind_framebuffer)(0x8D40, 0);
            // The UI toolkit draws in this same context and leaves its own state behind (pixel
            // transfer layout, tests, masks, bound buffers and samplers). mpv expects the defaults
            // for what it doesn't set itself, so it is handed them.
            for (pname, value) in [(0x0CF5, 4), (0x0CF2, 0), (0x0CF3, 0), (0x0CF4, 0), (0x806E, 0), (0x806B, 0), (0x0D05, 4), (0x0D02, 0)] {
                (gl.pixel_store)(pname, value);
            }
            for cap in [0x0C11, 0x0B90, 0x0BE2, 0x0B71, 0x0B44] {
                (gl.disable)(cap);
            }
            (gl.color_mask)(1, 1, 1, 1);
            (gl.depth_mask)(1);
            (gl.stencil_mask)(0xFFFF_FFFF);
            (gl.bind_buffer)(0x88EC, 0);
            (gl.bind_buffer)(0x88EB, 0);
            if let Some(bind_sampler) = gl.bind_sampler {
                for unit in 0..8 {
                    bind_sampler(unit, 0);
                }
            }
            let fbo = GlFbo { fbo: 0, w, h, internal_format: 0 };
            let flip: c_int = 1;
            let mut params = [
                RenderParam { type_: P_GL_FBO, data: &fbo as *const _ as *mut c_void },
                RenderParam { type_: P_FLIP_Y, data: &flip as *const _ as *mut c_void },
                RenderParam { type_: P_INVALID, data: std::ptr::null_mut() },
            ];
            mpv_render_context_render(ctx, params.as_mut_ptr());
            // mpv leaves its own GL state behind; put back the neutral state the UI expects.
            (gl.use_program)(0);
            (gl.bind_vertex_array)(0);
            (gl.bind_buffer)(0x8892, 0);
            (gl.active_texture)(0x84C0);
        }
    }

    /// Frees mpv's video output, and with it the hardware decoder's device. GL context current.
    pub fn destroy_render(&self) {
        let ctx = self.render.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !ctx.is_null() {
            unsafe { mpv_render_context_free(ctx) };
        }
    }

    // ---- control (any thread) ----

    /// Play `url` from `start` seconds, sending `headers` ("Name: value") with every HTTP request
    /// mpv makes for it. Replaces whatever was playing. `audio_file`, when given, plays alongside
    /// as the audio (a trailer's separate audio stream). Returns mpv's playlist entry id for the
    /// file, which its StartFile and EndFile events carry.
    pub fn load(&self, url: &str, start: f64, headers: &[String], audio_file: Option<&str>) -> Result<Option<i64>, String> {
        let mpv = self.mpv;
        // One entry at a time: an address holds the list separator.
        mpv.command(&["change-list", "audio-files", "clr", ""])?;
        if let Some(audio) = audio_file {
            mpv.command(&["change-list", "audio-files", "append", audio])?;
        }
        // change-list appends one entry verbatim. Setting the list in one go would split a header
        // value on its commas.
        mpv.command(&["change-list", "http-header-fields", "clr", ""])?;
        for header in headers {
            mpv.command(&["change-list", "http-header-fields", "append", header])?;
        }
        mpv.set_property("start", &format!("{:.3}", start.max(0.0)))?;
        mpv.set_property("pause", "no")?;
        // aid and sid are options, so a track picked for the last file would carry over by number:
        // the wrong track, or none at all in a file with fewer tracks. Each file starts from mpv's
        // own pick, and Bloom's choice is applied once the file's tracks are known.
        mpv.set_property("aid", "auto")?;
        mpv.set_property("sid", "auto")?;
        mpv.command(&["loadfile", url, "replace"])?;
        // `replace` leaves the new file as the playlist's only entry, already numbered.
        Ok(mpv.get_property("playlist/0/id").and_then(|id| id.parse().ok()))
    }

    pub fn stop(&self) -> Result<(), String> {
        self.mpv.command(&["stop"])
    }

    pub fn toggle_pause(&self) -> Result<(), String> {
        self.mpv.command(&["cycle", "pause"])
    }

    /// Exact seeks either way: a keyframe seek can land seconds away from where someone clicked.
    pub fn seek(&self, seconds: f64, relative: bool) -> Result<(), String> {
        let target = format!("{seconds:.3}");
        self.mpv.command(&["seek", &target, if relative { "relative+exact" } else { "absolute+exact" }])
    }

    pub fn set_volume(&self, volume: f64) -> Result<(), String> {
        self.mpv.set_property("volume", &format!("{:.1}", volume.clamp(0.0, 100.0)))
    }

    /// 1 is normal. mpv keeps the audio's pitch while the speed changes.
    pub fn set_speed(&self, speed: f64) -> Result<(), String> {
        self.mpv.set_property("speed", &format!("{:.2}", speed.clamp(0.25, 4.0)))
    }

    /// mpv's `hwdec`: "auto-safe", "nvdec", "vaapi" or "no". Applies to the playing file at once.
    pub fn set_hwdec(&self, value: &str) -> Result<(), String> {
        // BLOOM_HWDEC overrides the setting, to try a decoder the settings don't list
        // (nvdec-copy, vaapi-copy) while chasing a picture problem.
        let forced = std::env::var("BLOOM_HWDEC").ok();
        self.mpv.set_property("hwdec", forced.as_deref().unwrap_or(value))
    }

    /// mpv's `sub-scale` and `sub-border-style`. Styled (ASS) subtitles take the scale (mpv's
    /// `sub-ass-override=scale`) but keep their own borders.
    pub fn set_subtitle_style(&self, scale: &str, border_style: &str) -> Result<(), String> {
        self.mpv.set_property("sub-scale", scale)?;
        self.mpv.set_property("sub-border-style", border_style)
    }

    pub fn set_muted(&self, muted: bool) -> Result<(), String> {
        self.mpv.set_property("mute", if muted { "yes" } else { "no" })
    }

    /// Select an audio (`audio`) or subtitle track by mpv id, or turn it off with None.
    pub fn set_track(&self, audio: bool, id: Option<i64>) -> Result<(), String> {
        let value = id.map_or_else(|| "no".to_string(), |id| id.to_string());
        self.mpv.set_property(if audio { "aid" } else { "sid" }, &value)
    }

    /// Load a subtitle file into the playing file as another track, without selecting it. mpv
    /// fetches it with the same HTTP headers as the video.
    pub fn add_subtitle(&self, url: &str, title: &str, lang: &str) -> Result<(), String> {
        self.mpv.command(&["sub-add", url, "auto", title, lang])
    }

    /// The playing file's tracks, read now.
    pub fn tracks(&self) -> Vec<Track> {
        read_tracks(self.mpv)
    }

    /// Confine the picture to a box, given as fractions of the window left free on each side.
    /// All zeros fills the window. A negative side is a box running past the window's edge (a page
    /// scrolled); `aspect` is the window's width over its height. See `view::crop`.
    pub fn set_viewport(&self, left: f64, top: f64, right: f64, bottom: f64, aspect: f64) -> Result<(), String> {
        let video_aspect = self.mpv.get_property("video-params/aspect").and_then(|v| v.parse::<f64>().ok());
        let view = view::crop([left, top, right, bottom], aspect, video_aspect);
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
            self.mpv.set_property(name, &format!("{value:.5}"))?;
        }
        Ok(())
    }

    /// A property as mpv words it, for diagnostics.
    pub fn property(&self, name: &str) -> Option<String> {
        self.mpv.get_property(name)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::SeqCst);
        unsafe { mpv_wakeup(self.mpv.0) };
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
        // The render context is freed by `destroy_render` with the GL context current; if that
        // never ran there is no context left to free it in, and mpv's teardown copes.
        unsafe { mpv_terminate_destroy(self.mpv.0) };
    }
}

/// The window system's display, as mpv's render API takes it.
pub struct Display {
    kind: c_int,
    pointer: *mut c_void,
}

impl Display {
    pub fn wayland(display: *mut c_void) -> Self {
        Self { kind: P_WL_DISPLAY, pointer: display }
    }
    pub fn x11(display: *mut c_void) -> Self {
        Self { kind: P_X11_DISPLAY, pointer: display }
    }
}

fn read_tracks(mpv: Handle) -> Vec<Track> {
    let count = mpv.get_property("track-list/count").and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
    (0..count)
        .filter_map(|i| {
            let field = |name: &str| mpv.get_property(&format!("track-list/{i}/{name}")).filter(|v| !v.is_empty());
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

fn event_loop(mpv: Handle, shared: &Shared) {
    loop {
        // Blocks until mpv has something; mpv owns the event until the next wait.
        let ev = unsafe { &*mpv_wait_event(mpv.0, -1.0) };
        if shared.stopping.load(Ordering::SeqCst) {
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
        if let Some(listener) = shared.listener.get() {
            // A handler that panics must not end this thread: the player would go on, and the
            // controls would silently stop following it.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener(event))).is_err() {
                eprintln!("bloom: the player's event handler panicked; carrying on");
            }
        }
    }
}

fn string_value(prop: &MpvEventProperty) -> Option<String> {
    (prop.format == FORMAT_STRING).then(|| unsafe { CStr::from_ptr(*(prop.data as *const *const c_char)) }.to_string_lossy().into_owned())
}

fn property_event(mpv: Handle, ev: &MpvEvent) -> Option<Event> {
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
