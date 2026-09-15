//! Discord presence: what's playing, on the user's Discord profile, through the Discord app
//! running on this computer and its local IPC socket. Off by default; Settings turns it on.
//!
//! Only the title, the episode (or the year), whether it's paused and the time left are sent,
//! and only to the local Discord app. No connection is made while the setting is off or nothing
//! is playing, and nothing is sent when Discord isn't running. With "Show the title" off, the
//! profile only says that a show or a film is being watched.
//!
//! The cover art is a public image address, because Discord fetches the picture itself and can't
//! reach the Jellyfin server. It comes from the server's own metadata sources (its remote images:
//! TMDB, AniDB), so Bloom needs no keys of its own, and only an https address on a public host is
//! ever used, never one on the server. With no such cover, or no server, it's the moth.
//!
//! The protocol: frames of a little-endian opcode and length, then JSON. A handshake names the
//! application (which gives the activity its name and artwork), then SET_ACTIVITY frames set or
//! clear it. Discord answers every frame, and closes the connection for an unknown application.

use crate::jellyfin::Jellyfin;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

/// Bloom's application in the Discord Developer Portal: it names the activity and holds the
/// "bloom" artwork. Not a secret: every copy of Bloom shows presence through it.
const CLIENT_ID: &str = "1549258214246322297";
/// Updates arriving this close together (seeking about, the next episode starting) are sent as
/// one: Discord accepts only a few a minute.
const COALESCE: Duration = Duration::from_millis(750);
/// The longest updates are held back by a steady stream of them.
const COALESCE_MOST: Duration = Duration::from_secs(2);
/// How long the connection stays open once nothing is shown. Closing it at once let the next
/// title's activity race Discord's handling of the closed connection, which could clear it.
const LINGER: Duration = Duration::from_secs(60);
/// While something should be shown but Discord isn't there, how often to look for it again.
const RETRY: Duration = Duration::from_secs(20);
/// How long one question to the server about a cover may take before the moth stands in.
const COVER_TIMEOUT: Duration = Duration::from_secs(5);
/// A title with no public cover is asked about again after this long.
const COVER_RETRY: Duration = Duration::from_secs(600);
#[cfg(unix)]
const IO_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(unix)]
const OP_HANDSHAKE: u32 = 0;
#[cfg(unix)]
const OP_FRAME: u32 = 1;
#[cfg(unix)]
const OP_CLOSE: u32 = 2;

/// Whether this build can show presence at all.
pub(crate) fn configured() -> bool {
    !CLIENT_ID.is_empty()
}

/// What's playing, as the player last knew it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Activity {
    /// Jellyfin's item type: "Episode", "Movie" and so on.
    pub(crate) kind: String,
    /// The show for an episode.
    pub(crate) title: String,
    /// "S1 E4, Name" for an episode, the year for a film.
    pub(crate) subtitle: Option<String>,
    pub(crate) position: f64,
    pub(crate) duration: Option<f64>,
    pub(crate) paused: bool,
    pub(crate) speed: f64,
    /// When `position` was read.
    pub(crate) at: Instant,
    /// Whose poster is the cover: the show for an episode, the item otherwise.
    pub(crate) art_item: Option<String>,
}

enum Message {
    Show(Option<Activity>),
    Settings { enabled: bool, show_title: bool },
}

pub struct Presence {
    tx: mpsc::UnboundedSender<Message>,
}

impl Presence {
    pub fn new(app: AppHandle, enabled: bool, show_title: bool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(run(app, rx, enabled, show_title));
        Self { tx }
    }

    /// What's playing now, or None once nothing is.
    pub(crate) fn show(&self, activity: Option<Activity>) {
        let _ = self.tx.send(Message::Show(activity));
    }

    pub(crate) fn set(&self, enabled: bool, show_title: bool) {
        let _ = self.tx.send(Message::Settings { enabled, show_title });
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteImagesDto {
    images: Vec<RemoteImageDto>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteImageDto {
    url: String,
}

/// The public metadata sites a cover may come from. Discord's friends see the address, so any
/// other host (the server itself, or the owner's own domain through a metadata plugin) is left out.
const COVER_HOSTS: [&str; 5] = ["image.tmdb.org", "cdn.anidb.net", "artworks.thetvdb.com", "assets.fanart.tv", "m.media-amazon.com"];

/// Whether an image can be shown to Discord: https from a public metadata site, with nothing
/// added that could carry something private (credentials, a query, a port).
fn public_image(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else { return false };
    parsed.scheme() == "https"
        && url.len() <= 256
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && matches!(parsed.host(), Some(url::Host::Domain(host)) if COVER_HOSTS.contains(&host))
}

/// A cover no larger than Discord shows it: TMDB serves any width from the same path.
fn smaller(url: String) -> String {
    url.replacen("image.tmdb.org/t/p/original/", "image.tmdb.org/t/p/w500/", 1)
}

/// The first public cover the server's metadata sources have for an item: its preferred source,
/// then TheMovieDb, then AniDB.
async fn find_cover(jf: &Jellyfin, item: &str) -> Option<String> {
    if item.is_empty() || !item.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    for provider in [None, Some("TheMovieDb"), Some("AniDB")] {
        let mut path = format!("/Items/{item}/RemoteImages?type=Primary&limit=10");
        if let Some(provider) = provider {
            path.push_str(&format!("&providerName={provider}"));
        }
        let Ok(Ok(page)) = tokio::time::timeout(COVER_TIMEOUT, jf.get_json::<RemoteImagesDto>(path)).await else {
            // No server, or a slow one: the moth, and no more asking this time.
            return None;
        };
        if let Some(url) = page.images.into_iter().map(|image| image.url).find(|url| public_image(url)) {
            return Some(smaller(url));
        }
    }
    None
}

/// The activity Discord shows, with the playhead at `position` at `now_ms`, and `cover` as its
/// picture where there is one.
fn activity_json(a: &Activity, show_title: bool, position: f64, now_ms: i64, cover: Option<&str>) -> Value {
    // Discord takes 2 to 128 characters in each line.
    let fit = |text: &str| -> Option<String> {
        let text: String = text.trim().chars().take(128).collect();
        (text.chars().count() >= 2).then_some(text)
    };
    let (details, subtitle) = if show_title {
        (fit(&a.title), a.subtitle.as_deref().and_then(fit))
    } else {
        let generic = match a.kind.as_str() {
            "Episode" => "Watching a show",
            "Movie" => "Watching a film",
            _ => "Watching a video",
        };
        (Some(generic.to_string()), None)
    };
    let state = match (a.paused, subtitle) {
        (true, Some(subtitle)) => fit(&format!("Paused, {subtitle}")),
        (true, None) => Some("Paused".to_string()),
        (false, subtitle) => subtitle,
    };
    // Type 3 reads as "Watching" on the profile. The cover is only ever sent with the title: it
    // would give the title away.
    let assets = match cover.filter(|_| show_title) {
        Some(cover) => json!({
            "large_image": cover,
            "large_text": fit(&a.title).unwrap_or_else(|| "Bloom".into()),
            "small_image": "bloom",
            "small_text": "Bloom",
        }),
        None => json!({ "large_image": "bloom", "large_text": "Bloom" }),
    };
    let mut activity = json!({ "type": 3, "assets": assets });
    if let Some(details) = details {
        activity["details"] = json!(details);
    }
    if let Some(state) = state {
        activity["state"] = json!(state);
    }
    // Paused, there's no clock: a time left that doesn't move would be wrong.
    if !a.paused && a.speed > 0.0 {
        let start = now_ms - (position.max(0.0) * 1000.0 / a.speed) as i64;
        let mut timestamps = json!({ "start": start });
        if let Some(duration) = a.duration.filter(|d| *d > 0.0) {
            timestamps["end"] = json!(start + (duration * 1000.0 / a.speed) as i64);
        }
        activity["timestamps"] = timestamps;
    }
    activity
}

/// SET_ACTIVITY, or its clearing with None.
fn set_activity(activity: Option<Value>) -> Value {
    json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": uuid::Uuid::new_v4().simple().to_string(),
    })
}

/// Where Discord listens: the desktop app, its Flatpak and Snap packages, and Vesktop's Flatpak,
/// under the runtime and temporary folders.
fn socket_paths() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> =
        ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    roots.push(PathBuf::from("/tmp"));
    let mut paths = Vec::new();
    for root in roots {
        for sub in ["", "app/com.discordapp.Discord", "snap.discord", ".flatpak/dev.vencord.Vesktop/xdg-run", "app/dev.vencord.Vesktop"] {
            for n in 0..10 {
                let path = root.join(sub).join(format!("discord-ipc-{n}"));
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

fn apply(message: Message, current: &mut Option<Activity>, enabled: &mut bool, show_title: &mut bool) {
    match message {
        Message::Show(activity) => *current = activity,
        Message::Settings { enabled: on, show_title: title } => {
            *enabled = on;
            *show_title = title;
        }
    }
}

#[cfg(unix)]
async fn run(app: AppHandle, mut rx: mpsc::UnboundedReceiver<Message>, mut enabled: bool, mut show_title: bool) {
    use tokio::net::UnixStream;

    let mut current: Option<Activity> = None;
    let mut socket: Option<UnixStream> = None;
    // Covers found per title this run; a title without one is asked about again after a while.
    let mut covers: HashMap<String, (Option<String>, Instant)> = HashMap::new();
    // Something changed while a cover was looked up: decide again before waiting.
    let mut recheck = false;
    loop {
        let looking = enabled && configured() && current.is_some() && socket.is_none();
        let message = if std::mem::take(&mut recheck) {
            None
        } else if looking {
            match tokio::time::timeout(RETRY, rx.recv()).await {
                Ok(None) => return,
                Ok(Some(message)) => Some(message),
                Err(_) => None,
            }
        } else if socket.is_some() && !(enabled && configured() && current.is_some()) {
            // Cleared, and the connection kept for a while in case something plays again.
            match tokio::time::timeout(LINGER, rx.recv()).await {
                Ok(None) => return,
                Ok(Some(message)) => Some(message),
                Err(_) => {
                    socket = None;
                    continue;
                }
            }
        } else {
            match rx.recv().await {
                None => return,
                message => message,
            }
        };
        if let Some(message) = message {
            apply(message, &mut current, &mut enabled, &mut show_title);
            let until = Instant::now() + COALESCE_MOST;
            while let Some(left) = until.checked_duration_since(Instant::now()) {
                match tokio::time::timeout(COALESCE.min(left), rx.recv()).await {
                    Ok(Some(message)) => apply(message, &mut current, &mut enabled, &mut show_title),
                    _ => break,
                }
            }
        }

        // A copy, since what's playing can change while a cover is looked up.
        match current.clone().filter(|_| enabled && configured()) {
            Some(activity) => {
                if socket.is_none() {
                    socket = ipc::connect().await;
                }
                if let Some(stream) = socket.as_mut() {
                    let item = activity.art_item.as_deref().filter(|_| show_title);
                    let cover = match item {
                        Some(item) => {
                            let known = covers.get(item).filter(|(found, at)| found.is_some() || at.elapsed() < COVER_RETRY);
                            match known {
                                Some((found, _)) => found.clone(),
                                None => {
                                    let found = find_cover(&app.state::<Jellyfin>(), item).await;
                                    covers.insert(item.to_string(), (found.clone(), Instant::now()));
                                    // The lookup can take seconds: presence or the title may have
                                    // been turned off meanwhile, so nothing is sent from before.
                                    let mut changed = false;
                                    while let Ok(message) = rx.try_recv() {
                                        apply(message, &mut current, &mut enabled, &mut show_title);
                                        changed = true;
                                    }
                                    if changed {
                                        recheck = true;
                                        continue;
                                    }
                                    found
                                }
                            }
                        }
                        None => None,
                    };
                    let position = activity.position + if activity.paused { 0.0 } else { activity.at.elapsed().as_secs_f64() * activity.speed };
                    let payload = set_activity(Some(activity_json(&activity, show_title, position, now_ms(), cover.as_deref())));
                    match ipc::send(stream, OP_FRAME, &payload).await {
                        // Refused, such as a field Discord doesn't accept: nothing shows until the next update.
                        Ok(answer) if answer["evt"] == "ERROR" => {
                            eprintln!("bloom: Discord didn't take the activity: {}", answer["data"]);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("bloom: Discord presence connection lost: {e}");
                            socket = None;
                        }
                    }
                }
            }
            None => {
                // Nothing to show: clear it, keeping the connection for a while (LINGER).
                if let Some(stream) = socket.as_mut() {
                    if ipc::send(stream, OP_FRAME, &set_activity(None)).await.is_err() {
                        socket = None;
                    }
                }
            }
        }
    }
}

#[cfg(not(unix))]
async fn run(_app: AppHandle, mut rx: mpsc::UnboundedReceiver<Message>, mut enabled: bool, mut show_title: bool) {
    let mut current = None;
    while let Some(message) = rx.recv().await {
        apply(message, &mut current, &mut enabled, &mut show_title);
    }
}

#[cfg(unix)]
mod ipc {
    use super::{socket_paths, CLIENT_ID, IO_TIMEOUT, OP_CLOSE, OP_HANDSHAKE};
    use serde_json::{json, Value};
    use std::io;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    pub(super) async fn connect() -> Option<UnixStream> {
        // No checking for each path first, which would block: a missing one fails to connect.
        for path in socket_paths() {
            let Ok(mut stream) = UnixStream::connect(&path).await else { continue };
            // Answered with READY, or closed for an application Discord doesn't know.
            if send(&mut stream, OP_HANDSHAKE, &json!({ "v": 1, "client_id": CLIENT_ID })).await.is_ok() {
                return Some(stream);
            }
        }
        None
    }

    pub(super) fn frame(op: u32, payload: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(payload).unwrap_or_default();
        let mut frame = Vec::with_capacity(8 + body.len());
        frame.extend_from_slice(&op.to_le_bytes());
        frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
        frame.extend_from_slice(&body);
        frame
    }

    /// Sends a frame and reads Discord's answer, so answers never pile up unread.
    pub(super) async fn send(stream: &mut UnixStream, op: u32, payload: &Value) -> io::Result<Value> {
        let timed_out = |_| io::Error::from(io::ErrorKind::TimedOut);
        tokio::time::timeout(IO_TIMEOUT, stream.write_all(&frame(op, payload))).await.map_err(timed_out)??;
        let (answer, body) = tokio::time::timeout(IO_TIMEOUT, read_frame(stream)).await.map_err(timed_out)??;
        if answer == OP_CLOSE {
            Err(io::Error::other("Discord closed the connection"))
        } else {
            Ok(body)
        }
    }

    async fn read_frame(stream: &mut UnixStream) -> io::Result<(u32, Value)> {
        let mut header = [0u8; 8];
        stream.read_exact(&mut header).await?;
        let op = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if len > 1 << 20 {
            return Err(io::Error::other("an answer too large to be Discord's"));
        }
        let mut body = vec![0; len];
        stream.read_exact(&mut body).await?;
        Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(paused: bool, speed: f64) -> Activity {
        Activity {
            kind: "Episode".into(),
            title: "Tokyo Ghoul".into(),
            subtitle: Some("S1 E1, Tragedy".into()),
            position: 600.0,
            duration: Some(1440.0),
            paused,
            speed,
            at: Instant::now(),
            art_item: Some("show1".into()),
        }
    }

    #[test]
    fn covers_are_public_https_images_off_the_server() {
        assert!(public_image("https://image.tmdb.org/t/p/original/1m4RlC9BTCbyY549TOdVQ5NRPcR.jpg"));
        assert!(public_image("https://cdn.anidb.net/images/main/156337.jpg"));
        assert!(public_image("https://artworks.thetvdb.com/banners/posters/81189-1.jpg"));
        // As the Shoko plugin served covers on the dev server: from the server itself.
        assert!(!public_image("http://192.168.1.20:8097/Shokofin/Host/Image/TMDB/Poster/269"));
        // The owner's own public domain, through a plugin, is still theirs.
        assert!(!public_image("https://jellyfin.example.com/Items/x/Images/Primary"));
        assert!(!public_image("https://covers.owner.example/poster.jpg"));
        assert!(!public_image("https://192.168.1.5/cover.jpg"));
        assert!(!public_image("https://nas.local/cover.jpg"));
        assert!(!public_image("http://image.tmdb.org/t/p/w500/x.jpg"), "plain http");
        assert!(!public_image("https://image.tmdb.org.evil.example/x.jpg"), "a look-alike host");
        assert!(!public_image("https://user:pw@image.tmdb.org/t/p/w500/x.jpg"), "credentials");
        assert!(!public_image("https://image.tmdb.org/t/p/w500/x.jpg?api_key=secret"), "a query");
        assert!(!public_image("https://image.tmdb.org:8443/t/p/w500/x.jpg"), "a port");
        assert_eq!(
            smaller("https://image.tmdb.org/t/p/original/abc.jpg".into()),
            "https://image.tmdb.org/t/p/w500/abc.jpg"
        );
    }

    #[test]
    fn the_cover_is_the_picture_and_the_moth_the_badge() {
        let cover = "https://image.tmdb.org/t/p/w500/abc.jpg";
        let json = activity_json(&episode(false, 1.0), true, 600.0, 1_000_000, Some(cover));
        assert_eq!(json["assets"]["large_image"], cover);
        assert_eq!(json["assets"]["large_text"], "Tokyo Ghoul");
        assert_eq!(json["assets"]["small_image"], "bloom");
        let hidden = activity_json(&episode(false, 1.0), false, 600.0, 1_000_000, Some(cover));
        assert_eq!(hidden["assets"]["large_image"], "bloom", "the cover was sent with the title hidden");
    }

    #[test]
    fn playing_shows_the_title_episode_and_time_left() {
        let json = activity_json(&episode(false, 1.0), true, 600.0, 1_000_000, None);
        assert_eq!(json["type"], 3);
        assert_eq!(json["details"], "Tokyo Ghoul");
        assert_eq!(json["state"], "S1 E1, Tragedy");
        assert_eq!(json["timestamps"]["start"], 400_000);
        assert_eq!(json["timestamps"]["end"], 1_840_000);
        assert_eq!(json["assets"]["large_image"], "bloom");
    }

    #[test]
    fn paused_hidden_and_faster() {
        let paused = activity_json(&episode(true, 1.0), true, 600.0, 1_000_000, None);
        assert_eq!(paused["state"], "Paused, S1 E1, Tragedy");
        assert!(paused.get("timestamps").is_none(), "a paused clock was sent");

        let hidden = activity_json(&episode(false, 1.0), false, 600.0, 1_000_000, None);
        assert_eq!(hidden["details"], "Watching a show");
        assert!(hidden.get("state").is_none(), "the episode was sent with the title hidden");
        assert!(!hidden.to_string().contains("Tokyo"));

        let fast = activity_json(&episode(false, 2.0), true, 600.0, 1_000_000, None);
        assert_eq!(fast["timestamps"]["start"], 700_000);
        assert_eq!(fast["timestamps"]["end"], 1_420_000);

        let long = Activity { title: "x".repeat(300), subtitle: Some("1".into()), ..episode(false, 1.0) };
        let json = activity_json(&long, true, 0.0, 0, None);
        assert_eq!(json["details"].as_str().unwrap().chars().count(), 128);
        assert!(json.get("state").is_none(), "a one-character line was sent");
    }

    #[test]
    fn clearing_and_framing() {
        let clear = set_activity(None);
        assert_eq!(clear["cmd"], "SET_ACTIVITY");
        assert!(clear["args"]["activity"].is_null());
        #[cfg(unix)]
        {
            let frame = ipc::frame(1, &json!({ "a": 1 }));
            assert_eq!(&frame[..4], &1u32.to_le_bytes());
            assert_eq!(&frame[4..8], &7u32.to_le_bytes());
            assert_eq!(&frame[8..], br#"{"a":1}"#);
        }
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            assert!(socket_paths().contains(&PathBuf::from(runtime).join("discord-ipc-0")));
        }
    }
}
