//! libmpv's C API as far as Bloom uses it (client API 2.5, mpv 0.41), and thin helpers over it.
//! Layouts are from <mpv/client.h> and <mpv/render.h>.

use std::ffi::{c_char, c_int, c_void, CStr, CString};

#[repr(C)]
pub struct RenderParam {
    pub type_: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct GlInitParams {
    pub get_proc_address: extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub get_proc_address_ctx: *mut c_void,
}
#[repr(C)]
pub struct GlFbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}
#[repr(C)]
pub struct MpvEvent {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct MpvEventProperty {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct MpvEventStartFile {
    pub playlist_entry_id: i64,
}
#[repr(C)]
pub struct MpvEventEndFile {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: c_int,
}

pub const P_INVALID: c_int = 0;
pub const P_API_TYPE: c_int = 1;
pub const P_GL_INIT: c_int = 2;
pub const P_GL_FBO: c_int = 3;
pub const P_FLIP_Y: c_int = 4;
pub const P_X11_DISPLAY: c_int = 8;
pub const P_WL_DISPLAY: c_int = 9;

pub const FORMAT_NONE: c_int = 0;
pub const FORMAT_STRING: c_int = 1;
pub const FORMAT_FLAG: c_int = 3;
pub const FORMAT_DOUBLE: c_int = 5;

pub const EVENT_SHUTDOWN: c_int = 1;
pub const EVENT_START_FILE: c_int = 6;
pub const EVENT_END_FILE: c_int = 7;
pub const EVENT_FILE_LOADED: c_int = 8;
pub const EVENT_PLAYBACK_RESTART: c_int = 21;
pub const EVENT_PROPERTY_CHANGE: c_int = 22;

pub const END_FILE_EOF: c_int = 0;
pub const END_FILE_ERROR: c_int = 4;

#[link(name = "mpv")]
unsafe extern "C" {
    pub fn mpv_create() -> *mut c_void;
    pub fn mpv_initialize(ctx: *mut c_void) -> c_int;
    pub fn mpv_set_option_string(ctx: *mut c_void, name: *const c_char, data: *const c_char) -> c_int;
    pub fn mpv_set_property_string(ctx: *mut c_void, name: *const c_char, data: *const c_char) -> c_int;
    pub fn mpv_get_property_string(ctx: *mut c_void, name: *const c_char) -> *mut c_char;
    pub fn mpv_free(data: *mut c_void);
    pub fn mpv_command(ctx: *mut c_void, args: *const *const c_char) -> c_int;
    pub fn mpv_observe_property(ctx: *mut c_void, reply_userdata: u64, name: *const c_char, format: c_int) -> c_int;
    pub fn mpv_wait_event(ctx: *mut c_void, timeout: f64) -> *mut MpvEvent;
    pub fn mpv_error_string(error: c_int) -> *const c_char;
    pub fn mpv_render_context_create(res: *mut *mut c_void, mpv: *mut c_void, p: *mut RenderParam) -> c_int;
    pub fn mpv_render_context_render(ctx: *mut c_void, p: *mut RenderParam) -> c_int;
    pub fn mpv_render_context_update(ctx: *mut c_void) -> u64;
    pub fn mpv_render_context_set_update_callback(ctx: *mut c_void, cb: extern "C" fn(*mut c_void), data: *mut c_void);
    pub fn mpv_render_context_free(ctx: *mut c_void);
    pub fn mpv_wakeup(ctx: *mut c_void);
    pub fn mpv_terminate_destroy(ctx: *mut c_void);
}

#[link(name = "EGL")]
unsafe extern "C" {
    pub fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
}

/// GL functions are resolved through EGL directly (Wayland, and X11 through EGL), so nothing
/// depends on the lifetime of the UI toolkit's own resolver.
pub extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { eglGetProcAddress(name) }
}

pub fn error_text(code: c_int) -> String {
    unsafe { CStr::from_ptr(mpv_error_string(code)) }.to_string_lossy().into_owned()
}

pub fn check(rc: c_int) -> Result<(), String> {
    if rc >= 0 {
        Ok(())
    } else {
        Err(error_text(rc))
    }
}

pub fn c_text(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| "text for the player contained a NUL byte".to_string())
}

/// The client handle. mpv's client API is thread-safe, so this may be shared freely; only the
/// render calls are bound to the thread that owns the GL context.
#[derive(Clone, Copy)]
pub struct Handle(pub *mut c_void);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    pub fn command(self, args: &[&str]) -> Result<(), String> {
        let owned = args.iter().map(|a| c_text(a)).collect::<Result<Vec<_>, _>>()?;
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        check(unsafe { mpv_command(self.0, ptrs.as_ptr()) })
    }

    pub fn set_property(self, name: &str, value: &str) -> Result<(), String> {
        let (name, value) = (c_text(name)?, c_text(value)?);
        check(unsafe { mpv_set_property_string(self.0, name.as_ptr(), value.as_ptr()) })
    }

    pub fn get_property(self, name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        let raw = unsafe { mpv_get_property_string(self.0, name.as_ptr()) };
        if raw.is_null() {
            return None;
        }
        let value = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
        unsafe { mpv_free(raw as *mut c_void) };
        Some(value)
    }
}
