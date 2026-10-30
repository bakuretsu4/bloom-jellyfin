//! Keeping the screen awake while a video plays, over the session bus.
//!
//! mpv would normally ask the desktop itself, but here it draws into Bloom's own window through
//! the render API, so it has no window and never speaks to the compositor. Bloom asks instead, and
//! lets go as soon as playback pauses or stops, which is what a desktop's own players do.
//!
//! No single way of asking reaches every Linux desktop, so Bloom uses all three that matter, each
//! best-effort and none of them required (as `app/src-tauri/src/awake.rs` does):
//!
//! - **The session bus**, below: `org.freedesktop.ScreenSaver` (KDE, GNOME through
//!   gnome-settings-daemon, Xfce) keeps the screen lit and unlocked, and
//!   `org.freedesktop.PowerManagement.Inhibit` (KDE's PowerDevil, Xfce) keeps the machine from
//!   suspending, which nothing else does.
//! - **The compositor**, [`wayland`]: `zwp_idle_inhibit_manager_v1`, for sway, Hyprland, river.
//! - **The X server**, [`x11`]: the idle count every X locker and blanker reads, for bare window
//!   managers like i3, bspwm and dwm.
//!
//! A desktop that answers none of them is left alone; the film still plays.

mod wayland;
mod x11;

use crate::App;
use slint::{ComponentHandle, Weak};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// Shown by desktops that list what is keeping the screen on.
const REASON: &str = "Playing a video";

/// How long the desktop has to answer before a request is given up on. The worker is its own
/// thread, so nothing of Bloom's waits on this.
const REPLY: Duration = Duration::from_secs(5);

/// The services asked to hold off. Each takes `Inhibit(application, reason) -> cookie` and
/// `UnInhibit(cookie)` on an interface of its own name.
const SERVICES: [(&str, &str); 2] = [
    ("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver"),
    ("org.freedesktop.PowerManagement.Inhibit", "/org/freedesktop/PowerManagement/Inhibit"),
];

pub struct Awake {
    /// The worker, made on the first request.
    asks: Mutex<Option<Sender<bool>>>,
    /// The window, for the compositor's and the X server's ways of asking, which are the window's.
    window: Weak<App>,
}

impl Awake {
    pub fn new(window: Weak<App>) -> Self {
        Self { asks: Mutex::new(None), window }
    }

    /// True while a video is playing: the screen should stay on. False on a pause or a stop, when
    /// the desktop may do as it usually would. Safe to repeat; only changes are passed on.
    pub fn set(&self, playing: bool) {
        self.on_the_window(playing);
        let mut asks = self.asks.lock().unwrap_or_else(|e| e.into_inner());
        if asks.is_none() {
            // Nothing is asked of anything until a video actually plays.
            if !playing {
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            // The connection is the inhibition's owner: both services let go by themselves if it
            // closes, so it's held for as long as Bloom runs, and Bloom quitting mid-film leaves
            // nothing behind.
            match std::thread::Builder::new().name("bloom-awake".into()).spawn(move || serve(rx)) {
                Ok(_) => *asks = Some(tx),
                Err(e) => {
                    eprintln!("bloom: couldn't ask the desktop to stay awake: {e}");
                    return;
                }
            }
        }
        if let Some(tx) = asks.as_ref() {
            // The worker only stops when Bloom does, so a failure here isn't worth reporting.
            let _ = tx.send(playing);
        }
    }
}

impl Awake {
    /// The display server's own ways of asking, alongside the session bus. Both belong to the UI
    /// thread, where the window is and where its surface pointer is good.
    fn on_the_window(&self, playing: bool) {
        let _ = self.window.upgrade_in_event_loop(move |app| {
            use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
            let handle = app.window().window_handle();
            let (Ok(display), Ok(window)) = (handle.display_handle(), handle.window_handle()) else { return };
            match (display.as_raw(), window.as_raw()) {
                (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(window)) => wayland::set(playing, display.display.as_ptr(), window.surface.as_ptr()),
                (RawDisplayHandle::Xlib(display), _) => {
                    if let Some(display) = display.display {
                        x11::set(playing, display.as_ptr());
                    }
                }
                _ => {}
            }
        });
    }
}

/// Holds one session-bus connection and keeps the two services in step with what's asked of it.
fn serve(asked: Receiver<bool>) {
    let connection = match dbus::blocking::Connection::new_session() {
        Ok(connection) => connection,
        Err(e) => {
            eprintln!("bloom: no session bus, so the screen may sleep while watching: {e}");
            // Draining leaves `set` free to keep sending into nothing.
            while asked.recv().is_ok() {}
            return;
        }
    };
    let mut held: [Option<u32>; SERVICES.len()] = [None; SERVICES.len()];
    // A desktop without one of these services would otherwise be told off on every play; it's
    // still asked each time, since a power manager may be started later in the session.
    let mut said: [bool; SERVICES.len()] = [false; SERVICES.len()];
    while let Ok(playing) = asked.recv() {
        // A pause and an unpause in the same moment are one change, not two round trips.
        let playing = asked.try_iter().last().unwrap_or(playing);
        for ((held, said), (service, path)) in held.iter_mut().zip(said.iter_mut()).zip(SERVICES) {
            let proxy = connection.with_proxy(service, path, REPLY);
            match (playing, *held) {
                (true, None) => match proxy.method_call(service, "Inhibit", ("Bloom", REASON)) {
                    Ok((cookie,)) => *held = Some(cookie),
                    // Usually the desktop doesn't offer this service at all: said once.
                    Err(e) => {
                        if !std::mem::replace(said, true) {
                            eprintln!("bloom: {service} didn't take Bloom's request to stay awake: {e}");
                        }
                    }
                },
                (false, Some(cookie)) => {
                    let released: Result<(), _> = proxy.method_call(service, "UnInhibit", (cookie,));
                    if let Err(e) = released {
                        eprintln!("bloom: {service} wasn't let go of: {e}");
                    }
                    // Kept out of the way either way: a cookie it has forgotten is no use, and the
                    // connection closing releases whatever is left.
                    *held = None;
                }
                _ => {}
            }
        }
    }
}
