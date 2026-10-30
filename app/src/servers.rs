//! The Servers screen: every server added on this computer, whether it answers, the accounts saved
//! for it, and removing one (with or without the downloads from it).

use crate::{flow, watch, App, NavTarget, Screen, ServerData};
use bloom_core::downloads::{self, Downloads};
use bloom_core::jellyfin::{self, Jellyfin, ServerEntry};
use bloom_core::Ctx;
use slint::{Model, ModelRc, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// What a row needs that the screen doesn't show: where to go, and which sign-ins are saved.
#[derive(Clone)]
struct Entry {
    id: String,
    name: String,
    address: String,
    current: bool,
    /// (user id, name)
    accounts: Vec<(String, String)>,
}

static ENTRIES: LazyLock<Mutex<Vec<Entry>>> = LazyLock::new(Mutex::default);

fn entries() -> std::sync::MutexGuard<'static, Vec<Entry>> {
    ENTRIES.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    /// The rows on screen, to change one when its check comes back. UI thread.
    static ROWS: RefCell<Option<Rc<VecModel<ServerData>>>> = const { RefCell::new(None) };
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn plural(n: u64, unit: &str) -> String {
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// "last seen 3 days ago", from when it last answered (seconds since the epoch).
fn last_seen(seen: Option<u64>, now: u64) -> String {
    let Some(seen) = seen else { return "never answered".into() };
    let minutes = now.saturating_sub(seen) / 60;
    match minutes {
        0 => "last seen just now".into(),
        1..=59 => format!("last seen {} ago", plural(minutes, "minute")),
        60..=1439 => format!("last seen {} ago", plural(minutes / 60, "hour")),
        _ => format!("last seen {} ago", plural(minutes / 1440, "day")),
    }
}

/// How a server's check came out: the words, and the tone (0 plain, 1 good, 2 bad).
fn status(current: bool, reachable: Option<bool>, seen: Option<u64>, now: u64) -> (String, i32) {
    match reachable {
        None => ("Checking".into(), 0),
        Some(false) if current => ("Can't reach it".into(), 2),
        Some(false) => (format!("No response, {}", last_seen(seen, now)), 2),
        Some(true) if current => ("Connected".into(), 1),
        Some(true) => ("Reachable".into(), 0),
    }
}

/// "192.168.1.20:8096, Jellyfin 12.1.0, saved for alyx, guest".
fn detail_text(e: &ServerEntry, accounts: &[(String, String)]) -> String {
    let address = e.server.address.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let mut text = format!("{address}, Jellyfin {}", e.server.version);
    if !accounts.is_empty() {
        text.push_str(&format!(", saved for {}", accounts.iter().map(|a| a.1.as_str()).collect::<Vec<_>>().join(", ")));
    }
    text
}

fn row(e: &ServerEntry, accounts: &[(String, String)], reachable: Option<bool>) -> ServerData {
    let (text, tone) = status(e.current, reachable, e.last_seen, now());
    ServerData {
        id: e.server.id.clone().into(),
        initial: e.server.name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into(),
        name: e.server.name.clone().into(),
        detail: detail_text(e, accounts).into(),
        status: text.into(),
        tone,
        current: e.current,
        reachable: reachable == Some(true),
        unreachable: reachable == Some(false),
        has_accounts: !accounts.is_empty(),
        signs_out: !accounts.is_empty() || e.current,
    }
}

/// Reads the saved servers and shows them, then asks each whether it answers. UI thread.
pub fn show(ctx: &Ctx, weak: &Weak<App>, app: &App) {
    app.set_nav_target(NavTarget::None);
    app.set_servers_loading(true);
    app.set_servers_add_error("".into());
    app.set_servers_remove_error("".into());
    app.set_servers_confirm_id("".into());
    app.set_screen(Screen::Servers);
    load(ctx.clone(), weak.clone());
}

fn load(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let list = jellyfin::servers_with(&jf);
        let kept: Vec<Entry> = list
            .iter()
            .map(|e| Entry {
                id: e.server.id.clone(),
                name: e.server.name.clone(),
                address: e.server.address.clone(),
                current: e.current,
                accounts: e.accounts.iter().map(|u| (u.id.clone(), u.name.clone())).collect(),
            })
            .collect();
        let rows: Vec<ServerData> = list.iter().zip(&kept).map(|(e, k)| row(e, &k.accounts, None)).collect();
        let ids: Vec<String> = kept.iter().map(|k| k.id.clone()).collect();
        *entries() = kept;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if app.get_screen() != Screen::Servers {
                return;
            }
            let model = Rc::new(VecModel::from(rows));
            ROWS.with(|r| *r.borrow_mut() = Some(model.clone()));
            app.set_servers_data(ModelRc::from(model));
            app.set_servers_loading(false);
        });
        for id in ids {
            check(ctx.clone(), weak.clone(), id);
        }
    });
}

/// Asks a server whether it's there, and shows the answer on its row.
pub fn check(ctx: Ctx, weak: Weak<App>, id: String) {
    let w = weak.clone();
    let id2 = id.clone();
    let _ = w.upgrade_in_event_loop(move |_| set_row(&id2, |r| (r.status, r.tone) = ("Checking".into(), 0)));
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let answer = jellyfin::ping_server_with(&jf, &id).await;
        let current = entries().iter().find(|e| e.id == id).is_some_and(|e| e.current);
        let _ = weak.upgrade_in_event_loop(move |_| {
            set_row(&id, |r| {
                let (reachable, seen) = match &answer {
                    Ok(reach) => (reach.reachable, reach.last_seen),
                    Err(_) => (false, None),
                };
                let (text, tone) = status(current, Some(reachable), seen, now());
                r.status = text.into();
                r.tone = tone;
                if let Some(version) = answer.as_ref().ok().and_then(|a| a.version.as_ref()) {
                    // "192.168.1.20:8096, Jellyfin 12.0.0": the version may have moved on.
                    if let Some((host, _)) = r.detail.rsplit_once(", Jellyfin ") {
                        r.detail = format!("{host}, Jellyfin {version}").into();
                    }
                }
            });
        });
    });
}

/// Changes the row for `id`. UI thread.
fn set_row(id: &str, change: impl FnOnce(&mut ServerData)) {
    ROWS.with(|r| {
        let Some(model) = r.borrow().clone() else { return };
        for i in 0..model.row_count() {
            if let Some(mut row) = model.row_data(i) {
                if row.id == id {
                    change(&mut row);
                    model.set_row_data(i, row);
                    return;
                }
            }
        }
    });
}

/// "Open": the account saved for it if there is one, otherwise a sign-in there.
pub fn open(ctx: Ctx, weak: Weak<App>, id: String) {
    let Some(entry) = entries().iter().find(|e| e.id == id).cloned() else { return };
    match entry.accounts.first() {
        Some((user_id, _)) => flow::switch_account(ctx, weak, entry.id, user_id.clone()),
        None => flow::sign_in_at(ctx, weak, entry.address),
    }
}

/// "Remove": asks first, saying what would happen to the downloads from it.
pub fn ask_remove(ctx: Ctx, weak: Weak<App>, id: String) {
    let summary = downloads::server_downloads(&ctx.state::<Downloads>(), id.clone());
    let text = if summary.count == 0 {
        String::new()
    } else {
        let name = entries().iter().find(|e| e.id == id).map(|e| (e.name.clone(), e.current || !e.accounts.is_empty()));
        let (name, signs_out) = name.unwrap_or_default();
        format!(
            "{} ({}) came from {name}{}. Kept, they stay on disk and come back if you add it again.",
            plural(summary.count as u64, "download"),
            crate::downloads::bytes_label(summary.bytes),
            if signs_out { ", and removing it signs out" } else { "" }
        )
    };
    let _ = weak.upgrade_in_event_loop(move |app| {
        app.set_servers_remove_error("".into());
        app.set_servers_confirm_downloads(text.into());
        app.set_servers_confirm_id(id.into());
    });
}

pub fn cancel_remove(weak: Weak<App>) {
    let _ = weak.upgrade_in_event_loop(|app| app.set_servers_confirm_id("".into()));
}

pub fn remove(ctx: Ctx, weak: Weak<App>, id: String, delete_downloads: bool) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_servers_removing(true);
        app.set_servers_remove_error("".into());
    });
    bloom_core::rt::spawn(async move {
        if delete_downloads {
            downloads::remove_server_downloads(&ctx.state::<Downloads>(), id.clone());
        }
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::remove_server_with(&jf, &id).await {
            Ok(signed_out) => {
                let _ = weak.upgrade_in_event_loop(|app| {
                    app.set_servers_removing(false);
                    app.set_servers_confirm_id("".into());
                });
                // Removing the server in use signs Bloom out.
                if signed_out {
                    flow::signed_out(&ctx, &weak);
                } else {
                    load(ctx, weak);
                }
            }
            Err(e) => {
                let message = e.to_string();
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_servers_removing(false);
                    app.set_servers_remove_error(message.into());
                });
            }
        }
    });
}

/// "Add": checks the address, which records the server, then lists it.
pub fn add(ctx: Ctx, weak: Weak<App>, address: String) {
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(|app| {
        app.set_servers_adding(true);
        app.set_servers_add_error("".into());
    });
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match jellyfin::check_server_with(&jf, address).await {
            Ok(check) => {
                let name = check.server.name;
                let _ = weak.upgrade_in_event_loop(|app| {
                    app.set_servers_adding(false);
                    app.set_servers_add_address("".into());
                });
                watch::say(&weak, format!("{name} added."));
                load(ctx, weak);
            }
            Err(e) => {
                let message = e.to_string();
                let _ = weak.upgrade_in_event_loop(move |app| {
                    app.set_servers_adding(false);
                    app.set_servers_add_error(message.into());
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_seen_counts_in_the_largest_whole_unit() {
        let now = 1_000_000;
        assert_eq!(last_seen(None, now), "never answered");
        assert_eq!(last_seen(Some(now - 20), now), "last seen just now");
        assert_eq!(last_seen(Some(now - 60), now), "last seen 1 minute ago");
        assert_eq!(last_seen(Some(now - 59 * 60), now), "last seen 59 minutes ago");
        assert_eq!(last_seen(Some(now - 3 * 3600), now), "last seen 3 hours ago");
        assert_eq!(last_seen(Some(now - 2 * 86_400), now), "last seen 2 days ago");
        // A clock that moved backwards doesn't underflow.
        assert_eq!(last_seen(Some(now + 500), now), "last seen just now");
    }

    #[test]
    fn a_checked_server_reads_as_connected_reachable_or_silent() {
        let now = 1_000_000;
        assert_eq!(status(true, None, None, now), ("Checking".into(), 0));
        assert_eq!(status(true, Some(true), Some(now), now), ("Connected".into(), 1));
        assert_eq!(status(false, Some(true), Some(now), now), ("Reachable".into(), 0));
        assert_eq!(status(true, Some(false), Some(now - 3600), now), ("Can't reach it".into(), 2));
        assert_eq!(status(false, Some(false), Some(now - 3600), now), ("No response, last seen 1 hour ago".into(), 2));
        assert_eq!(status(false, Some(false), None, now), ("No response, never answered".into(), 2));
    }

    #[test]
    fn a_server_row_says_where_it_is_and_who_is_saved_for_it() {
        let entry = |address: &str| ServerEntry { server: bloom_core::jellyfin::Server { address: address.into(), id: "s".into(), name: "Home".into(), version: "12.1.0".into() }, last_seen: None, current: false, accounts: Vec::new() };
        assert_eq!(detail_text(&entry("http://192.168.1.20:8096/"), &[]), "192.168.1.20:8096, Jellyfin 12.1.0");
        let two = [("1".to_string(), "alyx".to_string()), ("2".to_string(), "guest".to_string())];
        assert_eq!(detail_text(&entry("https://media.example.com"), &two), "media.example.com, Jellyfin 12.1.0, saved for alyx, guest");
    }
}
