//! Desktop notifications for new episodes and films, while Bloom is open.
//!
//! The server's socket says which items were added (live.rs); with the Settings switch on, they're
//! looked up as the signed-in user, so only what that account can watch is announced, and shown
//! through the desktop's notification service (org.freedesktop.Notifications on the session bus,
//! which KDE, GNOME and the rest provide). Additions are gathered for a minute first, since a
//! library scan reports them in many messages. A show's episodes arriving together make one
//! notification, and a big batch of titles makes one in all.

use crate::ctx::Ctx;
use crate::jellyfin::Jellyfin;
use serde::Deserialize;
use std::sync::Mutex;
use std::time::Duration;

/// How long additions are gathered before they're announced.
const GATHER: Duration = Duration::from_secs(60);
/// More ids than this in a minute is a library import: summed up from the first ones.
const MOST_IDS: usize = 100;
/// More shows and films than this in one batch are one notification rather than several.
const MOST_SEPARATE: usize = 3;

/// Additions waiting to be announced, for the account they were reported to.
struct Gathered {
    user_id: String,
    ids: Vec<String>,
    /// More ids came than are kept.
    more: bool,
}

static GATHERED: Mutex<Option<Gathered>> = Mutex::new(None);

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct AddedDto {
    id: String,
    name: String,
    series_id: Option<String>,
    #[serde(rename = "Type")]
    kind: String,
    series_name: Option<String>,
    index_number: Option<u32>,
    parent_index_number: Option<u32>,
    production_year: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct AddedPage {
    items: Vec<AddedDto>,
}

fn episode_code(episode: &AddedDto) -> Option<String> {
    match (episode.parent_index_number, episode.index_number) {
        // Season 0 is where Jellyfin keeps a show's specials.
        (Some(0), Some(number)) => Some(format!("Special {number}")),
        (Some(season), Some(number)) => Some(format!("S{season} E{number}")),
        (None, Some(number)) => Some(format!("E{number}")),
        _ => None,
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// One notification: what it says, and the title that clicking it opens, when it is about one.
#[derive(Debug, PartialEq)]
struct Notice {
    summary: String,
    body: String,
    open: Option<String>,
}

impl Notice {
    fn new(summary: impl Into<String>, body: impl Into<String>, open: Option<String>) -> Self {
        Self { summary: summary.into(), body: body.into(), open }
    }
}

/// The notifications for a batch of added items. Only episodes and films. `more` says the items
/// are only the first of a larger batch, so they can't be counted.
fn notifications(items: &[AddedDto], more: bool) -> Vec<Notice> {
    let films: Vec<&AddedDto> = items.iter().filter(|i| i.kind == "Movie").collect();
    let mut shows: Vec<(String, Vec<&AddedDto>)> = Vec::new();
    for episode in items.iter().filter(|i| i.kind == "Episode") {
        let show = episode.series_name.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "a show".into());
        match shows.iter_mut().find(|(name, _)| *name == show) {
            Some((_, episodes)) => episodes.push(episode),
            None => shows.push((show, vec![episode])),
        }
    }
    for (_, episodes) in &mut shows {
        episodes.sort_by_key(|e| (e.parent_index_number, e.index_number));
    }

    if more || shows.len() + films.len() > MOST_SEPARATE {
        let names: Vec<String> = shows.iter().map(|(name, _)| name.clone()).chain(films.iter().map(|f| f.name.clone())).collect();
        if names.is_empty() {
            return Vec::new();
        }
        let episode_count: usize = shows.iter().map(|(_, e)| e.len()).sum();
        let summary = match (more, episode_count, films.len()) {
            (true, _, _) => "Many new titles added".to_string(),
            (false, 0, f) => format!("{} added", plural(f, "new film", "new films")),
            (false, e, 0) => format!("{} added", plural(e, "new episode", "new episodes")),
            (false, e, f) => format!("{} and {} added", plural(e, "new episode", "new episodes"), plural(f, "film", "films")),
        };
        let body = if names.len() > MOST_SEPARATE {
            format!("{}, and {} more", names[..MOST_SEPARATE].join(", "), names.len() - MOST_SEPARATE)
        } else if more {
            format!("{}, and more", names.join(", "))
        } else {
            names.join(", ")
        };
        return vec![Notice::new(summary, body, None)];
    }

    let mut out = Vec::new();
    for (show, episodes) in shows {
        if let [episode] = episodes.as_slice() {
            let body = match episode_code(episode) {
                Some(code) => format!("{code} · {}", episode.name),
                None => episode.name.clone(),
            };
            out.push(Notice::new(format!("New episode of {show}"), body, episode.series_id.clone()));
        } else {
            let first = episodes.first().and_then(|e| episode_code(e));
            let last = episodes.last().and_then(|e| episode_code(e));
            let body = match (first, last) {
                (Some(first), Some(last)) => format!("{first} to {last}"),
                _ => episodes.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", "),
            };
            let open = episodes.iter().find_map(|e| e.series_id.clone());
            out.push(Notice::new(format!("{} of {show}", plural(episodes.len(), "new episode", "new episodes")), body, open));
        }
    }
    for film in films {
        let body = match film.production_year {
            Some(year) => format!("{} ({year})", film.name),
            None => film.name.clone(),
        };
        out.push(Notice::new("New film", body, Some(film.id.clone()).filter(|id| !id.is_empty())));
    }
    out
}

/// Notification bodies may be read as markup; the summary is plain text by the spec.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// How long a notification can be clicked while Bloom is open to hear it.
#[cfg(target_os = "linux")]
const CLICKABLE_FOR: Duration = Duration::from_secs(15 * 60);

/// The desktop's "a notification was clicked" signal.
#[cfg(target_os = "linux")]
struct ActionInvoked {
    id: u32,
}

#[cfg(target_os = "linux")]
impl dbus::arg::ReadAll for ActionInvoked {
    fn read(i: &mut dbus::arg::Iter) -> Result<Self, dbus::arg::TypeMismatchError> {
        let id = i.read()?;
        let _action: String = i.read()?;
        Ok(Self { id })
    }
}

#[cfg(target_os = "linux")]
impl dbus::arg::AppendAll for ActionInvoked {
    fn append(&self, i: &mut dbus::arg::IterAppend) {
        dbus::arg::RefArg::append(&self.id, i);
        dbus::arg::RefArg::append(&String::from("default"), i);
    }
}

#[cfg(target_os = "linux")]
impl dbus::message::SignalArgs for ActionInvoked {
    const NAME: &'static str = "ActionInvoked";
    const INTERFACE: &'static str = "org.freedesktop.Notifications";
}

/// The desktop's "a notification went away" signal.
#[cfg(target_os = "linux")]
struct NotificationClosed {
    id: u32,
}

#[cfg(target_os = "linux")]
impl dbus::arg::ReadAll for NotificationClosed {
    fn read(i: &mut dbus::arg::Iter) -> Result<Self, dbus::arg::TypeMismatchError> {
        let id = i.read()?;
        let _reason: u32 = i.read()?;
        Ok(Self { id })
    }
}

#[cfg(target_os = "linux")]
impl dbus::arg::AppendAll for NotificationClosed {
    fn append(&self, i: &mut dbus::arg::IterAppend) {
        dbus::arg::RefArg::append(&self.id, i);
        dbus::arg::RefArg::append(&0u32, i);
    }
}

#[cfg(target_os = "linux")]
impl dbus::message::SignalArgs for NotificationClosed {
    const NAME: &'static str = "NotificationClosed";
    const INTERFACE: &'static str = "org.freedesktop.Notifications";
}

/// Shows the notifications. Those about a title carry a default action, so clicking one calls
/// `open` with that title's id; this keeps listening for clicks until they are all gone or a
/// quarter of an hour has passed.
#[cfg(target_os = "linux")]
fn show(notices: Vec<Notice>, open: impl Fn(String)) -> Result<(), dbus::Error> {
    use dbus::arg::PropMap;
    use dbus::blocking::Connection;
    use std::collections::HashMap;
    use std::sync::{mpsc, Arc};

    let connection = Connection::new_session()?;
    let proxy = connection.with_proxy("org.freedesktop.Notifications", "/org/freedesktop/Notifications", Duration::from_secs(5));
    let mut targets: HashMap<u32, String> = HashMap::new();
    for notice in notices {
        let actions: Vec<&str> = if notice.open.is_some() { vec!["default", "Open"] } else { Vec::new() };
        let (id,): (u32,) = proxy.method_call(
            "org.freedesktop.Notifications",
            "Notify",
            ("Bloom", 0u32, "", notice.summary, escape(&notice.body), actions, PropMap::new(), -1i32),
        )?;
        if let Some(title) = notice.open {
            targets.insert(id, title);
        }
    }
    if targets.is_empty() {
        return Ok(());
    }

    // Clicks and dismissals arrive as signals, on this connection, while it is processed.
    let (clicked, clicks) = mpsc::channel::<u32>();
    let (closed, closes) = mpsc::channel::<u32>();
    let listener = Connection::new_session()?;
    let watcher = listener.with_proxy("org.freedesktop.Notifications", "/org/freedesktop/Notifications", Duration::from_secs(5));
    watcher.match_signal(move |s: ActionInvoked, _: &Connection, _: &dbus::Message| clicked.send(s.id).is_ok())?;
    watcher.match_signal(move |s: NotificationClosed, _: &Connection, _: &dbus::Message| closed.send(s.id).is_ok())?;
    let started = std::time::Instant::now();
    let targets = Arc::new(targets);
    let mut waiting: Vec<u32> = targets.keys().copied().collect();
    while !waiting.is_empty() && started.elapsed() < CLICKABLE_FOR {
        listener.process(Duration::from_secs(1))?;
        while let Ok(id) = clicks.try_recv() {
            if let Some(title) = targets.get(&id) {
                open(title.clone());
            }
        }
        while let Ok(id) = closes.try_recv() {
            waiting.retain(|w| *w != id);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn show(_notices: Vec<Notice>, _open: impl Fn(String)) -> Result<(), String> {
    Ok(())
}

/// Adds `ids` to what's gathered for `user_id`. True when nothing was gathered yet, so a minute's
/// wait should start. Ids reported to an earlier account are dropped.
fn gather(gathered: &mut Option<Gathered>, user_id: &str, ids: Vec<String>) -> bool {
    let starting = gathered.is_none();
    let batch = match gathered {
        Some(batch) if batch.user_id == user_id => batch,
        _ => gathered.insert(Gathered { user_id: user_id.to_string(), ids: Vec::new(), more: false }),
    };
    for id in ids {
        if batch.ids.contains(&id) {
            continue;
        }
        if batch.ids.len() == MOST_IDS {
            batch.more = true;
            break;
        }
        batch.ids.push(id);
    }
    starting
}

/// Gathers the added items, and a minute later looks them up as the signed-in user and shows
/// what's new among them.
pub fn announce(app: Ctx, ids: Vec<String>) {
    let ids: Vec<String> = ids.into_iter().filter(|id| !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit())).collect();
    let Ok(user_id) = app.state::<Jellyfin>().user_id() else { return };
    if ids.is_empty() {
        return;
    }
    let starting = gather(&mut GATHERED.lock().unwrap_or_else(|e| e.into_inner()), &user_id, ids);
    if !starting {
        return;
    }
    crate::rt::spawn(async move {
        tokio::time::sleep(GATHER).await;
        let Some(batch) = GATHERED.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        let jf = app.state::<Jellyfin>();
        // Signed out or switched account meanwhile: those items were someone else's news.
        if jf.user_id().ok().as_deref() != Some(batch.user_id.as_str()) {
            return;
        }
        let path = format!(
            "/Items?userId={}&ids={}&fields=ProductionYear&enableImages=false&enableUserData=false",
            batch.user_id,
            batch.ids.join(",")
        );
        let Ok(page) = jf.get_json::<AddedPage>(path).await else { return };
        let messages = notifications(&page.items, batch.more);
        if messages.is_empty() {
            return;
        }
        // On its own thread, since it stays listening for clicks long after the notifications show.
        let spawned = std::thread::Builder::new().name("notifications".into()).spawn(move || {
            let open = |id: String| {
                let _ = app.emit("notify:open", serde_json::json!({ "id": id }));
            };
            if let Err(e) = show(messages, open) {
                eprintln!("bloom: couldn't show a notification: {e}");
            }
        });
        if let Err(e) = spawned {
            eprintln!("bloom: couldn't show a notification: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(show: &str, season: u32, number: u32, name: &str) -> AddedDto {
        AddedDto {
            id: format!("{show}-{season}-{number}"),
            series_id: Some(format!("{show}-id")),
            name: name.into(),
            kind: "Episode".into(),
            series_name: Some(show.into()),
            index_number: Some(number),
            parent_index_number: Some(season),
            production_year: None,
        }
    }

    fn film(name: &str, year: u32) -> AddedDto {
        AddedDto { id: format!("{name}-id"), name: name.into(), kind: "Movie".into(), production_year: Some(year), ..AddedDto::default() }
    }

    #[test]
    fn a_few_new_titles_each_get_one() {
        let items = [
            episode("Frieren", 2, 5, "The Hero's Party"),
            episode("Dandadan", 1, 7, "Z"),
            episode("Dandadan", 1, 6, "Y"),
            film("Dune: Part Two", 2024),
            AddedDto { name: "Season 2".into(), kind: "Season".into(), ..AddedDto::default() },
        ];
        // Each opens its show, or its film, when clicked.
        assert_eq!(
            notifications(&items, false),
            vec![
                Notice::new("New episode of Frieren", "S2 E5 · The Hero's Party", Some("Frieren-id".into())),
                Notice::new("2 new episodes of Dandadan", "S1 E6 to S1 E7", Some("Dandadan-id".into())),
                Notice::new("New film", "Dune: Part Two (2024)", Some("Dune: Part Two-id".into())),
            ]
        );
    }

    /// Shows a real notification on this desktop; run with `--ignored`.
    #[test]
    #[ignore]
    #[cfg(target_os = "linux")]
    fn shows_a_notification() {
        let notice = Notice::new("New episode of Tom & Jerry", "S1 E1 · A notification test from cargo test <b>not bold</b>", Some("abc".into()));
        show(vec![notice], |id| println!("clicked: {id}")).expect("the notification service refused it");
    }

    #[test]
    fn a_big_batch_is_summed_up() {
        let items = [
            episode("A", 1, 1, "x"),
            episode("B", 1, 1, "x"),
            episode("B", 1, 2, "x"),
            episode("C", 1, 1, "x"),
            film("D", 2020),
            film("E", 2021),
        ];
        assert_eq!(notifications(&items, false), vec![Notice::new("4 new episodes and 2 films added", "A, B, C, and 2 more", None)]);
        assert!(notifications(&[AddedDto { kind: "Folder".into(), ..AddedDto::default() }], false).is_empty());
        assert_eq!(escape("Tom & Jerry <3"), "Tom &amp; Jerry &lt;3");
        assert_eq!(notifications(&[episode("Tanaka-kun", 0, 42, "Cooking Course")], false)[0].body, "Special 42 · Cooking Course");
    }

    #[test]
    fn past_the_limit_it_says_many_without_counting() {
        assert_eq!(
            notifications(&[episode("A", 1, 1, "x"), episode("A", 1, 2, "y")], true),
            vec![Notice::new("Many new titles added", "A, and more", None)]
        );
        assert!(notifications(&[AddedDto { kind: "Folder".into(), ..AddedDto::default() }], true).is_empty());
    }

    #[test]
    fn additions_are_gathered_for_one_account() {
        let mut gathered = None;
        let ids = |range: std::ops::Range<usize>| range.map(|n| format!("{n:x}")).collect::<Vec<_>>();
        assert!(gather(&mut gathered, "u1", ids(0..60)), "the first addition starts the wait");
        assert!(!gather(&mut gathered, "u1", ids(50..90)), "a second one joins it");
        let batch = gathered.as_ref().unwrap();
        assert_eq!((batch.ids.len(), batch.more), (90, false), "repeated ids are counted once");
        gather(&mut gathered, "u1", ids(90..200));
        let batch = gathered.as_ref().unwrap();
        assert_eq!((batch.ids.len(), batch.more), (MOST_IDS, true));
        // Another account's additions replace what was gathered for the one before, on the same wait.
        assert!(!gather(&mut gathered, "u2", ids(0..3)));
        let batch = gathered.as_ref().unwrap();
        assert_eq!((batch.user_id.as_str(), batch.ids.len(), batch.more), ("u2", 3, false));
    }
}
