//! Keeping the screen awake under X11, where a bare window manager has nothing to ask.
//!
//! On i3, bspwm or dwm there's no power service on the session bus and no Wayland compositor: the
//! screen goes dark because the X server's own blanking (and DPMS after it) fires on an idle count,
//! and because lockers like xautolock, xidlehook and xss-lock watch that same count. All of them
//! read one number, how long since the server last saw the user, so Bloom keeps it down by telling
//! the server it has been reset, every so often, while a video plays. It's what every "keep awake"
//! tool for X does, and it's a plain core request: no extension to look for, nothing to fail on.
//!
//! On a full desktop under X11 (KDE, GNOME, Xfce) the session bus already answers, and this does
//! no harm alongside it. The same approach as `app/src-tauri/src/awake/x11.rs`.

use std::cell::Cell;
use std::ffi::{c_int, c_void, CStr};
use std::time::Duration;

/// Far under the shortest blanking or locking time anyone sets, and one small request each time.
const EVERY: Duration = Duration::from_secs(20);

thread_local! {
    /// The running heartbeat, while a video plays.
    static BEATING: Cell<Option<std::rc::Rc<slint::Timer>>> = const { Cell::new(None) };
}

/// Starts or stops the heartbeat. On the UI thread, with the window's X display.
pub(super) fn set(playing: bool, display: *mut c_void) {
    let beating = BEATING.with(Cell::take);
    match (playing, beating) {
        (true, Some(timer)) => BEATING.with(|b| b.set(Some(timer))),
        (true, None) => {
            reset(display);
            // The pointer belongs to the toolkit and is only used on this thread, where the timer
            // runs; carried as a number so the closure needs nothing of X in it.
            let display = display as usize;
            let timer = std::rc::Rc::new(slint::Timer::default());
            timer.start(slint::TimerMode::Repeated, EVERY, move || reset(display as *mut c_void));
            BEATING.with(|b| b.set(Some(timer)));
        }
        (false, Some(timer)) => timer.stop(),
        (false, None) => {}
    }
}

/// Tells the X server the idle count starts again from now.
fn reset(display: *mut c_void) {
    let Some(library) = xlib() else { return };
    let (Some(reset), Some(flush)) = (symbol(library, c"XResetScreenSaver"), symbol(library, c"XFlush")) else {
        return;
    };
    // SAFETY: the toolkit's display for this thread's session, and both are plain Xlib calls on it.
    unsafe {
        reset(display);
        // Xlib holds requests back until something asks; unflushed, the reset would sit in the
        // buffer while the screen went dark.
        flush(display);
    }
}

/// The libX11 the window system backend has already loaded (opening it again only adds a
/// reference to the same copy; its symbols aren't visible process-wide, as the backend loads it
/// with its own handle). None on a system without it, which on an X11 session can't be.
fn xlib() -> Option<*mut c_void> {
    // SAFETY: dlopen by name, with a NUL-terminated name.
    let handle = unsafe { libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_LAZY) };
    (!handle.is_null()).then_some(handle)
}

fn symbol(library: *mut c_void, name: &CStr) -> Option<unsafe extern "C" fn(*mut c_void) -> c_int> {
    // SAFETY: a lookup by name in a loaded library; the cast matches Xlib's `int f(Display *)`,
    // which both of these are.
    unsafe {
        let found = libc::dlsym(library, name.as_ptr());
        (!found.is_null()).then(|| std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut c_void) -> c_int>(found))
    }
}
