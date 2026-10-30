//! Live updates: what the server says changed while pages are open (an episode watched on the TV,
//! a favourite set in the web client, a title added).
//!
//! The core keeps the connection and announces changes as events; this lays them over what is on
//! screen. Watched state, progress and favourites change in place on cards, the title page and its
//! episodes. The rows on the page are asked for again, but not while watching, not sooner than 2.5
//! seconds after the news, and for changes of user data not more than once a minute.

use crate::{cards, hover, nav, title, App, Screen};
use bloom_core::Ctx;
use serde::Deserialize;
use serde_json::Value;
use slint::Weak;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// News is waited for this long before the page reloads, in case more follows.
const SETTLE: Duration = Duration::from_millis(2500);
/// A reload for user data comes at most this often.
const USERDATA_GAP: Duration = Duration::from_secs(60);

/// One item's user data as it now stands on the server.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub item_id: String,
    pub played: bool,
    pub progress: Option<f64>,
    pub favorite: bool,
    pub unplayed_count: Option<u32>,
}

static LAST_USERDATA_RELOAD: Mutex<Option<Instant>> = Mutex::new(None);
static RELOAD_PENDING: AtomicBool = AtomicBool::new(false);
/// How many times the socket has connected: the first is when the pages have only just loaded.
static CONNECTIONS: Mutex<u32> = Mutex::new(0);

/// How long until a reload for `userdata` news may happen, given when the last one was.
fn wait_for(userdata: bool, last: Option<Instant>, now: Instant) -> Duration {
    if !userdata {
        return SETTLE;
    }
    let allowed = last.map(|l| (l + USERDATA_GAP).saturating_duration_since(now)).unwrap_or_default();
    SETTLE.max(allowed)
}

pub fn on_userdata(ctx: Option<Ctx>, weak: &Weak<App>, payload: Value) {
    let changes: Vec<Change> = serde_json::from_value(payload).unwrap_or_default();
    if changes.is_empty() {
        return;
    }
    let _ = weak.upgrade_in_event_loop(move |app| {
        for change in &changes {
            // A hover card fetched before the change would show it stale.
            hover::forget(&change.item_id);
            cards::apply_change(change);
            title::apply_change(&app, change);
        }
    });
    if let Some(ctx) = ctx {
        schedule_reload(ctx, weak.clone(), true);
    }
}

pub fn on_library(ctx: Option<Ctx>, weak: &Weak<App>) {
    if let Some(ctx) = ctx {
        schedule_reload(ctx, weak.clone(), false);
    }
}

/// The socket connected. After the first time, whatever changed while it was down went unheard.
pub fn on_connected(ctx: Option<Ctx>, weak: &Weak<App>) {
    let again = {
        let mut n = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        *n += 1;
        *n > 1
    };
    if again {
        if let Some(ctx) = ctx {
            schedule_reload(ctx, weak.clone(), true);
        }
    }
}

fn schedule_reload(ctx: Ctx, weak: Weak<App>, userdata: bool) {
    if RELOAD_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let delay = wait_for(userdata, *LAST_USERDATA_RELOAD.lock().unwrap_or_else(|e| e.into_inner()), Instant::now());
    bloom_core::rt::spawn(async move {
        tokio::time::sleep(delay).await;
        RELOAD_PENDING.store(false, Ordering::SeqCst);
        if userdata {
            *LAST_USERDATA_RELOAD.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        }
        let w = weak.clone();
        let _ = w.upgrade_in_event_loop(move |app| {
            // Not while watching, and not over a page being typed into.
            if matches!(app.get_screen(), Screen::Watch | Screen::SignIn | Screen::Loading) {
                return;
            }
            match app.get_screen() {
                // These come from the server: ask again. Settings and the like don't change.
                Screen::Home | Screen::Library | Screen::Search => nav::reload(ctx, weak),
                _ => {}
            }
        });
    });
}

/// Clears what was heard (another account's news doesn't apply).
pub fn forget() {
    *LAST_USERDATA_RELOAD.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner()) = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_news_settles_and_user_data_news_waits_out_the_minute() {
        let now = Instant::now();
        assert_eq!(wait_for(false, Some(now), now), SETTLE);
        assert_eq!(wait_for(true, None, now), SETTLE);
        // A reload ten seconds ago: fifty seconds to go.
        assert_eq!(wait_for(true, Some(now - Duration::from_secs(10)), now), Duration::from_secs(50));
        // One long ago: only the settling.
        assert_eq!(wait_for(true, Some(now - Duration::from_secs(300)), now), SETTLE);
    }

    #[test]
    fn a_change_reads_as_the_core_sends_it() {
        let v = serde_json::json!([{"itemId": "a1", "played": true, "progress": null, "favorite": false, "unplayedCount": 3}]);
        let changes: Vec<Change> = serde_json::from_value(v).unwrap();
        assert_eq!(changes[0].item_id, "a1");
        assert!(changes[0].played && changes[0].progress.is_none());
        assert_eq!(changes[0].unplayed_count, Some(3));
    }
}
