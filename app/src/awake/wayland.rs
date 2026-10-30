//! Keeping the screen awake the Wayland way: `zwp_idle_inhibit_manager_v1`.
//!
//! The session bus (see the parent module) covers desktops with a power service of their own. A
//! bare compositor has none: on sway, Hyprland or river nothing answers, and the screen would
//! still blank part-way through a film. What they do have is this protocol, which is also how
//! every Wayland video player asks. Bloom binds it through the connection the toolkit already
//! holds, and creates an inhibitor against the app's own window, since the protocol counts an
//! inhibitor as holding only while its surface is on screen.
//!
//! Everything here runs on the UI thread, where the window's surface pointer is good. The
//! connection gets an event queue of its own, so nothing here dispatches what the toolkit is
//! waiting for. The same code as `app/src-tauri/src/awake/wayland.rs`.

use std::cell::RefCell;
use std::ffi::c_void;
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1;

/// Nothing Bloom binds here sends events; the queue carries its requests and the registry's news.
struct Quiet;

impl Dispatch<WlRegistry, GlobalListContents> for Quiet {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
wayland_client::delegate_noop!(Quiet: ZwpIdleInhibitManagerV1);
wayland_client::delegate_noop!(Quiet: ZwpIdleInhibitorV1);

/// What the compositor offers, looked for once.
enum Offer {
    Unknown,
    /// No such protocol here: KDE and GNOME, whose session bus answers instead, and anything older.
    Missing,
    Found(Bound),
}

struct Bound {
    connection: Connection,
    /// Kept for its handle, and so the queue outlives the objects made on it.
    queue: EventQueue<Quiet>,
    manager: ZwpIdleInhibitManagerV1,
}

thread_local! {
    static OFFER: RefCell<Offer> = const { RefCell::new(Offer::Unknown) };
    /// Bloom's inhibitor while a video is playing.
    static HELD: RefCell<Option<ZwpIdleInhibitorV1>> = const { RefCell::new(None) };
}

/// Asks the compositor to hold off, or lets go. On the UI thread, with the window's own
/// `wl_display` and `wl_surface`.
pub(super) fn set(playing: bool, display: *mut c_void, surface: *mut c_void) {
    if !playing {
        let held = HELD.with(|h| h.borrow_mut().take());
        if let Some(inhibitor) = held {
            inhibitor.destroy();
            with_bound(display, |bound| {
                let _ = bound.connection.flush();
            });
        }
        return;
    }
    if HELD.with(|h| h.borrow().is_some()) {
        return;
    }
    with_bound(display, |bound| {
        // The surface is asked for afresh on each play and turned into a proxy here rather than
        // kept: the toolkit may make a new one whenever the window goes away and comes back, and a stale one
        // would be a protocol error, which ends the whole connection the toolkit is on.
        // SAFETY: the toolkit's surface for the window on screen, and `from_ptr` checks that it really is
        // a wl_surface before anything is sent to it.
        let id = match unsafe { ObjectId::from_ptr(WlSurface::interface(), surface as *mut _) } {
            Ok(id) => id,
            Err(e) => return eprintln!("bloom: the window's Wayland surface wasn't usable: {e}"),
        };
        let surface = match WlSurface::from_id(&bound.connection, id) {
            Ok(surface) => surface,
            Err(e) => return eprintln!("bloom: the window's Wayland surface wasn't usable: {e}"),
        };
        let inhibitor = bound.manager.create_inhibitor(&surface, &bound.queue.handle(), ());
        let _ = bound.connection.flush();
        HELD.with(|h| *h.borrow_mut() = Some(inhibitor));
    });
}

/// Runs `f` with the compositor's inhibit manager, binding it the first time. Does nothing at all
/// where the compositor doesn't offer one.
fn with_bound(display: *mut c_void, f: impl FnOnce(&mut Bound)) {
    OFFER.with(|offer| {
        let mut offer = offer.borrow_mut();
        if matches!(*offer, Offer::Unknown) {
            *offer = match bind(display) {
                Some(bound) => Offer::Found(bound),
                None => Offer::Missing,
            };
        }
        if let Offer::Found(bound) = &mut *offer {
            // Keeps the registry's own news from piling up unread, and brings out any complaint
            // the compositor has made about what was sent.
            if let Err(e) = bound.queue.dispatch_pending(&mut Quiet) {
                eprintln!("bloom: the compositor didn't take Bloom's request to stay awake: {e}");
            }
            f(bound);
        }
    });
}

fn bind(display: *mut c_void) -> Option<Bound> {
    if display.is_null() {
        return None;
    }
    // SAFETY: the toolkit's own display, which lives as long as the window. `from_foreign_display` doesn't
    // take it over (it closes nothing when dropped) and makes an event queue of its own.
    let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
    let connection = Connection::from_backend(backend);
    let (globals, queue) = match registry_queue_init::<Quiet>(&connection) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("bloom: couldn't read what the compositor offers: {e}");
            return None;
        }
    };
    match globals.bind::<ZwpIdleInhibitManagerV1, _, _>(&queue.handle(), 1..=1, ()) {
        Ok(manager) => Some(Bound { connection, queue, manager }),
        // Said once: KDE and GNOME don't offer this, and answer on the session bus instead.
        Err(e) => {
            eprintln!("bloom: the compositor has no idle inhibitor ({e}); relying on the session bus");
            None
        }
    }
}
