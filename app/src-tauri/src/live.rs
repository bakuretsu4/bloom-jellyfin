//! Live updates from the server over its WebSocket, so pages change while they're open when
//! something changes elsewhere: an episode watched on the TV, a favourite set in the web client,
//! a film added to the library.
//!
//! One connection for the account signed in, made from Rust with the sign-in header, so the
//! token never reaches the page or a URL. The server asks for a keep-alive every so often
//! (ForceKeepAlive); the connection is re-made after a drop, with a growing wait, and whenever
//! the account changes. The page hears:
//!   sync:userdata  the latest watched state, progress and favourite of items that changed
//!   sync:library   that items were added to or removed from the library

use crate::jellyfin::{Jellyfin, SocketAccess};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

pub(crate) type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the connection checks the account is still the one it was made for, and whether a
/// keep-alive is due.
const CHECK_INTERVAL: Duration = Duration::from_secs(5);
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_LONGEST: Duration = Duration::from_secs(60);
/// A connection open at least this long starts the retry waits over; one closed sooner (a proxy
/// turning it away) keeps backing off.
const STABLE: Duration = Duration::from_secs(30);
/// How long the server may stay silent before it has said how often to expect keep-alives.
const QUIET_LIMIT: Duration = Duration::from_secs(90);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Envelope {
    message_type: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct UserDataDto {
    item_id: String,
    played: bool,
    played_percentage: Option<f64>,
    is_favorite: bool,
    unplayed_item_count: Option<u32>,
}

/// An item's user data as it now stands on the server.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserDataChange {
    item_id: String,
    played: bool,
    /// 0..1 when partly watched.
    progress: Option<f64>,
    favorite: bool,
    /// For a show or season: episodes left to watch.
    unplayed_count: Option<u32>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryChange {
    added: usize,
    removed: usize,
    /// For notifications (notify.rs); the page only needs the counts.
    #[serde(skip)]
    added_ids: Vec<String>,
}

#[derive(Debug, PartialEq)]
enum Incoming {
    /// The server wants a keep-alive this often.
    KeepAliveEvery(Duration),
    UserData(Vec<UserDataChange>),
    Library(LibraryChange),
    Other,
}

/// Ids come with or without dashes depending on the message; the pages use them without.
fn normalise_id(id: &str) -> String {
    id.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase()
}

/// One message from the server, for the user signed in.
fn parse(text: &str, user_id: &str) -> Incoming {
    let Ok(envelope) = serde_json::from_str::<Envelope>(text) else { return Incoming::Other };
    let data = &envelope.data;
    match envelope.message_type.as_str() {
        // Asked for at half the server's timeout, so one late message doesn't end the connection.
        "ForceKeepAlive" => Incoming::KeepAliveEvery(Duration::from_secs(data.as_u64().unwrap_or(60).clamp(10, 600) / 2)),
        "UserDataChanged" => {
            if data["UserId"].as_str().map(normalise_id) != Some(normalise_id(user_id)) {
                return Incoming::Other;
            }
            let list: Vec<UserDataDto> = serde_json::from_value(data["UserDataList"].clone()).unwrap_or_default();
            Incoming::UserData(
                list.into_iter()
                    .filter(|d| !d.item_id.is_empty())
                    .map(|d| UserDataChange {
                        item_id: normalise_id(&d.item_id),
                        played: d.played,
                        progress: if d.played { None } else { d.played_percentage.filter(|p| *p > 0.0).map(|p| (p / 100.0).min(1.0)) },
                        favorite: d.is_favorite,
                        unplayed_count: d.unplayed_item_count,
                    })
                    .collect(),
            )
        }
        "LibraryChanged" => {
            let count = |key: &str| data[key].as_array().map_or(0, Vec::len);
            let added_ids = data["ItemsAdded"].as_array().map_or_else(Vec::new, |ids| ids.iter().filter_map(|id| id.as_str()).map(normalise_id).collect());
            Incoming::Library(LibraryChange { added: count("ItemsAdded"), removed: count("ItemsRemoved"), added_ids })
        }
        _ => Incoming::Other,
    }
}

/// `http://host:8096/jellyfin` becomes `ws://host:8096/jellyfin/socket`.
fn socket_url(address: &str) -> Option<url::Url> {
    let mut url = url::Url::parse(address).ok()?;
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => return None,
    };
    url.set_scheme(scheme).ok()?;
    let path = format!("{}/socket", url.path().trim_end_matches('/'));
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}

/// Opens the socket as the signed-in user. Errors are kept vague on purpose: nothing about the
/// request (which carries the token) is ever printed.
pub(crate) async fn connect(access: &SocketAccess) -> Result<Socket, &'static str> {
    let url = socket_url(&access.address).ok_or("not a server address")?;
    let mut request = url.as_str().into_client_request().map_err(|_| "couldn't build the request")?;
    let header = HeaderValue::from_str(&access.authorization).map_err(|_| "couldn't build the request")?;
    request.headers_mut().insert("Authorization", header);
    let (socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
        .await
        .map_err(|_| "timed out")?
        .map_err(|_| "refused or unreachable")?;
    Ok(socket)
}

enum Ended {
    /// Signed out, or another account: connect again for the new one straight away.
    AccountChanged,
    /// The connection failed or dropped, after being open this long (zero if it never opened).
    Dropped { lasted: Duration },
}

/// How long the server may stay silent before the connection counts as dead. It answers every
/// keep-alive, so silence means a connection that went away without closing: the computer slept,
/// the network changed, a router forgot the connection. Nothing else would notice for many minutes.
fn silence_limit(keep_alive_every: Option<Duration>) -> Duration {
    keep_alive_every.map_or(QUIET_LIMIT, |every| every * 3)
}

async fn send_keep_alive(socket: &mut Socket) -> bool {
    let sent = tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::text(r#"{"MessageType":"KeepAlive"}"#))).await;
    matches!(sent, Ok(Ok(())))
}

async fn run(app: &AppHandle, access: &SocketAccess) -> Ended {
    let Ok(mut socket) = connect(access).await else { return Ended::Dropped { lasted: Duration::ZERO } };
    let opened = Instant::now();
    let dropped = || Ended::Dropped { lasted: opened.elapsed() };
    // Changes made while it was down weren't heard: the page drops what it heard before.
    let _ = app.emit("sync:connected", ());
    let mut every: Option<Duration> = None;
    let mut last_keep_alive = Instant::now();
    let mut last_heard = Instant::now();
    loop {
        let next = tokio::time::timeout(CHECK_INTERVAL, socket.next()).await;
        if app.state::<Jellyfin>().socket_access().ok().as_ref() != Some(access) {
            let _ = tokio::time::timeout(SEND_TIMEOUT, socket.close(None)).await;
            return Ended::AccountChanged;
        }
        if last_heard.elapsed() >= silence_limit(every) {
            return dropped();
        }
        if every.is_some_and(|every| last_keep_alive.elapsed() >= every) {
            last_keep_alive = Instant::now();
            if !send_keep_alive(&mut socket).await {
                return dropped();
            }
        }
        let text = match next {
            // Quiet: go round to check the account, the silence and the keep-alive.
            Err(_) => continue,
            Ok(Some(Ok(message))) => {
                last_heard = Instant::now();
                match message {
                    Message::Text(text) => text,
                    Message::Close(_) => return dropped(),
                    _ => continue,
                }
            }
            Ok(Some(Err(_))) | Ok(None) => return dropped(),
        };
        match parse(text.as_str(), &access.user_id) {
            Incoming::KeepAliveEvery(interval) => {
                every = Some(interval);
                last_keep_alive = Instant::now();
                if !send_keep_alive(&mut socket).await {
                    return dropped();
                }
            }
            Incoming::UserData(changes) if !changes.is_empty() => {
                let _ = app.emit("sync:userdata", changes);
            }
            Incoming::Library(change) if change.added + change.removed > 0 => {
                if !change.added_ids.is_empty() && app.state::<crate::settings::SettingsStore>().get().notify_new_media {
                    crate::notify::announce(app.clone(), change.added_ids.clone());
                }
                let _ = app.emit("sync:library", change);
            }
            _ => {}
        }
    }
}

/// Keeps a connection open for whichever account is signed in, for as long as Bloom runs.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut wait = RETRY_FIRST;
        loop {
            let access = app.state::<Jellyfin>().socket_access();
            let Ok(access) = access else {
                wait = RETRY_FIRST;
                tokio::time::sleep(CHECK_INTERVAL).await;
                continue;
            };
            match run(&app, &access).await {
                Ended::AccountChanged => wait = RETRY_FIRST,
                Ended::Dropped { lasted } => {
                    if lasted >= STABLE {
                        wait = RETRY_FIRST;
                    }
                    wait = if account_changed_within(&app, &access, wait).await { RETRY_FIRST } else { (wait * 2).min(RETRY_LONGEST) };
                }
            }
        }
    });
}

/// Waits `wait` before connecting again, but stops early (and says so) if the account changes
/// meanwhile, so another account's updates start at once rather than after a long backoff.
async fn account_changed_within(app: &AppHandle, access: &SocketAccess, wait: Duration) -> bool {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        tokio::time::sleep(until.saturating_duration_since(Instant::now()).min(CHECK_INTERVAL)).await;
        if app.state::<Jellyfin>().socket_access().ok().as_ref() != Some(access) {
            return true;
        }
    }
    false
}

/// For the live test: the next change the server reports for `item_id`.
#[cfg(test)]
pub(crate) async fn wait_for_user_data(socket: &mut Socket, user_id: &str, item_id: &str, within: Duration) -> Option<UserDataChange> {
    let deadline = Instant::now() + within;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(left, socket.next()).await else { continue };
        if let Incoming::UserData(changes) = parse(text.as_str(), user_id) {
            if let Some(change) = changes.into_iter().find(|c| c.item_id == normalise_id(item_id)) {
                return Some(change);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_silent_connection_counts_as_dead() {
        // Jellyfin asks for keep-alives at half its 60-second timeout, and answers each one.
        assert_eq!(silence_limit(Some(Duration::from_secs(30))), Duration::from_secs(90));
        assert_eq!(silence_limit(None), QUIET_LIMIT);
    }

    #[test]
    fn socket_addresses() {
        assert_eq!(socket_url("http://192.168.1.20:8097").unwrap().as_str(), "ws://192.168.1.20:8097/socket");
        assert_eq!(socket_url("https://media.example.com/jellyfin/").unwrap().as_str(), "wss://media.example.com/jellyfin/socket");
        assert!(socket_url("ftp://x").is_none());
    }

    #[test]
    fn messages_for_the_user_signed_in() {
        assert_eq!(parse(r#"{"MessageType":"ForceKeepAlive","Data":60}"#, "u1"), Incoming::KeepAliveEvery(Duration::from_secs(30)));
        let changed = r#"{"MessageType":"UserDataChanged","Data":{"UserId":"aa-bb","UserDataList":[
            {"ItemId":"C1-D2","Played":false,"PlayedPercentage":42.5,"IsFavorite":true},
            {"ItemId":"e3","Played":true,"PlayedPercentage":100,"IsFavorite":false,"UnplayedItemCount":0}]}}"#;
        let Incoming::UserData(changes) = parse(changed, "AABB") else { panic!("user data wasn't read") };
        assert_eq!(changes[0].item_id, "c1d2");
        assert_eq!(changes[0].progress, Some(0.425));
        assert!(changes[0].favorite);
        assert_eq!((changes[1].played, changes[1].progress, changes[1].unplayed_count), (true, None, Some(0)));
        assert_eq!(parse(changed, "someone-else"), Incoming::Other, "another user's change was passed on");
        assert_eq!(
            parse(r#"{"MessageType":"LibraryChanged","Data":{"ItemsAdded":["A-1","b"],"ItemsRemoved":[],"ItemsUpdated":["c"]}}"#, "u1"),
            Incoming::Library(LibraryChange { added: 2, removed: 0, added_ids: vec!["a1".into(), "b".into()] })
        );
        assert_eq!(parse("not json", "u1"), Incoming::Other);
        assert_eq!(parse(r#"{"MessageType":"Play","Data":{}}"#, "u1"), Incoming::Other);
    }
}
