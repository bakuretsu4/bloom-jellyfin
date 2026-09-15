//! Downloads: films and episodes saved to disk, to play with no server.
//!
//! One transfer runs at a time, in the order queued, for the account signed in; other accounts'
//! downloads wait until they're in use again. Original quality fetches the file itself through
//! `/Items/{id}/Download`, which answers ranged requests, so a paused or interrupted download
//! carries on from the byte it stopped at. 1080p and 720p have the server convert the file as it
//! sends it: that stream has no length and can't be resumed, so resuming one starts it again,
//! and its size is an estimate until it's done.
//!
//! A conversion is only offered when the file is heavier than it: converting a light HEVC file
//! to H.264 at a fixed bitrate makes it bigger, not smaller.
//!
//! Videos are laid out as media servers expect, so the folder reads well in a file manager (and
//! another player, or a Jellyfin library, could use it as it is):
//!   {location}/Show/Season 1/Show - S01E04 - Name.mkv        an episode
//!   {location}/Show/Season 1/Show - S01E04 - Name.eng.3.srt  one of its text subtitles
//!   {location}/Film (2015)/Film (2015).mkv                   a film
//!   .{name}.part beside them while a transfer runs
//! What only Bloom needs stays in the app data folder:
//!   downloads.json            every download, its state and its offline resume point
//!   downloads/{downloadId}/   the item's page and skip segments (item.json), and its artwork
//!
//! Watching a downloaded copy keeps its resume point here as well as telling the server. Points
//! recorded while the server wasn't answering are sent with /UserItems/{id}/UserData once it is,
//! unless the server heard of a later watch in the meantime.

use crate::home::{Card, Image};
use crate::jellyfin::{Error, Jellyfin};
use crate::playback::{self, DownloadFacts, LocalSubtitle, Plan, Quality, StoredImage, TrackPreferences, UpNext};
use crate::settings::SettingsStore;
use futures_util::future::Either;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{watch, Notify};

/// The page hears a running transfer's progress this often.
const EMIT_INTERVAL: Duration = Duration::from_millis(500);
const SAVE_INTERVAL: Duration = Duration::from_secs(5);
/// How long the worker rests when nothing can start, and how often it sends offline resume points.
const IDLE_INTERVAL: Duration = Duration::from_secs(30);
/// An interrupted transfer tries again this many times, further apart each time, then gives up.
const MAX_ATTEMPTS: u32 = 5;
/// The longest the workers after the first rest without being nudged.
const WORKER_REST: Duration = Duration::from_secs(5 * 60);
/// How often shows' saved pages missing from finished downloads are fetched again.
const SERIES_REFRESH: Duration = Duration::from_secs(10 * 60);
/// A resumable transfer that got this far before failing starts its tries over, so a long download
/// on a link that drops now and then still finishes.
const PROGRESS_FOR_FRESH_TRIES: u64 = 50_000_000;
/// The longest a title or episode name gets in a file name. Linux allows 255 bytes per name, and an
/// episode's is its show's and its own together, with a number, a subtitle's language and ".part".
const NAME_BYTES: usize = 80;
/// The most transfers Settings can let run at once.
pub(crate) const MAX_PARALLEL: u8 = 3;
/// Left free on the disk beyond what a download needs.
const HEADROOM: u64 = 512 * 1024 * 1024;
const TICKS_PER_SECOND: f64 = 10_000_000.0;
const AUDIO_BITRATE: u64 = 192_000;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Downloading,
    Paused,
    Failed,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    id: String,
    server_id: String,
    user_id: String,
    quality: Quality,
    status: Status,
    error: Option<String>,
    facts: DownloadFacts,
    /// The folder the video goes in.
    folder: PathBuf,
    /// The video's file name without its extension, which its subtitles share.
    #[serde(default)]
    stem: String,
    /// The download location it was saved under; folders it empties are removed up to here.
    #[serde(default)]
    root: PathBuf,
    /// Bloom's own files for it: the page and artwork.
    #[serde(default)]
    meta: PathBuf,
    /// The poster its cards use: its show's for an episode, its own for a film.
    #[serde(default)]
    poster: Option<StoredImage>,
    /// The finished video's name in the folder.
    media_file: Option<String>,
    #[serde(default)]
    subtitles: Vec<LocalSubtitle>,
    #[serde(default)]
    bytes_done: u64,
    /// Known for original quality; for a conversion, an estimate until it's done.
    bytes_total: Option<u64>,
    #[serde(default)]
    estimated: bool,
    #[serde(default)]
    attempts: u32,
    /// Seconds since the epoch before which an interrupted transfer isn't tried again.
    retry_at: Option<u64>,
    added_at: u64,
    finished_at: Option<u64>,
    /// Where watching it last stopped, on this device.
    #[serde(default)]
    position_seconds: f64,
    #[serde(default)]
    played: bool,
    last_played_at: Option<u64>,
    /// A resume point the server hasn't been sent yet.
    #[serde(default)]
    sync_pending: bool,
}

impl Entry {
    fn belongs_to(&self, server: &str, user: &str) -> bool {
        self.server_id == server && self.user_id == user
    }

    fn converted(&self) -> bool {
        self.quality != Quality::Original
    }

    fn part_path(&self) -> PathBuf {
        self.folder.join(format!(".{}.part", self.stem))
    }

    /// "Name.eng.3.srt": the video's name, so players find it, then the language and stream.
    fn subtitle_name(&self, index: i64, lang: &str, format: &str) -> String {
        let lang: String = lang.chars().filter(char::is_ascii_alphanumeric).take(8).collect();
        if lang.is_empty() {
            format!("{}.{index}.{format}", self.stem)
        } else {
            format!("{}.{lang}.{index}.{format}", self.stem)
        }
    }

    fn media_path(&self) -> Option<PathBuf> {
        self.media_file.as_ref().map(|name| self.folder.join(name))
    }

    /// Bloom's folder for this download, only when it's the one named for it.
    fn owned_meta(&self) -> Option<&Path> {
        let named = self.meta.file_name()?.to_str()? == self.id;
        let inside = self.meta.parent()?.file_name()?.to_str()? == "downloads";
        (named && inside && safe_token(&self.id)).then_some(self.meta.as_path())
    }
}

/// A download as the page sees it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    id: String,
    item_id: String,
    kind: String,
    title: String,
    subtitle: Option<String>,
    series_id: Option<String>,
    season_id: Option<String>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
    image: Option<Image>,
    quality: Quality,
    /// The qualities worth choosing: Original, and conversions lighter than the file.
    qualities: Vec<Quality>,
    status: Status,
    error: Option<String>,
    /// Why a queued download isn't moving: "network" (it waits for the local network) or "server".
    waiting: Option<&'static str>,
    bytes_done: u64,
    bytes_total: Option<u64>,
    estimated: bool,
    bytes_per_second: Option<f64>,
    runtime_seconds: Option<f64>,
    position_seconds: f64,
    played: bool,
    added_at: u64,
    finished_at: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Control {
    Run,
    Pause,
    Cancel,
    /// The quality changed: throw away what's there and start again.
    Restart,
}

struct Active {
    id: String,
    control: watch::Sender<Control>,
    /// When the speed was last measured, and the bytes done then.
    sample: (Instant, u64),
    speed: Option<f64>,
    /// The quality changed while this ran: what it wrote is the old quality, however it ends.
    needs_reset: bool,
    /// Bytes done when this run began.
    started_bytes: u64,
}

#[derive(Default)]
struct Inner {
    entries: Vec<Entry>,
    /// Transfers running now, up to the "downloads at once" setting.
    active: Vec<Active>,
    waiting: HashMap<String, &'static str>,
    last_emit: Option<Instant>,
    last_save: Option<Instant>,
}

pub struct Downloads {
    app: AppHandle,
    index: PathBuf,
    /// Where each download's page and artwork are kept.
    meta_root: PathBuf,
    /// `~/Videos/Bloom`, used unless Settings names another folder.
    default_location: PathBuf,
    inner: Mutex<Inner>,
    /// Wakes the worker: something was queued or resumed.
    wake: Notify,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The server and user signed in.
fn account(jf: &Jellyfin) -> Option<(String, String)> {
    Some((jf.server_id().ok()?, jf.user_id().ok()?))
}

impl Downloads {
    pub fn new(app: AppHandle, dir: &Path, default_location: PathBuf) -> Self {
        let index = dir.join("downloads.json");
        let meta_root = dir.join("downloads");
        let mut entries: Vec<Entry> = fs::read(&index).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut migrated = false;
        for entry in &mut entries {
            // Bloom closed mid-transfer: carry on.
            if entry.status == Status::Downloading {
                entry.status = Status::Queued;
            }
            if entry.stem.is_empty() {
                migrate(entry, &meta_root);
                migrated = true;
            }
            // Finished before posters were kept.
            if entry.status == Status::Done && entry.poster.is_none() {
                entry.poster = poster_from_page(&entry.meta);
                migrated |= entry.poster.is_some();
            }
        }
        migrate_series_folders(&entries, &meta_root);
        let downloads = Self {
            app,
            index,
            meta_root,
            default_location,
            inner: Mutex::new(Inner { entries, ..Inner::default() }),
            wake: Notify::new(),
        };
        if migrated {
            let mut inner = downloads.lock();
            downloads.save(&mut inner);
        }
        downloads
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Written while the lock is held, so two saves can't land in the wrong order.
    fn save(&self, inner: &mut Inner) {
        inner.last_save = Some(Instant::now());
        let Ok(bytes) = serde_json::to_vec(&inner.entries) else { return };
        let tmp = self.index.with_extension("tmp");
        if fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &self.index)).is_err() {
            eprintln!("bloom: couldn't save the downloads list to {}", self.index.display());
        }
    }

    fn views_in(inner: &Inner, server: &str, user: &str) -> Vec<DownloadView> {
        inner
            .entries
            .iter()
            .filter(|e| e.belongs_to(server, user))
            .map(|e| DownloadView {
                id: e.id.clone(),
                item_id: e.facts.item_id.clone(),
                kind: e.facts.kind.clone(),
                title: e.facts.title.clone(),
                subtitle: e.facts.subtitle.clone(),
                series_id: e.facts.series_id.clone(),
                season_id: e.facts.season_id.clone(),
                season_number: e.facts.season_number,
                episode_number: e.facts.episode_number,
                image: e.facts.image.as_ref().and_then(|i| i.image()),
                quality: e.quality,
                qualities: offered_qualities(&e.facts),
                status: e.status,
                error: e.error.clone(),
                waiting: (e.status == Status::Queued).then(|| inner.waiting.get(&e.id).copied()).flatten(),
                bytes_done: e.bytes_done,
                bytes_total: e.bytes_total,
                estimated: e.estimated,
                bytes_per_second: inner.active.iter().find(|a| a.id == e.id).and_then(|a| a.speed),
                runtime_seconds: e.facts.runtime_seconds,
                position_seconds: e.position_seconds,
                played: e.played,
                added_at: e.added_at,
                finished_at: e.finished_at,
            })
            .collect()
    }

    pub(crate) fn views(&self, jf: &Jellyfin) -> Vec<DownloadView> {
        let Some((server, user)) = account(jf) else { return Vec::new() };
        Self::views_in(&self.lock(), &server, &user)
    }

    /// Tell the page. Progress ticks are throttled; `force` is for everything else.
    fn changed(&self, inner: &mut Inner, force: bool) {
        let now = Instant::now();
        if !force && inner.last_emit.is_some_and(|t| now.duration_since(t) < EMIT_INTERVAL) {
            return;
        }
        inner.last_emit = Some(now);
        let jf = self.app.state::<Jellyfin>();
        let Some((server, user)) = account(&jf) else { return };
        let views = serde_json::to_value(Self::views_in(inner, &server, &user)).unwrap_or_default();
        let _ = self.app.emit("downloads:changed", views);
    }

    fn location(&self, store: &SettingsStore) -> PathBuf {
        store.get().download_location.as_deref().map(expand_home).unwrap_or_else(|| self.default_location.clone())
    }

    /// Queue an item. Already listed, a paused or failed one is queued again instead.
    pub(crate) async fn queue(&self, jf: &Jellyfin, store: &SettingsStore, item_id: &str, quality: Option<Quality>) -> Result<(), Error> {
        let (server, user) = account(jf).ok_or(Error::SignedOut)?;
        let listed = |e: &Entry| e.belongs_to(&server, &user) && e.facts.item_id == item_id;
        {
            let mut inner = self.lock();
            if let Some(entry) = inner.entries.iter_mut().find(|e| listed(e)) {
                if matches!(entry.status, Status::Paused | Status::Failed) {
                    requeue(entry);
                    self.save(&mut inner);
                    self.changed(&mut inner, true);
                    drop(inner);
                    self.nudge();
                }
                return Ok(());
            }
        }
        let facts = playback::download_facts_with(jf, item_id).await?;
        // A conversion no lighter than the file would only make it bigger.
        let quality = effective_quality(quality.unwrap_or(store.get().download_quality), &facts);
        let id = uuid::Uuid::new_v4().simple().to_string();
        let mut entry = Entry {
            server_id: server.clone(),
            user_id: user.clone(),
            quality,
            status: Status::Queued,
            error: None,
            folder: PathBuf::new(),
            stem: String::new(),
            root: self.location(store),
            meta: self.meta_root.join(&id),
            id,
            poster: None,
            media_file: None,
            subtitles: Vec::new(),
            bytes_done: 0,
            bytes_total: if quality == Quality::Original { facts.size } else { estimate(quality, facts.runtime_seconds) },
            estimated: quality != Quality::Original,
            attempts: 0,
            retry_at: None,
            added_at: now_secs(),
            finished_at: None,
            position_seconds: 0.0,
            played: false,
            last_played_at: None,
            sync_pending: false,
            facts,
        };
        let mut inner = self.lock();
        // Asked for twice at once (a double click): the first stands.
        if !inner.entries.iter().any(listed) {
            (entry.folder, entry.stem) = unique_layout(&inner.entries, &entry.root, &entry.facts);
            inner.entries.push(entry);
            self.save(&mut inner);
            self.changed(&mut inner, true);
        }
        drop(inner);
        self.nudge();
        Ok(())
    }

    /// Queue every episode of a season. Resolves to how many are queued or already listed.
    pub(crate) async fn queue_season(
        &self,
        jf: &Jellyfin,
        store: &SettingsStore,
        series_id: &str,
        season_id: &str,
        quality: Option<Quality>,
    ) -> Result<u32, Error> {
        let page = crate::library::season_episodes_with(jf, series_id, season_id).await?;
        let mut queued = 0;
        let mut first_error = None;
        for episode in page.episodes {
            match self.queue(jf, store, &episode.id, quality).await {
                Ok(()) => queued += 1,
                Err(Error::SignedOut) => return Err(Error::SignedOut),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        match first_error {
            Some(e) if queued == 0 => Err(e),
            _ => Ok(queued),
        }
    }

    pub(crate) fn pause(&self, id: &str) {
        let mut inner = self.lock();
        if let Some(active) = inner.active.iter().find(|a| a.id == id) {
            // The worker marks it paused once the transfer has stopped. A cancel on its way stands.
            if *active.control.borrow() != Control::Cancel {
                let _ = active.control.send(Control::Pause);
            }
            return;
        }
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id && e.status == Status::Queued) {
            entry.status = Status::Paused;
            self.save(&mut inner);
            self.changed(&mut inner, true);
        }
    }

    pub(crate) fn resume(&self, id: &str) {
        let mut inner = self.lock();
        let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id && matches!(e.status, Status::Paused | Status::Failed)) else {
            return;
        };
        requeue(entry);
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        self.nudge();
    }

    /// Cancel a download, or delete a finished one, with its files.
    pub(crate) fn remove(&self, id: &str) {
        let mut inner = self.lock();
        if let Some(active) = inner.active.iter().find(|a| a.id == id) {
            // The worker removes it once the transfer has stopped writing.
            let _ = active.control.send(Control::Cancel);
            return;
        }
        let Some(index) = inner.entries.iter().position(|e| e.id == id) else { return };
        let entry = inner.entries.remove(index);
        inner.waiting.remove(id);
        let orphan = self.orphaned_series(&inner, &entry);
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        delete_files(&entry);
        if let Some(folder) = orphan {
            let _ = fs::remove_dir_all(folder);
        }
    }

    /// Another quality for one download. Whatever was fetched at the old one is thrown away.
    pub(crate) fn set_quality(&self, id: &str, quality: Quality) {
        let mut inner = self.lock();
        let Some(index) = inner.entries.iter().position(|e| e.id == id) else { return };
        let quality = effective_quality(quality, &inner.entries[index].facts);
        if inner.entries[index].quality == quality {
            return;
        }
        inner.entries[index].quality = quality;
        let part = inner.entries[index].part_path();
        match inner.active.iter().position(|a| a.id == id) {
            Some(a) => {
                // Whatever the transfer does next (a pause can land before it stops), what's on
                // disk is the old quality, and `finish` throws it away. The part file goes now, so
                // Bloom closing first can't resume it at the new quality either.
                inner.active[a].needs_reset = true;
                let _ = fs::remove_file(&part);
                if *inner.active[a].control.borrow() != Control::Cancel {
                    let _ = inner.active[a].control.send(Control::Restart);
                }
            }
            None => reset_transfer(&mut inner.entries[index]),
        }
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        self.nudge();
    }

    /// A finished download of the item for the account signed in, whose video is still on disk.
    pub(crate) fn playable(&self, jf: &Jellyfin, item_id: &str) -> Option<Playable> {
        let (server, user) = account(jf)?;
        let inner = self.lock();
        let entry = inner
            .entries
            .iter()
            .find(|e| e.belongs_to(&server, &user) && e.facts.item_id == item_id && e.status == Status::Done)?;
        entry.media_path().filter(|p| p.is_file())?;
        Some(Playable { entry: entry.clone() })
    }

    /// Where watching a download got to. `stopped` ends a session, rather than a progress tick,
    /// and is when the server is sent the point.
    pub(crate) fn record_watch(&self, id: &str, position: f64, duration: Option<f64>, stopped: bool, finished: bool) {
        let mut inner = self.lock();
        let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id) else { return };
        let (resume, watched) = resume_after(position, duration.or(entry.facts.runtime_seconds), finished);
        entry.position_seconds = resume;
        entry.played |= watched;
        entry.last_played_at = Some(now_secs());
        entry.sync_pending = true;
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        if stopped {
            let app = self.app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = sync_with(&app.state::<Jellyfin>(), &app.state::<Downloads>()).await;
            });
        }
    }

    /// The downloaded episode after this one in its show, for up next with no server.
    pub(crate) fn next_downloaded(&self, jf: &Jellyfin, item_id: &str) -> Option<UpNext> {
        let (server, user) = account(jf)?;
        let inner = self.lock();
        let mine = |e: &&Entry| e.belongs_to(&server, &user);
        let current = inner.entries.iter().filter(mine).find(|e| e.facts.item_id == item_id)?;
        let series = current.facts.series_id.clone()?;
        let order = |e: &Entry| (e.facts.season_number.unwrap_or(0), e.facts.episode_number.unwrap_or(0));
        let here = order(current);
        inner
            .entries
            .iter()
            .filter(mine)
            .filter(|e| e.status == Status::Done && e.facts.series_id.as_deref() == Some(series.as_str()) && order(e) > here)
            .min_by_key(|e| order(e))
            .map(|e| UpNext {
                item_id: e.facts.item_id.clone(),
                title: e.facts.title.clone(),
                subtitle: e.facts.subtitle.clone(),
                image: e.facts.image.as_ref().and_then(|i| i.image()),
            })
    }

    fn saved_page(&self, jf: &Jellyfin, item_id: &str) -> Option<Value> {
        let (server, user) = account(jf)?;
        let meta = self.lock().entries.iter().find(|e| e.belongs_to(&server, &user) && e.facts.item_id == item_id)?.meta.clone();
        serde_json::from_slice(&fs::read(meta.join("item.json")).ok()?).ok()
    }

    /// A downloaded item's page, as it was when it was downloaded.
    /// For a show, its page narrowed to what's downloaded.
    pub(crate) fn saved_detail(&self, jf: &Jellyfin, item_id: &str) -> Option<Value> {
        self.saved_page(jf, item_id)
            .and_then(|page| page.get("detail").cloned())
            .filter(|v| v.is_object())
            .or_else(|| self.saved_series(jf, item_id))
    }

    /// A show's finished downloads for the account signed in, in episode order.
    fn downloaded_episodes(&self, jf: &Jellyfin, series_id: &str) -> Vec<Entry> {
        let Some((server, user)) = account(jf) else { return Vec::new() };
        let mut episodes: Vec<Entry> = self
            .lock()
            .entries
            .iter()
            .filter(|e| e.belongs_to(&server, &user) && e.status == Status::Done && e.facts.series_id.as_deref() == Some(series_id))
            .cloned()
            .collect();
        episodes.sort_by_key(|e| (e.facts.season_number.unwrap_or(0), e.facts.episode_number.unwrap_or(0)));
        episodes
    }

    fn saved_series(&self, jf: &Jellyfin, series_id: &str) -> Option<Value> {
        let episodes = self.downloaded_episodes(jf, series_id);
        if episodes.is_empty() {
            return None;
        }
        let saved = series_folder(&self.meta_root, &episodes[0].server_id, series_id)
            .and_then(|folder| fs::read(folder.join("item.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|page| page.get("detail").cloned())
            .filter(|v| v.is_object());
        Some(narrowed_series(saved, series_id, &episodes))
    }

    /// A season's downloaded episodes, as the episode grid expects them.
    pub(crate) fn saved_episodes(&self, jf: &Jellyfin, series_id: &str, season_id: &str) -> Option<Value> {
        let episodes = self.downloaded_episodes(jf, series_id);
        (!episodes.is_empty()).then(|| episodes_page(&episodes, season_id))
    }

    /// The films and shows with something downloaded, most recent first.
    pub(crate) fn titles(&self, jf: &Jellyfin) -> Vec<Card> {
        let Some((server, user)) = account(jf) else { return Vec::new() };
        let inner = self.lock();
        title_cards(inner.entries.iter().filter(|e| e.belongs_to(&server, &user) && e.status == Status::Done).collect())
    }

    pub(crate) fn search_titles(&self, jf: &Jellyfin, query: &str, limit: u32) -> Vec<Card> {
        let query = query.trim().to_lowercase();
        if query.chars().count() < 2 {
            return Vec::new();
        }
        self.titles(jf).into_iter().filter(|card| card.title().to_lowercase().contains(&query)).take(limit as usize).collect()
    }

    /// The saved page of a show whose last download is going.
    fn orphaned_series(&self, inner: &Inner, removed: &Entry) -> Option<PathBuf> {
        let series = removed.facts.series_id.as_deref()?;
        let still_used = inner.entries.iter().any(|e| e.server_id == removed.server_id && e.facts.series_id.as_deref() == Some(series));
        if still_used {
            None
        } else {
            series_folder(&self.meta_root, &removed.server_id, series)
        }
    }

    pub(crate) fn saved_segments(&self, jf: &Jellyfin, item_id: &str) -> Option<Value> {
        self.saved_page(jf, item_id)?.get("segments").cloned().filter(|v| v.is_array())
    }

    /// A downloaded copy of an image, for pages opened with no server.
    pub(crate) fn artwork(&self, server_id: &str, item: &str, kind: &str, tag: &str) -> Option<Vec<u8>> {
        let name = art_name(item, kind, tag);
        let folders: Vec<PathBuf> = {
            let inner = self.lock();
            let mine = || inner.entries.iter().filter(|e| e.server_id == server_id);
            let mut folders: Vec<PathBuf> = mine().map(|e| e.meta.clone()).collect();
            folders.extend(mine().filter_map(|e| series_folder(&self.meta_root, server_id, e.facts.series_id.as_deref()?)));
            folders.sort();
            folders.dedup();
            folders
        };
        folders.iter().find_map(|folder| fs::read(folder.join(&name)).ok())
    }

    pub(crate) fn storage(&self, store: &SettingsStore) -> Storage {
        let location = self.location(store);
        let space = disk_space(&location);
        Storage {
            bloom_bytes: self.lock().entries.iter().map(|e| e.bytes_done).sum(),
            location: location.to_string_lossy().into_owned(),
            free_bytes: space.map(|(free, _)| free),
            total_bytes: space.map(|(_, total)| total),
        }
    }

    /// How many downloads there are from a server, for every account on it, and their size.
    pub(crate) fn server_summary(&self, server_id: &str) -> ServerDownloads {
        let inner = self.lock();
        let from = inner.entries.iter().filter(|e| e.server_id == server_id);
        ServerDownloads { count: from.clone().count(), bytes: from.map(|e| e.bytes_done).sum() }
    }

    /// Deletes every download from a server, for every account on it, with its files: for
    /// removing the server. Transfers running for it stop first, and are deleted once they have.
    pub(crate) fn remove_server(&self, server_id: &str) -> usize {
        let mut inner = self.lock();
        let running: Vec<String> = inner
            .active
            .iter()
            .filter(|a| inner.entries.iter().any(|e| e.id == a.id && e.server_id == server_id))
            .map(|a| {
                let _ = a.control.send(Control::Cancel);
                a.id.clone()
            })
            .collect();
        let (gone, kept): (Vec<Entry>, Vec<Entry>) =
            std::mem::take(&mut inner.entries).into_iter().partition(|e| e.server_id == server_id && !running.contains(&e.id));
        inner.entries = kept;
        for entry in &gone {
            inner.waiting.remove(&entry.id);
        }
        let mut series: Vec<PathBuf> = gone
            .iter()
            .filter_map(|e| e.facts.series_id.as_deref())
            .filter(|s| !inner.entries.iter().any(|e| e.server_id == server_id && e.facts.series_id.as_deref() == Some(*s)))
            .filter_map(|s| series_folder(&self.meta_root, server_id, s))
            .collect();
        series.sort();
        series.dedup();
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        for entry in &gone {
            delete_files(entry);
        }
        for folder in series {
            let _ = fs::remove_dir_all(folder);
        }
        gone.len() + running.len()
    }

    // --- the worker's side

    /// Wakes the workers: something was queued, a transfer ended, or the setting changed.
    pub(crate) fn nudge(&self) {
        self.wake.notify_waiters();
        // And one that's between waits.
        self.wake.notify_one();
    }

    /// Waits for a nudge, or at most `rest`, in case one came between checks.
    async fn idle(&self, rest: Duration) {
        let _ = tokio::time::timeout(rest, self.wake.notified()).await;
    }

    fn next_ready(&self, jf: &Jellyfin) -> Option<Entry> {
        let (server, user) = account(jf)?;
        let now = now_secs();
        self.lock()
            .entries
            .iter()
            .find(|e| e.belongs_to(&server, &user) && e.status == Status::Queued && e.retry_at.is_none_or(|t| t <= now))
            .cloned()
    }

    /// Marks every queued download for the account as waiting, or clears the marks.
    fn set_waiting(&self, jf: &Jellyfin, reason: Option<&'static str>) {
        let Some((server, user)) = account(jf) else { return };
        let mut inner = self.lock();
        let queued: Vec<String> =
            inner.entries.iter().filter(|e| e.belongs_to(&server, &user) && e.status == Status::Queued).map(|e| e.id.clone()).collect();
        let before = inner.waiting.clone();
        for id in queued {
            match reason {
                Some(reason) => inner.waiting.insert(id, reason),
                None => inner.waiting.remove(&id),
            };
        }
        if inner.waiting != before {
            self.changed(&mut inner, true);
        }
    }

    fn begin(&self, id: &str, control: watch::Sender<Control>) -> bool {
        let mut inner = self.lock();
        let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id && e.status == Status::Queued) else {
            // Paused or removed since it was picked.
            return false;
        };
        entry.status = Status::Downloading;
        entry.error = None;
        let bytes = entry.bytes_done;
        inner.waiting.remove(id);
        inner.active.push(Active {
            id: id.into(),
            control,
            sample: (Instant::now(), bytes),
            speed: None,
            needs_reset: false,
            started_bytes: bytes,
        });
        self.save(&mut inner);
        self.changed(&mut inner, true);
        true
    }

    fn progress(&self, id: &str, bytes_done: u64, bytes_total: Option<u64>) {
        let mut inner = self.lock();
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id) {
            entry.bytes_done = bytes_done;
            entry.bytes_total = bytes_total;
        }
        if let Some(active) = inner.active.iter_mut().find(|a| a.id == id) {
            let elapsed = active.sample.0.elapsed();
            if elapsed >= Duration::from_secs(1) {
                let rate = bytes_done.saturating_sub(active.sample.1) as f64 / elapsed.as_secs_f64();
                active.speed = Some(active.speed.map_or(rate, |speed| speed * 0.6 + rate * 0.4));
                active.sample = (Instant::now(), bytes_done);
            }
        }
        if inner.last_save.is_none_or(|t| t.elapsed() >= SAVE_INTERVAL) {
            self.save(&mut inner);
        }
        self.changed(&mut inner, false);
    }

    fn finish(&self, id: &str, outcome: Outcome) {
        let mut inner = self.lock();
        let active = inner.active.iter().position(|a| a.id == id).map(|i| inner.active.remove(i));
        let Some(index) = inner.entries.iter().position(|e| e.id == id) else { return };
        let now = now_secs();
        let mut removed = None;
        let mut waiting_for_server = false;
        let mut outcome = outcome;
        let asked = active.as_ref().map(|a| *a.control.borrow());
        // A cancel that lands as the transfer finishes still cancels it.
        if asked == Some(Control::Cancel) && !matches!(outcome, Outcome::Cancelled) {
            if let Outcome::Done { file, .. } = &outcome {
                let _ = fs::remove_file(inner.entries[index].folder.join(file));
            }
            outcome = Outcome::Cancelled;
        }
        // The quality changed while this ran: what's on disk is the old quality.
        if active.as_ref().is_some_and(|a| a.needs_reset) && !matches!(outcome, Outcome::Cancelled) {
            if let Outcome::Done { file, .. } = &outcome {
                let _ = fs::remove_file(inner.entries[index].folder.join(file));
            }
            if matches!(outcome, Outcome::Paused) {
                reset_transfer(&mut inner.entries[index]);
            } else {
                outcome = Outcome::Restart;
            }
        }
        match outcome {
            Outcome::Done { file, bytes, subtitles } => {
                let entry = &mut inner.entries[index];
                entry.status = Status::Done;
                entry.media_file = Some(file);
                entry.bytes_done = bytes;
                entry.bytes_total = Some(bytes);
                entry.estimated = false;
                entry.subtitles = subtitles;
                entry.finished_at = Some(now);
                entry.poster = poster_from_page(&entry.meta);
                entry.attempts = 0;
                entry.retry_at = None;
                entry.error = None;
            }
            Outcome::Paused => inner.entries[index].status = Status::Paused,
            Outcome::Cancelled => removed = Some(inner.entries.remove(index)),
            Outcome::Restart => reset_transfer(&mut inner.entries[index]),
            Outcome::Offline => {
                let entry = &mut inner.entries[index];
                entry.status = Status::Queued;
                entry.retry_at = Some(now + IDLE_INTERVAL.as_secs());
                waiting_for_server = true;
            }
            Outcome::Retry(message) => {
                let entry = &mut inner.entries[index];
                if !entry.converted() && active.as_ref().is_some_and(|a| entry.bytes_done >= a.started_bytes + PROGRESS_FOR_FRESH_TRIES) {
                    entry.attempts = 0;
                }
                entry.attempts += 1;
                if entry.attempts >= MAX_ATTEMPTS {
                    entry.status = Status::Failed;
                    entry.error = Some(message);
                } else {
                    entry.status = Status::Queued;
                    entry.retry_at = Some(now + IDLE_INTERVAL.as_secs() * u64::from(entry.attempts));
                }
            }
            Outcome::Failed(message) => {
                let entry = &mut inner.entries[index];
                entry.status = Status::Failed;
                entry.error = Some(message);
            }
        }
        if waiting_for_server {
            inner.waiting.insert(id.to_string(), "server");
        }
        let orphan = removed.as_ref().and_then(|entry| self.orphaned_series(&inner, entry));
        self.save(&mut inner);
        self.changed(&mut inner, true);
        drop(inner);
        // A worker is free: the next queued download can start.
        self.nudge();
        if let Some(entry) = removed {
            delete_files(&entry);
        }
        if let Some(folder) = orphan {
            let _ = fs::remove_dir_all(folder);
        }
    }
}

/// A finished download, ready to play.
pub(crate) struct Playable {
    entry: Entry,
}

impl Playable {
    pub(crate) fn plan(&self, uid: &str, start_seconds: f64, preferences: TrackPreferences) -> Plan {
        let e = &self.entry;
        let subtitles: Vec<(LocalSubtitle, String)> =
            e.subtitles.iter().map(|s| (s.clone(), e.folder.join(&s.file).to_string_lossy().into_owned())).collect();
        let file = e.media_path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        playback::local_plan(uid, &e.id, &e.facts, &file, e.converted(), &subtitles, e.quality, start_seconds, preferences)
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct UserDataDto {
    playback_position_ticks: i64,
    played: bool,
    last_played_date: Option<String>,
}

/// Where a downloaded copy starts: what this device watched and hasn't sent is the latest there
/// is; otherwise the server's resume point, if it answers quickly; otherwise this device's.
pub(crate) async fn resume_point(jf: &Jellyfin, local: &Playable) -> f64 {
    let entry = &local.entry;
    if entry.sync_pending {
        return entry.position_seconds;
    }
    let Ok(uid) = jf.user_id() else { return entry.position_seconds };
    let path = format!("/UserItems/{}/UserData?userId={uid}", entry.facts.item_id);
    match tokio::time::timeout(Duration::from_secs(3), jf.get_json::<UserDataDto>(path)).await {
        Ok(Ok(data)) => data.playback_position_ticks.max(0) as f64 / TICKS_PER_SECOND,
        _ => entry.position_seconds,
    }
}

/// Where a watch leaves a download: its resume point, and whether it now counts as watched. Past
/// 90% it's watched and starts over next time; under 5% it isn't worth resuming. These are
/// Jellyfin's own defaults, so a point sent later means the same on the server.
fn resume_after(position: f64, length: Option<f64>, finished: bool) -> (f64, bool) {
    if finished {
        return (0.0, true);
    }
    match length.filter(|l| *l > 0.0) {
        Some(length) if position >= length * 0.9 => (0.0, true),
        Some(length) if position < length * 0.05 => (0.0, false),
        _ => (position.max(0.0), false),
    }
}

/// Sends the resume points recorded while watching downloads, unless the server heard of a
/// later watch, in which case the server's is kept here instead. Resolves to how many were sent.
pub(crate) async fn sync_with(jf: &Jellyfin, dl: &Downloads) -> Result<u32, Error> {
    let Some((server, user)) = account(jf) else { return Ok(0) };
    let pending: Vec<(String, String, f64, bool, u64)> = dl
        .lock()
        .entries
        .iter()
        .filter(|e| e.belongs_to(&server, &user) && e.sync_pending)
        .map(|e| (e.id.clone(), e.facts.item_id.clone(), e.position_seconds, e.played, e.last_played_at.unwrap_or(0)))
        .collect();
    let mut sent = 0;
    for (id, item, position, played, at) in pending {
        let path = format!("/UserItems/{item}/UserData?userId={user}");
        let theirs: UserDataDto = match jf.get_json(path.clone()).await {
            Ok(theirs) => theirs,
            Err(e) => match sync_miss(e) {
                SyncMiss::Stop(e) => return Err(e),
                SyncMiss::Drop => {
                    dl.synced(&id, at, None);
                    continue;
                }
                SyncMiss::Later => continue,
            },
        };
        if theirs.last_played_date.as_deref().and_then(parse_date).is_some_and(|t| t > at) {
            let position = theirs.playback_position_ticks.max(0) as f64 / TICKS_PER_SECOND;
            dl.synced(&id, at, Some((position, theirs.played)));
        } else {
            let body = json!({
                "PlaybackPositionTicks": (position * TICKS_PER_SECOND) as i64,
                "Played": played,
                "LastPlayedDate": format_date(at),
            });
            if let Err(e) = jf.post_empty(&path, body).await {
                match sync_miss(e) {
                    SyncMiss::Stop(e) => return Err(e),
                    SyncMiss::Drop => dl.synced(&id, at, None),
                    SyncMiss::Later => {}
                }
                continue;
            }
            dl.synced(&id, at, None);
            sent += 1;
        }
    }
    Ok(sent)
}

/// What one item failing to sync means for the rest.
enum SyncMiss {
    /// The server is gone or the sign-in ended: stop, and try the whole round again later.
    Stop(Error),
    /// The server no longer has the item, or won't take it from this account: nothing to send.
    Drop,
    /// Anything else: this one again next round, the others now.
    Later,
}

fn sync_miss(e: Error) -> SyncMiss {
    match e {
        Error::Unreachable(_) | Error::SignedOut => SyncMiss::Stop(e),
        Error::Status(403 | 404) => SyncMiss::Drop,
        _ => SyncMiss::Later,
    }
}

impl Downloads {
    /// The download location has gone though downloads were saved there: most likely a drive that
    /// isn't connected, where making the folder again would fill the disk underneath instead.
    fn location_missing(&self, root: &Path) -> bool {
        !root.as_os_str().is_empty()
            && !root.is_dir()
            && self.lock().entries.iter().any(|e| e.root.as_path() == root && e.status == Status::Done)
    }

    fn synced(&self, id: &str, at: u64, servers: Option<(f64, bool)>) {
        let mut inner = self.lock();
        let Some(entry) = inner.entries.iter_mut().find(|e| e.id == id) else { return };
        // Watched again while this was on its way: that one goes next time.
        if entry.last_played_at != Some(at) {
            return;
        }
        entry.sync_pending = false;
        if let Some((position, played)) = servers {
            entry.position_seconds = position;
            entry.played = played;
        }
        self.save(&mut inner);
        self.changed(&mut inner, true);
    }
}

/// Runs transfers one at a time, and sends offline resume points, for as long as Bloom runs.
pub fn start(app: AppHandle) {
    // One worker per transfer that may run at once. A worker only takes work while its number is
    // under the setting, so lowering it lets running transfers finish before those workers rest.
    for worker in 0..MAX_PARALLEL {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { work(app, worker).await });
    }
}

async fn work(app: AppHandle, worker: u8) {
    {
        let dl = app.state::<Downloads>();
        let jf = app.state::<Jellyfin>();
        let store = app.state::<SettingsStore>();
        let mut last_sync: Option<Instant> = None;
        let mut last_series: Option<Instant> = None;
        // The first worker wakes on a timer, for retries coming due and the upkeep below; the
        // others rest until there's something to do.
        let rest = if worker == 0 { IDLE_INTERVAL } else { WORKER_REST };
        loop {
            // The first worker also keeps offline resume points and shows' pages up to date.
            if worker == 0 && last_sync.is_none_or(|t| t.elapsed() >= IDLE_INTERVAL) {
                last_sync = Some(Instant::now());
                let _ = sync_with(&jf, &dl).await;
            }
            if worker == 0 && last_series.is_none_or(|t| t.elapsed() >= SERIES_REFRESH) {
                last_series = Some(Instant::now());
                refresh_series_pages(&jf, &dl).await;
            }
            if worker >= store.get().download_parallel.clamp(1, MAX_PARALLEL) {
                dl.idle(rest).await;
                continue;
            }
            let Some(entry) = dl.next_ready(&jf) else {
                dl.idle(rest).await;
                continue;
            };
            if store.get().download_local_only && !is_local_address(&jf.server_address().unwrap_or_default()).await {
                dl.set_waiting(&jf, Some("network"));
                dl.idle(rest).await;
                continue;
            }
            dl.set_waiting(&jf, None);
            let (control, receiver) = watch::channel(Control::Run);
            if !dl.begin(&entry.id, control) {
                continue;
            }
            let id = entry.id.clone();
            let outcome = transfer(&jf, &dl, &store, entry, receiver).await;
            dl.finish(&id, outcome);
        }
    }
}

enum Outcome {
    Done { file: String, bytes: u64, subtitles: Vec<LocalSubtitle> },
    Paused,
    Cancelled,
    Restart,
    /// The server isn't answering: wait for it, without counting a failed try.
    Offline,
    /// Interrupted: try again later.
    Retry(String),
    Failed(String),
}

fn outcome_of(e: Error) -> Outcome {
    match e {
        Error::Unreachable(_) | Error::SignedOut => Outcome::Offline,
        Error::Status(403) => Outcome::Failed("Your Jellyfin account isn't allowed to download from this server.".into()),
        Error::Status(404) => Outcome::Failed("The server no longer has this file.".into()),
        Error::Download(_) | Error::DownloadFolder(_) | Error::NotPlayable(_) => Outcome::Failed(e.to_string()),
        other => Outcome::Retry(other.to_string()),
    }
}

fn folder_failed(e: std::io::Error) -> Outcome {
    Outcome::Failed(Error::DownloadFolder(e.to_string()).to_string())
}

fn control_outcome(control: Control) -> Outcome {
    match control {
        Control::Pause => Outcome::Paused,
        Control::Cancel => Outcome::Cancelled,
        Control::Restart => Outcome::Restart,
        Control::Run => Outcome::Retry("The transfer stopped.".into()),
    }
}

fn stopped(control: &watch::Receiver<Control>) -> Option<Outcome> {
    let current = *control.borrow();
    (current != Control::Run).then(|| control_outcome(current))
}

async fn transfer(jf: &Jellyfin, dl: &Downloads, store: &SettingsStore, entry: Entry, mut control: watch::Receiver<Control>) -> Outcome {
    // Requests go out as whoever is signed in: after a switch to another account they'd go to
    // that one, so the download waits for its own account to come back.
    let ours = || account(jf).is_some_and(|(server, user)| entry.belongs_to(&server, &user));
    if !ours() {
        return Outcome::Offline;
    }
    if dl.location_missing(&entry.root) {
        return Outcome::Failed(
            Error::DownloadFolder(format!("{} isn't there. If it's on a drive, connect it and try again.", entry.root.display()))
                .to_string(),
        );
    }
    if let Err(e) = fs::create_dir_all(&entry.folder).and_then(|_| fs::create_dir_all(&entry.meta)) {
        return folder_failed(e);
    }
    // The page and artwork first, so a finished download always has them.
    if !entry.meta.join("item.json").is_file() {
        if let Err(outcome) = save_page(jf, &entry).await {
            return outcome;
        }
    }
    if let Some(outcome) = stopped(&control) {
        return outcome;
    }
    if !ours() {
        return Outcome::Offline;
    }
    let subtitles = match save_subtitles(jf, &entry).await {
        Ok(subtitles) => subtitles,
        Err(outcome) => return outcome,
    };
    if let Some(outcome) = stopped(&control) {
        return outcome;
    }
    if !ours() {
        return Outcome::Offline;
    }
    let video = if entry.converted() {
        fetch_converted(jf, dl, store, &entry, &mut control, &ours).await
    } else {
        fetch_original(jf, dl, &entry, &mut control, &ours).await
    };
    match video {
        Ok(Some((file, bytes))) => Outcome::Done { file, bytes, subtitles },
        Ok(None) => control_outcome(*control.borrow()),
        Err(outcome) => outcome,
    }
}

fn art_name(item: &str, kind: &str, tag: &str) -> String {
    format!("art-{item}-{kind}-{tag}")
}

/// Ids and tags go into URLs and file names.
fn safe_token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Outcome> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, path)).map_err(folder_failed)
}

/// The item's page, skip segments and artwork, for playing and browsing it offline.
async fn save_page(jf: &Jellyfin, entry: &Entry) -> Result<(), Outcome> {
    let item = &entry.facts.item_id;
    let detail = crate::library::item_detail_with(jf, item).await.map_err(outcome_of)?;
    let detail = serde_json::to_value(&detail).unwrap_or(Value::Null);
    let segments = match crate::library::skip_segments_with(jf, item).await {
        Ok(segments) => serde_json::to_value(segments).unwrap_or(Value::Null),
        Err(e @ (Error::Unreachable(_) | Error::SignedOut)) => return Err(outcome_of(e)),
        Err(_) => Value::Null,
    };

    let mut images: Vec<(String, String, String)> = Vec::new();
    if let Some(image) = &entry.facts.image {
        images.push((image.item_id.clone(), image.kind.clone(), image.tag.clone()));
    }
    images.extend([&detail["backdrop"], &detail["poster"], &detail["logo"], &detail["series"]["poster"]].into_iter().filter_map(image_reference));
    save_art(jf, &entry.meta, images).await.map_err(outcome_of)?;
    let page = serde_json::to_vec(&json!({ "detail": detail, "segments": segments })).unwrap_or_default();
    write_file(&entry.meta.join("item.json"), &page)?;
    // The show's page as well, so it opens with no server. Refreshed with every episode; only
    // the server going away is worth stopping the download for.
    if let (Some(series), Some(meta_root)) = (entry.facts.series_id.as_deref(), entry.meta.parent()) {
        if let Err(e @ (Error::Unreachable(_) | Error::SignedOut)) = save_series_page(jf, meta_root, &entry.server_id, series).await {
            return Err(outcome_of(e));
        }
    }
    Ok(())
}

/// An image as an item page's JSON has it: (item, kind, tag).
fn image_reference(v: &Value) -> Option<(String, String, String)> {
    Some((v["itemId"].as_str()?.to_string(), v["kind"].as_str()?.to_string(), v["tag"].as_str()?.to_string()))
}

/// Saves artwork into `folder`, skipping what's already there. Artwork the server doesn't have is
/// only a placeholder offline, so only the server going away is an error.
async fn save_art(jf: &Jellyfin, folder: &Path, images: Vec<(String, String, String)>) -> Result<(), Error> {
    for (item, kind, tag) in images {
        if !safe_token(&item) || !safe_token(&tag) || !["Primary", "Thumb", "Backdrop", "Logo"].contains(&kind.as_str()) {
            continue;
        }
        let path = folder.join(art_name(&item, &kind, &tag));
        if path.is_file() {
            continue;
        }
        let width = if matches!(kind.as_str(), "Backdrop" | "Thumb") { 1280 } else { 720 };
        match jf.get(&format!("/Items/{item}/Images/{kind}?tag={tag}&maxWidth={width}&quality=90")).await {
            Ok(res) => {
                if let Ok(bytes) = res.bytes().await {
                    let tmp = path.with_extension("tmp");
                    fs::write(&tmp, &bytes).and_then(|_| fs::rename(&tmp, &path)).map_err(|e| Error::DownloadFolder(e.to_string()))?;
                }
            }
            Err(e @ (Error::Unreachable(_) | Error::SignedOut)) => return Err(e),
            Err(_) => {}
        }
    }
    Ok(())
}

/// Where a show's saved page and artwork are kept, shared by its downloaded episodes from one
/// server (two servers can have a show under the same id).
fn series_folder(meta_root: &Path, server_id: &str, series_id: &str) -> Option<PathBuf> {
    (safe_token(server_id) && safe_token(series_id)).then(|| meta_root.join(format!("series-{server_id}-{series_id}")))
}

/// Shows' pages were once kept in one folder per show for every server. Moved to their server's
/// folder, or deleted where downloads from two servers share the show's id (each server's page is
/// saved again the next time it's reachable).
fn migrate_series_folders(entries: &[Entry], meta_root: &Path) {
    let mut servers: HashMap<&str, Vec<&str>> = HashMap::new();
    for entry in entries {
        if let Some(series) = entry.facts.series_id.as_deref() {
            servers.entry(series).or_default().push(&entry.server_id);
        }
    }
    for (series, mut ids) in servers {
        if !safe_token(series) {
            continue;
        }
        let legacy = meta_root.join(format!("series-{series}"));
        if !legacy.is_dir() {
            continue;
        }
        ids.sort_unstable();
        ids.dedup();
        match (ids.as_slice(), ids.first().and_then(|id| series_folder(meta_root, id, series))) {
            ([_], Some(folder)) if !folder.exists() => {
                let _ = fs::rename(&legacy, folder);
            }
            _ => {
                let _ = fs::remove_dir_all(&legacy);
            }
        }
    }
}

async fn save_series_page(jf: &Jellyfin, meta_root: &Path, server_id: &str, series_id: &str) -> Result<(), Error> {
    let folder = series_folder(meta_root, server_id, series_id).ok_or(Error::Status(400))?;
    let detail = serde_json::to_value(crate::library::item_detail_with(jf, series_id).await?).unwrap_or(Value::Null);
    fs::create_dir_all(&folder).map_err(|e| Error::DownloadFolder(e.to_string()))?;
    let images = [&detail["backdrop"], &detail["poster"], &detail["logo"]].into_iter().filter_map(image_reference).collect();
    save_art(jf, &folder, images).await?;
    let bytes = serde_json::to_vec(&json!({ "detail": detail })).unwrap_or_default();
    let tmp = folder.join("item.tmp");
    fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, folder.join("item.json"))).map_err(|e| Error::DownloadFolder(e.to_string()))
}

/// Saves the show's page for downloaded episodes without one: downloaded before shows' pages
/// were kept, or while that request failed.
async fn refresh_series_pages(jf: &Jellyfin, dl: &Downloads) {
    let Some((server, user)) = account(jf) else { return };
    let mut missing: Vec<String> = dl
        .lock()
        .entries
        .iter()
        .filter(|e| e.belongs_to(&server, &user) && e.status == Status::Done)
        .filter_map(|e| e.facts.series_id.clone())
        .filter(|series| series_folder(&dl.meta_root, &server, series).is_some_and(|folder| !folder.join("item.json").is_file()))
        .collect();
    missing.sort();
    missing.dedup();
    for series in missing {
        if let Err(Error::Unreachable(_) | Error::SignedOut) = save_series_page(jf, &dl.meta_root, &server, &series).await {
            return;
        }
    }
}

/// The poster a download's cards use: its show's for an episode, its own for a film.
fn poster_from_page(meta: &Path) -> Option<StoredImage> {
    let page: Value = serde_json::from_slice(&fs::read(meta.join("item.json")).ok()?).ok()?;
    let detail = &page["detail"];
    let (item_id, kind, tag) = image_reference(&detail["series"]["poster"]).or_else(|| image_reference(&detail["poster"]))?;
    Some(StoredImage { item_id, kind, tag })
}

/// A show stands for all its downloaded episodes.
fn title_id(entry: &Entry) -> &str {
    match entry.facts.series_id.as_deref() {
        Some(series) if entry.facts.kind == "Episode" => series,
        _ => &entry.facts.item_id,
    }
}

/// One poster card per film or show, most recently finished first; a show says how many of its
/// episodes are downloaded.
fn title_cards(mut done: Vec<&Entry>) -> Vec<Card> {
    done.sort_by_key(|e| std::cmp::Reverse(e.finished_at.unwrap_or(0)));
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut firsts: Vec<&Entry> = Vec::new();
    for entry in &done {
        let count = counts.entry(title_id(entry)).or_insert(0);
        if *count == 0 {
            firsts.push(entry);
        }
        *count += 1;
    }
    firsts
        .into_iter()
        .map(|e| {
            let image = e.poster.as_ref().or(e.facts.image.as_ref()).and_then(StoredImage::image);
            let id = title_id(e).to_string();
            if id != e.facts.item_id {
                let n = counts[id.as_str()];
                let meta = format!("{n} {} downloaded", if n == 1 { "episode" } else { "episodes" });
                Card::new(id, "Series".into(), e.facts.title.clone(), Some(meta), image, None)
            } else {
                let progress = e
                    .facts
                    .runtime_seconds
                    .filter(|r| *r > 0.0 && e.position_seconds > 0.0 && !e.played)
                    .map(|r| (e.position_seconds / r).min(1.0));
                let year = film_year(&e.facts).map(|y| y.to_string());
                Card::new(id, e.facts.kind.clone(), e.facts.title.clone(), year, image, progress)
            }
        })
        .collect()
}

fn season_label(number: Option<i32>) -> String {
    match number {
        Some(0) => "Specials".into(),
        Some(n) => format!("Season {n}"),
        None => "Episodes".into(),
    }
}

/// A show's page with no server: the page saved while online (or a plain one from the
/// downloads), narrowed to the seasons with downloaded episodes, with Play on the first
/// unfinished one. `episodes` is in episode order and not empty.
fn narrowed_series(saved: Option<Value>, series_id: &str, episodes: &[Entry]) -> Value {
    let first = &episodes[0];
    let mut detail = saved.unwrap_or_else(|| {
        json!({
            "id": series_id, "kind": "Series", "title": first.facts.title, "year": null, "endYear": null,
            "status": null, "officialRating": null, "communityRating": null, "genres": [], "studios": [],
            "tagline": null, "overview": null, "backdrop": null,
            "poster": first.poster.as_ref().and_then(StoredImage::image), "logo": null,
            "audioLanguages": [], "subtitleLanguages": [], "favorite": false, "played": false,
            "runtimeMinutes": null, "episodeCount": 0, "unplayedCount": 0, "seasons": [], "play": null,
            "people": [], "media": null, "chapters": [], "series": null, "seasonId": null, "seasonName": null,
        })
    });
    let known = detail["seasons"].as_array().cloned().unwrap_or_default();
    let mut seasons: Vec<Value> = Vec::new();
    for e in episodes {
        let season_id = e.facts.season_id.clone().unwrap_or_default();
        match seasons.iter().position(|s| s["id"] == season_id.as_str()) {
            Some(i) => {
                let season = &mut seasons[i];
                season["episodeCount"] = json!(season["episodeCount"].as_u64().unwrap_or(0) + 1);
                season["unplayed"] = json!(season["unplayed"].as_u64().unwrap_or(0) + u64::from(!e.played));
            }
            None => {
                let name = known
                    .iter()
                    .find(|s| s["id"] == season_id.as_str())
                    .and_then(|s| s["name"].as_str())
                    .map_or_else(|| season_label(e.facts.season_number), String::from);
                seasons.push(json!({
                    "id": season_id, "name": name, "number": e.facts.season_number,
                    "episodeCount": 1, "unplayed": u32::from(!e.played),
                }));
            }
        }
    }
    let next = episodes
        .iter()
        .find(|e| !e.played && e.position_seconds > 0.0)
        .or_else(|| episodes.iter().find(|e| !e.played))
        .unwrap_or(first);
    let resume = !next.played && next.position_seconds > 0.0;
    let code = match (next.facts.season_number, next.facts.episode_number) {
        (Some(s), Some(n)) => format!("S{s} E{n}"),
        (None, Some(n)) => format!("E{n}"),
        _ => episode_name(&next.facts).to_string(),
    };
    detail["seasons"] = json!(seasons);
    detail["play"] = json!({
        "itemId": next.facts.item_id,
        "label": format!("{} {code}", if resume { "Resume" } else { "Play" }),
        "resume": resume,
        "seasonId": next.facts.season_id,
        "episodeNumber": next.facts.episode_number,
    });
    detail["episodeCount"] = json!(episodes.len());
    detail["unplayedCount"] = json!(episodes.iter().filter(|e| !e.played).count());
    detail
}

/// One season's downloaded episodes, shaped like `library::EpisodePage`.
fn episodes_page(episodes: &[Entry], season_id: &str) -> Value {
    let shown: Vec<Value> = episodes
        .iter()
        .filter(|e| e.facts.season_id.as_deref() == Some(season_id))
        .map(|e| {
            let f = &e.facts;
            let mut audio: Vec<String> = Vec::new();
            for lang in f.streams.iter().filter(|s| s.kind == "Audio").filter_map(|s| s.language.clone()) {
                if lang != "und" && !audio.contains(&lang) {
                    audio.push(lang);
                }
            }
            json!({
                "id": f.item_id,
                "number": f.episode_number,
                "title": episode_name(f),
                "overview": f.overview,
                "runtimeMinutes": f.runtime_seconds.map(|s| (s / 60.0).round() as u32).filter(|m| *m > 0),
                "image": f.image.as_ref().and_then(StoredImage::image),
                "played": e.played,
                "progress": f.runtime_seconds.filter(|r| *r > 0.0 && e.position_seconds > 0.0 && !e.played).map(|r| (e.position_seconds / r).min(1.0)),
                "premiereDate": null,
                "audioLanguages": audio,
                "hasSubtitles": f.streams.iter().any(|s| s.kind == "Subtitle"),
            })
        })
        .collect();
    json!({ "total": shown.len(), "episodes": shown })
}

/// Text subtitles as files beside the video: the server's separate files, and for a converted
/// download every text subtitle, since the conversion carries none.
async fn save_subtitles(jf: &Jellyfin, entry: &Entry) -> Result<Vec<LocalSubtitle>, Outcome> {
    let facts = &entry.facts;
    if !safe_token(&facts.media_source_id) {
        return Ok(Vec::new());
    }
    let mut saved = Vec::new();
    for stream in facts.streams.iter().filter(|s| s.kind == "Subtitle" && s.text && (s.external || entry.converted())) {
        let format = match stream.codec.as_deref().unwrap_or("") {
            "ass" => "ass",
            "ssa" => "ssa",
            "webvtt" | "vtt" => "vtt",
            _ => "srt",
        };
        let file = entry.subtitle_name(stream.index, stream.language.as_deref().unwrap_or(""), format);
        let path = entry.folder.join(&file);
        if !path.is_file() {
            let url = format!("/Videos/{}/{}/Subtitles/{}/0/Stream.{format}", facts.item_id, facts.media_source_id, stream.index);
            let bytes = match jf.transfer(&url, 0).await {
                Ok(res) => res.bytes().await.map_err(|_| Outcome::Retry("A subtitle's transfer was interrupted.".into()))?,
                Err(e @ (Error::Unreachable(_) | Error::SignedOut)) => return Err(outcome_of(e)),
                // One subtitle the server can't convert isn't worth the download.
                Err(_) => continue,
            };
            write_file(&path, &bytes)?;
        }
        saved.push(LocalSubtitle {
            index: stream.index,
            file,
            title: stream.title.clone().unwrap_or_default(),
            lang: stream.language.clone().unwrap_or_default(),
        });
    }
    Ok(saved)
}

fn bytes_label(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

fn check_space(folder: &Path, needed: u64) -> Result<(), Outcome> {
    match disk_space(folder) {
        Some((free, _)) if free < needed.saturating_add(HEADROOM) => Err(Outcome::Failed(format!(
            "Not enough space: this needs {} and {} is free.",
            bytes_label(needed),
            bytes_label(free)
        ))),
        _ => Ok(()),
    }
}

/// Copies a response into `out` until it ends (true) or the control changes (false). Stops as
/// Offline if another account signs in meanwhile (`ours`, checked about once a second).
async fn stream_body(
    mut res: reqwest::Response,
    out: &mut impl Write,
    written: &mut u64,
    mut progress: impl FnMut(u64),
    control: &mut watch::Receiver<Control>,
    ours: &(dyn Fn() -> bool + Sync),
) -> Result<bool, Outcome> {
    let mut checked = Instant::now();
    loop {
        if *control.borrow() != Control::Run {
            return Ok(false);
        }
        if checked.elapsed() >= Duration::from_secs(1) {
            checked = Instant::now();
            if !ours() {
                return Err(Outcome::Offline);
            }
        }
        let chunk = {
            let next = std::pin::pin!(res.chunk());
            let changed = std::pin::pin!(control.changed());
            match futures_util::future::select(next, changed).await {
                Either::Left((chunk, _)) => chunk,
                Either::Right(_) => return Ok(false),
            }
        };
        match chunk {
            Ok(Some(bytes)) => {
                out.write_all(&bytes).map_err(folder_failed)?;
                *written += bytes.len() as u64;
                progress(*written);
            }
            Ok(None) => return Ok(true),
            Err(_) => return Err(Outcome::Retry("The transfer was interrupted.".into())),
        }
    }
}

/// The file itself, carrying on from a partial file when the server answers the range.
async fn fetch_original(
    jf: &Jellyfin,
    dl: &Downloads,
    entry: &Entry,
    control: &mut watch::Receiver<Control>,
    ours: &(dyn Fn() -> bool + Sync),
) -> Result<Option<(String, u64)>, Outcome> {
    let part = entry.part_path();
    let have = if entry.bytes_done > 0 { fs::metadata(&part).map_or(0, |m| m.len()) } else { 0 };
    let name = format!("{}.{}", entry.stem, extension(entry.facts.container.as_deref()));
    let changed = || {
        let _ = fs::remove_file(&part);
        Outcome::Retry("The server's file changed since this download began; starting it again.".into())
    };
    if let Some(size) = entry.facts.size {
        // Stopped after the last byte arrived but before its end was noticed: nothing left to fetch.
        if have > 0 && have == size {
            fs::rename(&part, entry.folder.join(&name)).map_err(folder_failed)?;
            return Ok(Some((name, have)));
        }
        if have > size {
            return Err(changed());
        }
        check_space(&entry.folder, size.saturating_sub(have))?;
    }
    let res = match jf.transfer(&format!("/Items/{}/Download", entry.facts.item_id), have).await {
        // The server has no such range: the file is shorter than what's on disk.
        Err(Error::Status(416)) => return Err(changed()),
        other => other.map_err(outcome_of)?,
    };
    let resumed = have > 0 && res.status().as_u16() == 206;
    // The rest must carry on from exactly this byte of a file the same size, or the halves don't match.
    if resumed {
        let range = res.headers().get(reqwest::header::CONTENT_RANGE).and_then(|v| v.to_str().ok());
        if !range_continues(range, have, entry.facts.size) {
            return Err(changed());
        }
    }
    let mut written = if resumed { have } else { 0 };
    let total = res.content_length().map(|len| len + written).or(entry.facts.size);
    let file = if resumed {
        fs::OpenOptions::new().append(true).open(&part)
    } else {
        fs::File::create(&part)
    }
    .map_err(folder_failed)?;
    let mut out = BufWriter::with_capacity(256 * 1024, file);
    let ended = stream_body(res, &mut out, &mut written, |w| dl.progress(&entry.id, w, total), control, ours).await;
    out.flush().map_err(folder_failed)?;
    if !ended? {
        return Ok(None);
    }
    if total.is_some_and(|t| written < t) {
        return Err(Outcome::Retry("The transfer ended early.".into()));
    }
    fs::rename(&part, entry.folder.join(&name)).map_err(folder_failed)?;
    Ok(Some((name, written)))
}

/// Whether a Content-Range answer ("bytes 500-999/1000") starts at `from`, and is of a file the
/// size expected when that's known.
fn range_continues(header: Option<&str>, from: u64, size: Option<u64>) -> bool {
    let Some((span, total)) = header.and_then(|h| h.strip_prefix("bytes ")).and_then(|r| r.split_once('/')) else {
        return false;
    };
    let start = span.split_once('-').and_then(|(start, _)| start.trim().parse::<u64>().ok());
    let same_size = match size {
        Some(size) => total.trim().parse::<u64>().is_ok_and(|total| total == size),
        None => true,
    };
    start == Some(from) && same_size
}

/// The bitrate and largest frame a converted download is made at. Lower than the player's
/// streaming limits: a download is kept, so size matters more than headroom.
fn conversion(quality: Quality) -> (u64, u32, u32) {
    match quality {
        Quality::Hd720 => (4_000_000, 1280, 720),
        _ => (8_000_000, 1920, 1080),
    }
}

fn estimate(quality: Quality, runtime_seconds: Option<f64>) -> Option<u64> {
    let (video, _, _) = conversion(quality);
    runtime_seconds.map(|seconds| ((video + AUDIO_BITRATE) as f64 * seconds / 8.0) as u64)
}

/// A conversion made by the server as it sends it, always from the start.
async fn fetch_converted(
    jf: &Jellyfin,
    dl: &Downloads,
    store: &SettingsStore,
    entry: &Entry,
    control: &mut watch::Receiver<Control>,
    ours: &(dyn Fn() -> bool + Sync),
) -> Result<Option<(String, u64)>, Outcome> {
    let facts = &entry.facts;
    if !safe_token(&facts.media_source_id) {
        return Err(Outcome::Failed("The server described the file in a way Bloom can't use.".into()));
    }
    // The conversion is stopped on the server it runs on, even if another account signs in meanwhile.
    let who = jf.credentials().map_err(outcome_of)?;
    let uid = jf.user_id().map_err(outcome_of)?;
    let remembered = dl.app.state::<playback::Playback>().remembered(&playback::download_scope(&uid, facts));
    let audio = playback::download_audio(facts, remembered.as_ref(), &TrackPreferences::from_settings(&store.get()));
    let (video_bitrate, width, height) = conversion(entry.quality);
    let session = uuid::Uuid::new_v4().simple().to_string();
    let device = jf.device_id().to_string();
    let mut path = format!(
        "/Videos/{}/stream.mkv?mediaSourceId={}&deviceId={device}&playSessionId={session}&static=false\
         &videoCodec=h264&audioCodec=aac&maxWidth={width}&maxHeight={height}&videoBitRate={video_bitrate}\
         &audioBitRate={AUDIO_BITRATE}&audioChannels=2&copyTimestamps=false",
        facts.item_id, facts.media_source_id
    );
    if let Some(index) = audio {
        path.push_str(&format!("&audioStreamIndex={index}"));
    }
    let estimated = estimate(entry.quality, facts.runtime_seconds);
    if let Some(size) = estimated {
        check_space(&entry.folder, size)?;
    }

    let part = entry.part_path();
    let result = async {
        let res = jf.transfer(&path, 0).await.map_err(outcome_of)?;
        let mut out = BufWriter::with_capacity(256 * 1024, fs::File::create(&part).map_err(folder_failed)?);
        let mut written = 0;
        let ended =
            stream_body(res, &mut out, &mut written, |w| dl.progress(&entry.id, w, estimated.map(|e| e.max(w))), control, ours).await;
        out.flush().map_err(folder_failed)?;
        Ok::<_, Outcome>((ended?, written))
    }
    .await;
    // Otherwise the server keeps converting until it notices nobody is reading.
    let encoding = format!("/Videos/ActiveEncodings?deviceId={device}&playSessionId={session}");
    let _ = jf.request_as(&who, reqwest::Method::DELETE, &encoding, None).await;
    let (ended, written) = result?;
    if !ended {
        return Ok(None);
    }
    if written == 0 {
        return Err(Outcome::Retry("The server sent an empty file.".into()));
    }
    let name = format!("{}.mkv", entry.stem);
    fs::rename(&part, entry.folder.join(&name)).map_err(folder_failed)?;
    Ok(Some((name, written)))
}

/// Queued again from paused or failed, with a fresh set of tries.
fn requeue(entry: &mut Entry) {
    entry.status = Status::Queued;
    entry.error = None;
    entry.attempts = 0;
    entry.retry_at = None;
}

/// Throws away a download's video and subtitles, keeping its page and artwork, to fetch again.
fn reset_transfer(entry: &mut Entry) {
    let _ = fs::remove_file(entry.part_path());
    if let Some(path) = entry.media_path() {
        let _ = fs::remove_file(path);
    }
    for subtitle in &entry.subtitles {
        let _ = fs::remove_file(entry.folder.join(&subtitle.file));
    }
    for path in own_subtitle_files(entry) {
        let _ = fs::remove_file(path);
    }
    entry.media_file = None;
    entry.subtitles.clear();
    entry.bytes_done = 0;
    entry.bytes_total = if entry.converted() { estimate(entry.quality, entry.facts.runtime_seconds) } else { entry.facts.size };
    entry.estimated = entry.converted();
    entry.finished_at = None;
    requeue(entry);
}

/// Deletes a download's own files (never a folder with anything else in it), then the folders
/// that leaves empty, up to the download location.
fn delete_files(entry: &Entry) {
    let mut paths = vec![entry.part_path()];
    paths.extend(entry.media_path());
    paths.extend(entry.subtitles.iter().map(|s| entry.folder.join(&s.file)));
    paths.extend(own_subtitle_files(entry));
    paths.sort();
    paths.dedup();
    for path in paths {
        if let Err(e) = fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("bloom: couldn't delete {}: {e}", path.display());
            }
        }
    }
    if let Some(meta) = entry.owned_meta() {
        let _ = fs::remove_dir_all(meta);
    }
    prune_empty(&entry.folder, &entry.root);
}

/// Subtitle files beside the video named for it, whether or not the download finished: they're
/// saved before the video, so a cancel or a quality change would otherwise leave them behind.
fn own_subtitle_files(entry: &Entry) -> Vec<PathBuf> {
    if entry.stem.is_empty() {
        return Vec::new();
    }
    let prefix = format!("{}.", entry.stem);
    let Ok(files) = fs::read_dir(&entry.folder) else { return Vec::new() };
    files
        .flatten()
        .map(|file| file.file_name().to_string_lossy().into_owned())
        .filter(|name| name.strip_prefix(&prefix).is_some_and(is_subtitle_suffix))
        .map(|name| entry.folder.join(name))
        .collect()
}

/// "eng.3.srt" or "3.srt", the end of a name `Entry::subtitle_name` writes.
fn is_subtitle_suffix(rest: &str) -> bool {
    let parts: Vec<&str> = rest.split('.').collect();
    let (index, format) = match parts.as_slice() {
        [index, format] => (*index, *format),
        [lang, index, format] if (1..=8).contains(&lang.len()) && lang.bytes().all(|b| b.is_ascii_alphanumeric()) => (*index, *format),
        _ => return false,
    };
    !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) && matches!(format, "srt" | "ass" | "ssa" | "vtt")
}

/// Removes `from` and the folders above it while they're empty, stopping at `root`.
fn prune_empty(from: &Path, root: &Path) {
    if root.as_os_str().is_empty() {
        return;
    }
    let mut dir = Some(from);
    while let Some(current) = dir.filter(|d| *d != root && d.starts_with(root)) {
        // Only succeeds on an empty folder.
        if fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// A title as a file or folder name: readable, and safe on any filesystem.
fn clean_name(text: &str) -> String {
    let text = text.replace(": ", " - ");
    let mapped: String = text
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut name = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    // No hidden names, and never "." or "..".
    name = name.trim_matches(|c| c == '.' || c == ' ' || c == '-').to_string();
    // Cut by bytes, on a character boundary: Linux counts a name's bytes, and a title in Japanese
    // takes three per character.
    if name.len() > NAME_BYTES {
        let mut end = NAME_BYTES;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name = name[..end].trim_end().to_string();
    }
    name
}

/// The episode's own name. Downloads from before it was kept have it in the subtitle.
fn episode_name(facts: &DownloadFacts) -> &str {
    if !facts.name.is_empty() {
        return &facts.name;
    }
    facts.subtitle.as_deref().and_then(|s| s.split_once(", ")).map_or("", |(_, name)| name)
}

fn film_year(facts: &DownloadFacts) -> Option<i32> {
    facts.year.or_else(|| facts.subtitle.as_deref()?.parse().ok())
}

/// Where a download's video goes, as (folder, name without extension).
fn layout(root: &Path, facts: &DownloadFacts) -> (PathBuf, String) {
    let title = Some(clean_name(&facts.title)).filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into());
    if facts.kind == "Episode" {
        let mut folder = root.join(&title);
        match facts.season_number {
            Some(0) => folder.push("Specials"),
            Some(season) => folder.push(format!("Season {season}")),
            None => {}
        }
        let code = facts.episode_number.map(|e| format!("S{:02}E{:02}", facts.season_number.unwrap_or(0), e));
        let stem = [Some(title), code, Some(clean_name(episode_name(facts)))]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" - ");
        (folder, stem)
    } else {
        let label = match film_year(facts) {
            Some(year) => format!("{title} ({year})"),
            None => title,
        };
        (root.join(&label), label)
    }
}

/// The layout, with " (2)" and so on added when another download or another file has the name.
fn unique_layout(entries: &[Entry], root: &Path, facts: &DownloadFacts) -> (PathBuf, String) {
    let (folder, stem) = layout(root, facts);
    let extensions = [extension(facts.container.as_deref()), "mkv".to_string()];
    let taken = |candidate: &str| {
        entries.iter().any(|e| e.folder == folder && e.stem == candidate)
            || extensions.iter().any(|ext| folder.join(format!("{candidate}.{ext}")).exists())
    };
    let stem = (1..)
        .map(|n| if n == 1 { stem.clone() } else { format!("{stem} ({n})") })
        .find(|candidate| !taken(candidate))
        .expect("a numbered name is always free");
    (folder, stem)
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    // Across disks (the app data folder and the download location), a rename can't.
    fs::rename(from, to).or_else(|_| fs::copy(from, to).and_then(|_| fs::remove_file(from)))
}

/// Moves a file if it's there. True once it's at `to`.
fn relocate(from: &Path, to: &Path) -> bool {
    if !from.exists() {
        return false;
    }
    match move_file(from, to) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("bloom: couldn't move {} to {}: {e}", from.display(), to.display());
            false
        }
    }
}

/// `stem`, or "stem (2)" and so on, whichever has no video or part file in `folder` yet.
fn free_stem(folder: &Path, stem: &str, ext: &str) -> String {
    (1..)
        .map(|n| if n == 1 { stem.to_string() } else { format!("{stem} ({n})") })
        .find(|candidate| !folder.join(format!("{candidate}.{ext}")).exists() && !folder.join(format!(".{candidate}.part")).exists())
        .expect("a numbered name is always free")
}

/// Downloads from before files had readable names kept everything in
/// "{location}/{serverId}/{Title} - {itemId}/": media.{ext}, sub-{index}.{ext}, art-* and
/// item.json. Moves them into the layout above.
fn migrate(entry: &mut Entry, meta_root: &Path) {
    let old = entry.folder.clone();
    let owned = old.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(&format!(" - {}", entry.facts.item_id)));
    entry.root = old.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    entry.meta = meta_root.join(&entry.id);
    (entry.folder, entry.stem) = layout(&entry.root, &entry.facts);
    if !owned || !safe_token(&entry.id) {
        return;
    }
    // Two old downloads can come out with one name (the same film from two servers): the second
    // gets a number rather than replacing the first one's files.
    let ext = entry.media_file.as_deref().and_then(|name| name.rsplit_once('.')).map_or("mkv", |(_, ext)| ext).to_string();
    entry.stem = free_stem(&entry.folder, &entry.stem, &ext);
    if let Some(name) = entry.media_file.clone() {
        let renamed = format!("{}.{ext}", entry.stem);
        if relocate(&old.join(&name), &entry.folder.join(&renamed)) {
            entry.media_file = Some(renamed);
        }
    }
    relocate(&old.join("media.part"), &entry.part_path());
    let mut subtitles = std::mem::take(&mut entry.subtitles);
    for subtitle in &mut subtitles {
        let format = subtitle.file.rsplit_once('.').map_or("srt", |(_, ext)| ext).to_string();
        let renamed = entry.subtitle_name(subtitle.index, &subtitle.lang, &format);
        if relocate(&old.join(&subtitle.file), &entry.folder.join(&renamed)) {
            subtitle.file = renamed;
        }
    }
    entry.subtitles = subtitles;
    if let Ok(files) = fs::read_dir(&old) {
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if name == "item.json" || name.starts_with("art-") {
                relocate(&file.path(), &entry.meta.join(&name));
            }
        }
    }
    let _ = fs::remove_dir(&old);
    if let Some(server_folder) = old.parent() {
        let _ = fs::remove_dir(server_folder);
    }
}

/// The file's bitrate: the server's figure, or its size over its length.
fn source_bitrate(facts: &DownloadFacts) -> Option<u64> {
    facts
        .source
        .as_ref()
        .and_then(|s| s.bitrate)
        .and_then(|b| u64::try_from(b).ok())
        .filter(|b| *b > 0)
        .or_else(|| Some((facts.size? as f64 * 8.0 / facts.runtime_seconds.filter(|s| *s > 0.0)?) as u64))
}

/// Original always; a conversion only when the file is heavier than it would be.
fn offered(quality: Quality, facts: &DownloadFacts) -> bool {
    let (video, _, _) = conversion(quality);
    quality == Quality::Original || source_bitrate(facts).is_none_or(|bitrate| bitrate > video + AUDIO_BITRATE)
}

fn offered_qualities(facts: &DownloadFacts) -> Vec<Quality> {
    [Quality::Original, Quality::Hd1080, Quality::Hd720].into_iter().filter(|q| offered(*q, facts)).collect()
}

fn effective_quality(quality: Quality, facts: &DownloadFacts) -> Quality {
    if offered(quality, facts) {
        quality
    } else {
        Quality::Original
    }
}

/// The video's extension from the server's container name, which can be a list ("mov,mp4,m4a").
fn extension(container: Option<&str>) -> String {
    container
        .and_then(|c| c.split(',').next())
        .map(str::trim)
        .filter(|c| !c.is_empty() && c.len() <= 5 && c.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "mkv".into())
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => PathBuf::from(home).join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

/// Free and total bytes on the disk holding `path` (or the nearest folder above it that exists).
#[cfg(target_os = "linux")]
fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let existing = path.ancestors().find(|p| p.exists())?;
    let c_path = std::ffi::CString::new(existing.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a NUL-terminated path, and a zeroed struct for statvfs to fill in.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block = stats.f_frsize as u64;
    Some((stats.f_bavail as u64 * block, stats.f_blocks as u64 * block))
}

#[cfg(not(target_os = "linux"))]
fn disk_space(_: &Path) -> Option<(u64, u64)> {
    None
}

/// Whether a server address is on the local network: a private or link-local address, a name
/// only a local network would use, or a name that resolves only to local addresses.
async fn is_local_address(address: &str) -> bool {
    let Ok(url) = url::Url::parse(address) else { return false };
    let port = url.port_or_known_default().unwrap_or(80);
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_local_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => is_local_ip(ip.into()),
        Some(url::Host::Domain(name)) => {
            let name = name.to_ascii_lowercase();
            if !name.contains('.') || [".local", ".lan", ".home.arpa", ".internal"].iter().any(|s| name.ends_with(s)) {
                return true;
            }
            let found = tauri::async_runtime::spawn_blocking(move || {
                use std::net::ToSocketAddrs;
                (name.as_str(), port).to_socket_addrs().map(|addrs| addrs.map(|a| a.ip()).collect::<Vec<_>>()).unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            !found.is_empty() && found.into_iter().all(is_local_ip)
        }
        None => false,
    }
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(|v4| is_local_ip(v4.into()))
        }
    }
}

// Dates as Jellyfin writes them ("2026-09-14T10:04:05.1234567Z"), without a date library.
// Howard Hinnant's days-from-civil algorithms.

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn format_date(secs: u64) -> String {
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn parse_date(text: &str) -> Option<u64> {
    let (date, time) = text.split_once('T')?;
    let mut d = date.split('-');
    let (year, month, day): (i64, u32, u32) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let time = time.trim_end_matches('Z').split(['.', '+']).next()?;
    let mut t = time.split(':');
    let (hour, minute, second): (i64, i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    u64::try_from(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second).ok()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    location: String,
    /// Everything Bloom has downloaded, for every account.
    bloom_bytes: u64,
    free_bytes: Option<u64>,
    total_bytes: Option<u64>,
}

#[tauri::command]
pub fn downloads(jf: State<'_, Jellyfin>, dl: State<'_, Downloads>) -> Vec<DownloadView> {
    dl.views(&jf)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDownloads {
    count: usize,
    bytes: u64,
}

/// For the Servers screen's remove: what deleting a server's downloads would free.
#[tauri::command]
pub fn server_downloads(dl: State<'_, Downloads>, server_id: String) -> ServerDownloads {
    dl.server_summary(&server_id)
}

/// Resolves to how many downloads were deleted.
#[tauri::command]
pub fn remove_server_downloads(dl: State<'_, Downloads>, server_id: String) -> usize {
    dl.remove_server(&server_id)
}

/// The films and shows with something downloaded, as poster cards.
#[tauri::command]
pub fn downloaded_titles(jf: State<'_, Jellyfin>, dl: State<'_, Downloads>) -> Vec<Card> {
    dl.titles(&jf)
}

#[tauri::command]
pub async fn download_item(
    jf: State<'_, Jellyfin>,
    dl: State<'_, Downloads>,
    store: State<'_, SettingsStore>,
    item_id: String,
    quality: Option<Quality>,
) -> Result<(), Error> {
    dl.queue(&jf, &store, &item_id, quality).await
}

#[tauri::command]
pub async fn download_season(
    jf: State<'_, Jellyfin>,
    dl: State<'_, Downloads>,
    store: State<'_, SettingsStore>,
    series_id: String,
    season_id: String,
    quality: Option<Quality>,
) -> Result<u32, Error> {
    dl.queue_season(&jf, &store, &series_id, &season_id, quality).await
}

#[tauri::command]
pub fn download_pause(dl: State<'_, Downloads>, id: String) {
    dl.pause(&id);
}

#[tauri::command]
pub fn download_resume(dl: State<'_, Downloads>, id: String) {
    dl.resume(&id);
}

#[tauri::command]
pub fn download_remove(dl: State<'_, Downloads>, id: String) {
    dl.remove(&id);
}

#[tauri::command]
pub fn download_set_quality(dl: State<'_, Downloads>, id: String, quality: Quality) {
    dl.set_quality(&id, quality);
}

#[tauri::command]
pub fn download_storage(dl: State<'_, Downloads>, store: State<'_, SettingsStore>) -> Storage {
    dl.storage(&store)
}

/// Makes sure a folder can hold downloads, creating it if needed, and resolves to its full path.
/// Empty is the default folder.
#[tauri::command]
pub fn check_download_location(dl: State<'_, Downloads>, path: String) -> Result<String, Error> {
    let typed = path.trim();
    let folder = if typed.is_empty() { dl.default_location.clone() } else { expand_home(typed) };
    if !folder.is_absolute() {
        return Err(Error::DownloadFolder("give the whole path, starting with / or ~".into()));
    }
    fs::create_dir_all(&folder).map_err(|e| Error::DownloadFolder(e.to_string()))?;
    let probe = folder.join(format!(".bloom-{}", uuid::Uuid::new_v4().simple()));
    fs::write(&probe, b"").map_err(|e| Error::DownloadFolder(e.to_string()))?;
    let _ = fs::remove_file(&probe);
    Ok(folder.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn sync_downloads(jf: State<'_, Jellyfin>, dl: State<'_, Downloads>) -> Result<u32, Error> {
    sync_with(&jf, &dl).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(kind: &str, title: &str) -> DownloadFacts {
        DownloadFacts { item_id: "abc123".into(), kind: kind.into(), title: title.into(), ..DownloadFacts::default() }
    }

    #[test]
    fn dates_round_trip_in_jellyfins_format() {
        assert_eq!(format_date(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(parse_date("2026-09-14T10:04:05.1234567Z"), Some(1_789_380_245));
        assert_eq!(format_date(1_789_380_245), "2026-09-14T10:04:05.000Z");
        // A leap day, and the turn of a century.
        for secs in [951_782_400, 4_107_542_400, 1_709_208_000] {
            assert_eq!(parse_date(&format_date(secs)), Some(secs));
        }
        assert_eq!(parse_date("yesterday"), None);
    }

    #[test]
    fn watching_leaves_jellyfins_resume_points() {
        assert_eq!(resume_after(600.0, Some(1440.0), false), (600.0, false));
        assert_eq!(resume_after(1400.0, Some(1440.0), false), (0.0, true), "past 90% is watched");
        assert_eq!(resume_after(30.0, Some(1440.0), false), (0.0, false), "under 5% isn't worth resuming");
        assert_eq!(resume_after(10.0, Some(1440.0), true), (0.0, true));
        assert_eq!(resume_after(42.0, None, false), (42.0, false));
    }

    fn episode(title: &str, season: i32, number: i32, name: &str) -> DownloadFacts {
        DownloadFacts {
            season_number: Some(season),
            episode_number: Some(number),
            name: name.into(),
            ..facts("Episode", title)
        }
    }

    #[test]
    fn files_are_laid_out_as_media_servers_expect() {
        let root = Path::new("/home/me/Videos/Bloom");
        let (folder, stem) = layout(root, &episode("Tokyo Ghoul", 1, 1, "Tragedy"));
        assert_eq!(folder, root.join("Tokyo Ghoul/Season 1"));
        assert_eq!(stem, "Tokyo Ghoul - S01E01 - Tragedy");
        assert_eq!(layout(root, &episode("Frieren: Beyond Journey's End", 0, 2, "")).0, root.join("Frieren - Beyond Journey's End/Specials"));
        let film = DownloadFacts { year: Some(2015), ..facts("Movie", "Sicario") };
        assert_eq!(layout(root, &film), (root.join("Sicario (2015)"), "Sicario (2015)".to_string()));
        // An older download, whose facts only had the subtitle.
        let older = DownloadFacts { subtitle: Some("S1 E1, Tragedy".into()), ..episode("Tokyo Ghoul", 1, 1, "") };
        assert_eq!(layout(root, &older).1, "Tokyo Ghoul - S01E01 - Tragedy");
        for hostile in ["../../etc/passwd", "..", ".hidden", "a/b\\c"] {
            let name = clean_name(hostile);
            assert!(!name.contains('/') && !name.starts_with('.'), "{hostile} became {name}");
        }
        assert_eq!(layout(root, &facts("Movie", "///")).1, "Untitled");
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let root = Path::new("/nonexistent-bloom-root");
        let facts = episode("Show", 1, 1, "Pilot");
        let (folder, stem) = layout(root, &facts);
        let mut first = entry(folder.to_str().unwrap());
        first.stem = stem;
        assert_eq!(unique_layout(&[first], root, &facts).1, "Show - S01E01 - Pilot (2)");
    }

    #[test]
    fn conversions_are_only_offered_when_lighter() {
        let light = DownloadFacts { size: Some(309_000_000), runtime_seconds: Some(1440.0), ..facts("Episode", "Show") };
        assert_eq!(offered_qualities(&light), [Quality::Original], "a 1.7 Mb/s file was offered heavier conversions");
        assert_eq!(effective_quality(Quality::Hd720, &light), Quality::Original);
        let heavy = DownloadFacts {
            source: Some(playback::SourceVideo { bitrate: Some(20_000_000), ..Default::default() }),
            ..facts("Movie", "Film")
        };
        assert_eq!(offered_qualities(&heavy), [Quality::Original, Quality::Hd1080, Quality::Hd720]);
        let middling = DownloadFacts {
            source: Some(playback::SourceVideo { bitrate: Some(6_000_000), ..Default::default() }),
            ..facts("Movie", "Film")
        };
        assert_eq!(offered_qualities(&middling), [Quality::Original, Quality::Hd720]);
        assert_eq!(offered_qualities(&facts("Movie", "Unknown")).len(), 3);
    }

    #[test]
    fn old_downloads_move_into_the_new_layout_and_delete_cleanly() {
        let base = std::env::temp_dir().join(format!("bloom-dl-test-{}", uuid::Uuid::new_v4().simple()));
        let root = base.join("Bloom");
        let meta_root = base.join("data/downloads");
        let old = root.join("srv1").join("Tokyo Ghoul S01E01 - abc123");
        fs::create_dir_all(&old).unwrap();
        for (name, bytes) in [("media.mkv", "video"), ("sub-2.ass", "subs"), ("art-abc123-Primary-t1", "art"), ("item.json", "{}")] {
            fs::write(old.join(name), bytes).unwrap();
        }
        let mut download = entry(old.to_str().unwrap());
        download.facts = DownloadFacts { item_id: "abc123".into(), subtitle: Some("S1 E1, Tragedy".into()), ..episode("Tokyo Ghoul", 1, 1, "") };
        download.media_file = Some("media.mkv".into());
        download.subtitles = vec![LocalSubtitle { index: 2, file: "sub-2.ass".into(), title: String::new(), lang: "eng".into() }];

        migrate(&mut download, &meta_root);
        let season = root.join("Tokyo Ghoul/Season 1");
        let moved = |path: PathBuf| fs::read_to_string(path).unwrap_or_default();
        assert_eq!(moved(season.join("Tokyo Ghoul - S01E01 - Tragedy.mkv")), "video");
        assert_eq!(moved(season.join("Tokyo Ghoul - S01E01 - Tragedy.eng.2.ass")), "subs");
        assert_eq!(moved(meta_root.join("d1/item.json")), "{}");
        assert_eq!(moved(meta_root.join("d1/art-abc123-Primary-t1")), "art");
        assert!(!root.join("srv1").exists(), "the old folders were left behind");

        // Something else in the show's folder keeps it; the season folder Bloom emptied goes.
        fs::write(root.join("Tokyo Ghoul/notes.txt"), "mine").unwrap();
        delete_files(&download);
        assert!(!season.exists() && !meta_root.join("d1").exists());
        assert!(root.join("Tokyo Ghoul/notes.txt").exists() && root.exists());
        let _ = fs::remove_dir_all(&base);
    }

    fn entry(folder: &str) -> Entry {
        Entry {
            id: "d1".into(),
            server_id: "srv1".into(),
            user_id: "u1".into(),
            quality: Quality::Original,
            status: Status::Done,
            error: None,
            facts: facts("Movie", "Film"),
            folder: PathBuf::from(folder),
            stem: String::new(),
            root: PathBuf::from("/home/me/Videos/Bloom"),
            meta: PathBuf::new(),
            poster: None,
            media_file: None,
            subtitles: Vec::new(),
            bytes_done: 0,
            bytes_total: None,
            estimated: false,
            attempts: 0,
            retry_at: None,
            added_at: 0,
            finished_at: None,
            position_seconds: 0.0,
            played: false,
            last_played_at: None,
            sync_pending: false,
        }
    }

    fn downloaded_episode(id: &str, season: i32, number: i32, played: bool, position: f64) -> Entry {
        let mut e = entry("/x");
        e.id = format!("d-{id}");
        e.facts = DownloadFacts {
            item_id: id.into(),
            series_id: Some("show1".into()),
            season_id: Some(format!("season{season}")),
            runtime_seconds: Some(1440.0),
            ..episode("Tokyo Ghoul", season, number, &format!("Episode {number}"))
        };
        e.played = played;
        e.position_seconds = position;
        e.finished_at = Some(number as u64);
        e
    }

    #[test]
    fn a_show_is_one_card_however_many_episodes() {
        let episodes = [downloaded_episode("e1", 1, 1, true, 0.0), downloaded_episode("e2", 1, 2, false, 0.0)];
        let mut film = entry("/x");
        film.facts = DownloadFacts { item_id: "f1".into(), year: Some(2015), ..facts("Movie", "Sicario") };
        film.finished_at = Some(9);
        let cards = serde_json::to_value(title_cards(vec![&episodes[0], &episodes[1], &film])).unwrap();
        assert_eq!(cards.as_array().unwrap().len(), 2);
        assert_eq!((cards[0]["id"].as_str(), cards[0]["meta"].as_str()), (Some("f1"), Some("2015")));
        assert_eq!(cards[1]["kind"], "Series");
        assert_eq!(cards[1]["id"], "show1");
        assert_eq!(cards[1]["meta"], "2 episodes downloaded");
    }

    #[test]
    fn an_offline_show_page_lists_only_whats_downloaded() {
        let saved = json!({
            "id": "show1", "title": "Tokyo Ghoul", "genres": ["Action"],
            "seasons": [{ "id": "season1", "name": "Season 1", "episodeCount": 12 }, { "id": "season2", "name": "√A", "episodeCount": 12 }],
        });
        let episodes = [
            downloaded_episode("e1", 1, 1, true, 0.0),
            downloaded_episode("e2", 1, 2, false, 600.0),
            downloaded_episode("e3", 2, 1, false, 0.0),
        ];
        let detail = narrowed_series(Some(saved), "show1", &episodes);
        assert_eq!(detail["genres"][0], "Action", "the saved page's details were lost");
        assert_eq!(detail["seasons"][0]["episodeCount"], 2);
        assert_eq!(detail["seasons"][0]["unplayed"], 1);
        assert_eq!(detail["seasons"][1]["name"], "√A");
        assert_eq!(detail["play"]["itemId"], "e2");
        assert_eq!(detail["play"]["label"], "Resume S1 E2");
        assert_eq!(detail["episodeCount"], 3);

        let page = episodes_page(&episodes, "season1");
        assert_eq!(page["total"], 2);
        assert_eq!(page["episodes"][1]["title"], "Episode 2");
        assert!((page["episodes"][1]["progress"].as_f64().unwrap() - 600.0 / 1440.0).abs() < 1e-9);

        let plain = narrowed_series(None, "show1", &episodes[..1]);
        assert!(plain["people"].is_array() && plain["genres"].is_array(), "a plain page is missing fields the page reads");
        assert_eq!(plain["play"]["label"], "Play S1 E1");
    }

    #[test]
    fn long_names_fit_what_linux_allows() {
        let japanese = "あ".repeat(90);
        let name = clean_name(&japanese);
        assert!(name.len() <= NAME_BYTES, "{} bytes", name.len());
        assert_eq!(name, "あ".repeat(NAME_BYTES / 3), "cut on a character boundary");
        let short = "Frieren - Beyond Journey's End";
        assert_eq!(clean_name(short), short);
    }

    #[test]
    fn a_downloads_own_subtitles_are_found_by_name() {
        assert!(is_subtitle_suffix("eng.3.srt"));
        assert!(is_subtitle_suffix("3.ass"));
        assert!(!is_subtitle_suffix("mkv"));
        assert!(!is_subtitle_suffix("eng.srt"), "no stream number");
        assert!(!is_subtitle_suffix("B - S01E01.eng.3.srt"), "another download whose name only starts the same");
        assert!(!is_subtitle_suffix("eng.3.txt"));
    }

    #[test]
    fn a_resumed_range_must_carry_on_the_same_file() {
        assert!(range_continues(Some("bytes 500-999/1000"), 500, Some(1000)));
        assert!(range_continues(Some("bytes 500-999/1000"), 500, None));
        assert!(!range_continues(Some("bytes 0-999/1000"), 500, Some(1000)), "started over from the top");
        assert!(!range_continues(Some("bytes 500-1199/1200"), 500, Some(1000)), "a different file now");
        assert!(!range_continues(None, 500, Some(1000)));
    }

    #[test]
    fn containers_become_extensions() {
        assert_eq!(extension(Some("mkv")), "mkv");
        assert_eq!(extension(Some("mov,mp4,m4a,3gp,3g2,mj2")), "mov");
        assert_eq!(extension(Some("../x")), "mkv");
        assert_eq!(extension(None), "mkv");
    }

    #[test]
    fn local_addresses() {
        for ip in ["192.168.1.20", "10.0.0.5", "172.20.1.1", "127.0.0.1", "169.254.1.1", "fd12::1", "fe80::1", "::1"] {
            assert!(is_local_ip(ip.parse().unwrap()), "{ip} counted as remote");
        }
        for ip in ["8.8.8.8", "100.64.0.1", "2001:4860::8888"] {
            assert!(!is_local_ip(ip.parse().unwrap()), "{ip} counted as local");
        }
    }

    #[test]
    fn a_shows_saved_page_is_kept_per_server() {
        let root = Path::new("/data/downloads");
        assert_eq!(series_folder(root, "srv1", "show1"), Some(root.join("series-srv1-show1")));
        assert_ne!(series_folder(root, "srv1", "show1"), series_folder(root, "srv2", "show1"));
        assert!(series_folder(root, "../x", "show1").is_none() && series_folder(root, "srv1", "").is_none());
    }
}
