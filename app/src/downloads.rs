//! Downloads: the screen, and the state the download buttons on title pages follow.
//!
//! The core keeps the queue and does the downloading, announcing the whole list as a
//! `downloads:changed` event whenever anything moves. This keeps the latest list, turns it into the
//! screen's two lists (in progress, ready to watch) in place so a progress tick doesn't rebuild the
//! rows and reload their pictures, and tells the title page what each of its items' buttons says.

use crate::cards;
use crate::text;
use crate::nav::{self, Route};
use crate::title::Img;
use crate::{watch, App, DownloadData, NavTarget, Screen, StorageData};
use bloom_core::downloads::{self, Downloads};
use bloom_core::jellyfin::Jellyfin;
use bloom_core::playback::Quality;
use bloom_core::settings::SettingsStore;
use bloom_core::Ctx;
use serde::Deserialize;
use serde_json::Value;
use slint::{Image, Model, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

/// One download, as the core announces it (the parts this reads).
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct View {
    pub id: String,
    pub item_id: String,
    pub kind: String,
    pub series_id: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub title: String,
    pub subtitle: Option<String>,
    pub image: Option<Img>,
    pub quality: String,
    pub qualities: Vec<String>,
    /// "queued", "downloading", "paused", "failed" or "done".
    pub status: String,
    pub error: Option<String>,
    /// Why a queued download isn't moving: "network" or "server".
    pub waiting: Option<String>,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
    pub estimated: bool,
    pub bytes_per_second: Option<f64>,
    pub runtime_seconds: Option<f64>,
    pub position_seconds: f64,
    pub played: bool,
    pub added_at: u64,
    pub finished_at: Option<u64>,
}

impl View {
    /// 0..1.
    pub fn fraction(&self) -> f32 {
        match self.bytes_total {
            Some(total) if total > 0 => (self.bytes_done as f64 / total as f64).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        }
    }
}

/// The latest list the core announced.
static VIEWS: LazyLock<Mutex<Vec<View>>> = LazyLock::new(Mutex::default);

fn views() -> std::sync::MutexGuard<'static, Vec<View>> {
    VIEWS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The download of `item_id`, if there is one: its status and how far along it is.
pub fn of_item(item_id: &str) -> Option<(String, f32)> {
    views().iter().find(|v| v.item_id == item_id).map(|v| (v.status.clone(), v.fraction()))
}

/// Whether something finished downloading is on this computer for `id`: the item itself, or (for a
/// show) any of its episodes.
pub fn has_downloaded(id: &str) -> bool {
    views().iter().any(|v| v.status == "done" && (v.item_id == id || v.series_id.as_deref() == Some(id)))
}

/// A show's finished downloads, in the show's order.
pub fn episodes_of(series_id: &str) -> Vec<View> {
    let mut found: Vec<View> = views().iter().filter(|v| v.status == "done" && v.series_id.as_deref() == Some(series_id)).cloned().collect();
    found.sort_by_key(|v| (v.season_number.unwrap_or(0), v.episode_number.unwrap_or(0)));
    found
}

/// The films and shows with a finished download: an episode counts for its show.
pub fn downloaded_title_ids() -> Vec<String> {
    let mut ids: Vec<String> = views()
        .iter()
        .filter(|v| v.status == "done")
        .filter_map(|v| if v.kind == "Episode" { v.series_id.clone() } else { Some(v.item_id.clone()) })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// What a download button says.
pub fn button_label(download: Option<&(String, f32)>) -> &'static str {
    match download.map(|d| d.0.as_str()) {
        None => "Download",
        Some("done") => "Downloaded",
        Some("downloading") => "Downloading",
        Some("queued") => "Queued",
        Some("paused") => "Download paused",
        Some("failed") => "Download failed",
        Some(_) => "Download",
    }
}

/// "2.9 GB", "340 MB", "12 KB".
pub fn bytes_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// "1.2 GB of 2.9 GB", with "about" before a total that is only an estimate.
fn progress_text(v: &View) -> String {
    match v.bytes_total {
        Some(total) => format!("{} of {}{}", bytes_label(v.bytes_done), if v.estimated { "about " } else { "" }, bytes_label(total)),
        None => bytes_label(v.bytes_done),
    }
}

/// "1h 5m", "40m": at least a minute.
fn duration(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round().max(1.0) as u64;
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// The parts of the line under a download's title, as the original lists them.
fn meta_of(v: &View, server: &str) -> Vec<String> {
    match v.status.as_str() {
        "downloading" => {
            let mut parts = vec![progress_text(v)];
            match v.bytes_per_second.filter(|s| *s > 0.0) {
                None => parts.push("Starting".into()),
                Some(speed) => {
                    parts.push(format!("{}/s", bytes_label(speed as u64)));
                    if let (Some(total), false) = (v.bytes_total, v.estimated) {
                        let left = total.saturating_sub(v.bytes_done) as f64 / speed;
                        parts.push(if left < 60.0 { "Under a minute left".into() } else { format!("{} left", duration(left)) });
                    }
                }
            }
            parts
        }
        "queued" => {
            let state = match v.waiting.as_deref() {
                Some("network") => "Waits for the local network".to_string(),
                Some("server") => format!("Waiting for {server}"),
                _ => "Queued".to_string(),
            };
            match v.bytes_total {
                Some(total) => vec![state, format!("{}{}", if v.estimated { "about " } else { "" }, bytes_label(total))],
                None => vec![state],
            }
        }
        "paused" => vec!["Paused".into(), if v.quality == "original" { progress_text(v) } else { "Starts again when resumed".into() }],
        "failed" => vec![if v.bytes_done > 0 { progress_text(v) } else { "Not started".into() }],
        _ => {
            let mut parts = vec![bytes_label(v.bytes_done)];
            if v.quality != "original" {
                parts.push(v.quality.clone());
            }
            match v.runtime_seconds {
                _ if v.played => parts.push("Watched".into()),
                Some(runtime) if v.position_seconds > 0.0 => parts.push(format!("{} left", duration(runtime - v.position_seconds))),
                Some(runtime) => parts.push(duration(runtime)),
                None => {}
            }
            parts
        }
    }
}

/// The line under a download's title.
fn detail(v: &View, server: &str) -> String {
    meta_of(v, server).join("  \u{2022}  ")
}

/// Downloading first, then queued, paused and failed, each in the order added.
fn in_progress_order(list: &mut [&View]) {
    let rank = |s: &str| match s {
        "downloading" => 0,
        "queued" => 1,
        "paused" => 2,
        _ => 3,
    };
    list.sort_by_key(|v| (rank(&v.status), v.added_at));
}

fn quality_label(q: &str) -> String {
    match q {
        "original" => "Original".into(),
        other => other.to_string(),
    }
}

fn quality_from(label: &str) -> Quality {
    match label {
        "1080p" => Quality::Hd1080,
        "720p" => Quality::Hd720,
        _ => Quality::Original,
    }
}

/// A row, as sendable data (a `DownloadData` holds a picture, which can't cross threads).
struct Row {
    id: String,
    item_id: String,
    title: String,
    subtitle: String,
    status: String,
    progress: f32,
    detail: String,
    error: String,
    qualities: Vec<String>,
    quality_index: i32,
    watched: f32,
    size: String,
    image: Option<String>,
    header: bool,
    expanded: bool,
    indent: bool,
    group: String,
}

impl Row {
    fn from(v: &View, server: &str) -> Self {
        let watched = match (v.played, v.runtime_seconds) {
            (false, Some(runtime)) if runtime > 0.0 => (v.position_seconds / runtime).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        Self {
            id: v.id.clone(),
            item_id: v.item_id.clone(),
            title: v.title.clone(),
            subtitle: v.subtitle.clone().unwrap_or_default(),
            status: v.status.clone(),
            progress: v.fraction(),
            detail: detail(v, server),
            error: v.error.clone().unwrap_or_default(),
            qualities: v.qualities.iter().map(|q| quality_label(q)).collect(),
            quality_index: v.qualities.iter().position(|q| *q == v.quality).unwrap_or(0) as i32,
            watched,
            size: bytes_label(v.bytes_done),
            image: v.image.as_ref().map(|i| i.path(480)),
            header: false,
            expanded: false,
            indent: false,
            group: String::new(),
        }
    }

    /// The heading over a show's downloaded episodes.
    fn heading(members: &[&View], expanded: bool) -> Self {
        let first = members[0];
        let group = first.series_id.clone().unwrap_or_default();
        let bytes: u64 = members.iter().map(|m| m.bytes_done).sum();
        let watched = members.iter().filter(|m| m.played).count();
        let mut detail = vec![text::count(members.len() as u64, "episode"), bytes_label(bytes)];
        if watched == members.len() {
            detail.push("Watched".into());
        } else if watched > 0 {
            detail.push(format!("{watched} watched"));
        }
        Self {
            id: format!("group:{group}"),
            item_id: String::new(),
            title: first.title.clone(),
            subtitle: String::new(),
            status: "done".into(),
            progress: 0.0,
            detail: detail.join("  \u{2022}  "),
            error: String::new(),
            qualities: Vec::new(),
            quality_index: 0,
            watched: 0.0,
            size: bytes_label(bytes),
            image: first.image.as_ref().map(|i| i.path(480)),
            header: true,
            expanded,
            indent: false,
            group,
        }
    }

    fn data(&self, image: Option<(Image, bool)>) -> DownloadData {
        let (image, has_image) = image.unwrap_or_default();
        DownloadData {
            id: self.id.clone().into(),
            item_id: self.item_id.clone().into(),
            title: self.title.clone().into(),
            subtitle: self.subtitle.clone().into(),
            status: self.status.clone().into(),
            progress: self.progress,
            detail: self.detail.clone().into(),
            error: self.error.clone().into(),
            image,
            has_image,
            qualities: ModelRc::new(VecModel::from(self.qualities.iter().cloned().map(SharedString::from).collect::<Vec<_>>())),
            quality_index: self.quality_index,
            watched: self.watched,
            size: self.size.clone().into(),
            header: self.header,
            expanded: self.expanded,
            indent: self.indent,
            group: self.group.clone().into(),
        }
    }
}

struct Models {
    active: Rc<VecModel<DownloadData>>,
    ready: Rc<VecModel<DownloadData>>,
    active_ids: Vec<String>,
    ready_ids: Vec<String>,
}

thread_local! {
    /// The shows whose downloaded episodes are listed under their heading. UI thread only.
    static EXPANDED: RefCell<std::collections::HashSet<String>> = RefCell::new(std::collections::HashSet::new());
    /// The two lists on the Downloads screen. UI thread only.
    static MODELS: RefCell<Option<Models>> = const { RefCell::new(None) };
}

/// The ready downloads as the screen lists them: a show with two or more finished episodes is one
/// heading (at the place of its newest) with the episodes under it when it is open; the rest as they are.
fn grouped(ready: &[&View], server: &str, open: &std::collections::HashSet<String>) -> Vec<Row> {
    let in_show = |v: &View| v.kind == "Episode" && v.series_id.is_some();
    let members_of = |series: &str| -> Vec<&View> {
        let mut m: Vec<&View> = ready.iter().copied().filter(|v| in_show(v) && v.series_id.as_deref() == Some(series)).collect();
        m.sort_by_key(|v| (v.season_number.unwrap_or(0), v.episode_number.unwrap_or(0)));
        m
    };
    let mut rows = Vec::new();
    let mut done: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for v in ready {
        let Some(series) = v.series_id.as_deref().filter(|_| in_show(v)) else {
            rows.push(Row::from(v, server));
            continue;
        };
        let members = members_of(series);
        if members.len() < 2 {
            rows.push(Row::from(v, server));
        } else if done.insert(series) {
            let expanded = open.contains(series);
            rows.push(Row::heading(&members, expanded));
            if expanded {
                rows.extend(members.iter().map(|m| Row { indent: true, ..Row::from(m, server) }));
            }
        }
    }
    rows
}

/// A show's heading was pressed: its episodes open or close.
pub fn toggle_group(ctx: Ctx, weak: Weak<App>, group: String) {
    let _ = weak.clone().upgrade_in_event_loop(move |app| {
        EXPANDED.with(|e| {
            let mut e = e.borrow_mut();
            if !e.remove(&group) {
                e.insert(group);
            }
        });
        let list = views().clone();
        apply(&ctx, &weak, &app, &list);
    });
}

/// Brings the screen's lists in step with `views`. In place when the same downloads are in the
/// same groups, so a progress tick changes only the numbers; otherwise new lists, with pictures
/// asked for again (from the core's cache). UI thread.
fn apply(ctx: &Ctx, weak: &Weak<App>, app: &App, list: &[View]) {
    let (mut ready, mut active): (Vec<&View>, Vec<&View>) = list.iter().partition(|v| v.status == "done");
    in_progress_order(&mut active);
    ready.sort_by_key(|v| std::cmp::Reverse(v.finished_at.unwrap_or(0)));
    let server = app.get_server_name();
    let active_rows: Vec<Row> = active.iter().map(|v| Row::from(v, &server)).collect();
    let ready_rows = grouped(&ready, &server, &EXPANDED.with(|e| e.borrow().clone()));
    app.set_downloads_ready_count(ready.len() as i32);
    let ids = |rows: &[Row]| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();

    let same = MODELS.with(|m| m.borrow().as_ref().is_some_and(|m| m.active_ids == ids(&active_rows) && m.ready_ids == ids(&ready_rows)));
    if same {
        MODELS.with(|m| {
            let m = m.borrow();
            let m = m.as_ref().expect("checked above");
            for (model, rows) in [(&m.active, &active_rows), (&m.ready, &ready_rows)] {
                for (i, row) in rows.iter().enumerate() {
                    if let Some(old) = model.row_data(i) {
                        // The picture stays; everything else is as the core says now.
                        model.set_row_data(i, row.data(Some((old.image, old.has_image))));
                    }
                }
            }
        });
        return;
    }

    let build = |rows: &[Row]| {
        let model = Rc::new(VecModel::from(rows.iter().map(|r| r.data(None)).collect::<Vec<_>>()));
        let tracker = cards::track(&model, |row: &mut DownloadData, image| {
            row.image = image;
            row.has_image = true;
        });
        let jobs = rows.iter().enumerate().filter_map(|(i, r)| r.image.clone().map(|p| (i, p))).collect::<Vec<_>>();
        (model, tracker, jobs)
    };
    let (active_model, active_tracker, active_jobs) = build(&active_rows);
    let (ready_model, ready_tracker, ready_jobs) = build(&ready_rows);
    app.set_downloads_active(ModelRc::from(active_model.clone()));
    app.set_downloads_ready(ModelRc::from(ready_model.clone()));
    MODELS.with(|m| *m.borrow_mut() = Some(Models { active: active_model, ready: ready_model, active_ids: ids(&active_rows), ready_ids: ids(&ready_rows) }));
    cards::fetch_rows(ctx, weak, active_tracker, active_jobs);
    cards::fetch_rows(ctx, weak, ready_tracker, ready_jobs);
}

/// Whether anything is moving and whether anything has finished, for the rail's dot and the offline
/// banner's "Browse downloads". UI thread.
fn set_flags(app: &App, list: &[View]) {
    app.set_downloading(list.iter().any(|v| v.status == "downloading"));
    app.set_has_downloads(list.iter().any(|v| v.status == "done"));
}

/// The list as it stands at sign-in, so the flags are right before any event arrives.
pub fn load_initial(ctx: Ctx, weak: Weak<App>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let list: Vec<View> = serde_json::to_value(downloads::downloads(&jf, &dl)).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        *views() = list.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            set_flags(&app, &list);
            crate::cards::refresh_downloaded();
        });
    });
}

/// The core announced the downloads again.
pub fn on_changed(ctx: Option<Ctx>, weak: &Weak<App>, payload: Value) {
    let Some(ctx) = ctx else { return };
    let list: Vec<View> = serde_json::from_value(payload).unwrap_or_default();
    *views() = list.clone();
    let weak2 = weak.clone();
    let _ = weak.upgrade_in_event_loop(move |app| {
        if app.get_screen() == Screen::Downloads {
            apply(&ctx, &weak2, &app, &list);
            app.set_downloads_storage(storage_data(&ctx));
        }
        set_flags(&app, &list);
        crate::playpage::refresh_downloads(&app);
        crate::title::refresh_downloads(&app);
        crate::cards::refresh_downloaded();
    });
}

/// "Bloom has 12 GB in here  •  80 GB free of 462 GB".
pub fn storage_text(ctx: &Ctx) -> String {
    let dl = ctx.state::<Downloads>();
    let store = ctx.state::<SettingsStore>();
    let s = downloads::download_storage(&dl, &store);
    let mut parts = vec![format!("Bloom has {} in {}", bytes_label(s.bloom_bytes), s.location)];
    match (s.free_bytes, s.total_bytes) {
        (Some(free), Some(total)) => parts.push(format!("{} free of {}", bytes_label(free), bytes_label(total))),
        (Some(free), None) => parts.push(format!("{} free", bytes_label(free))),
        _ => {}
    }
    parts.join("  \u{2022}  ")
}

/// The disk as the bar and legend show it: Bloom's own share, everything else on the disk, and
/// what is free.
pub fn storage_data(ctx: &Ctx) -> StorageData {
    let dl = ctx.state::<Downloads>();
    let store = ctx.state::<SettingsStore>();
    let s = downloads::download_storage(&dl, &store);
    let mut shown = StorageData { heading: format!("Saved in {}", s.location).into(), ..StorageData::default() };
    if let (Some(total), Some(free)) = (s.total_bytes.filter(|t| *t > 0), s.free_bytes) {
        let bloom = s.bloom_bytes.min(total);
        let other = total.saturating_sub(free).saturating_sub(bloom);
        shown.has_capacity = true;
        shown.bloom = bloom as f32 / total as f32;
        shown.other = other as f32 / total as f32;
        shown.bloom_text = format!("Bloom {}", bytes_label(bloom)).into();
        shown.other_text = format!("Other files {}", bytes_label(other)).into();
        shown.free_text = format!("Free {}", bytes_label(free)).into();
    }
    shown
}

/// Shows the screen. UI thread.
pub fn show(ctx: &Ctx, weak: &Weak<App>, app: &App) {
    app.set_nav_target(NavTarget::Downloads);
    app.set_downloads_loading(true);
    app.set_downloads_storage(StorageData::default());
    MODELS.with(|m| *m.borrow_mut() = None);
    app.set_downloads_active(ModelRc::new(VecModel::<DownloadData>::default()));
    app.set_downloads_ready(ModelRc::new(VecModel::<DownloadData>::default()));
    app.set_screen(Screen::Downloads);

    let (ctx, weak) = (ctx.clone(), weak.clone());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let list: Vec<View> = serde_json::to_value(downloads::downloads(&jf, &dl)).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        *views() = list.clone();
        let ctx2 = ctx.clone();
        let weak2 = weak.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            if app.get_screen() != Screen::Downloads {
                return;
            }
            apply(&ctx2, &weak2, &app, &list);
            app.set_downloads_storage(storage_data(&ctx2));
            app.set_downloads_loading(false);
        });
    });
}

/// Cancels or deletes the download of `item_id`, whichever it is.
pub fn remove_for_item(ctx: &Ctx, item_id: &str) {
    let id = views().iter().find(|v| v.item_id == item_id).map(|v| v.id.clone());
    if let Some(id) = id {
        remove(ctx, id);
    }
}

pub fn pause(ctx: &Ctx, id: String) {
    downloads::download_pause(&ctx.state::<Downloads>(), id);
}

pub fn resume(ctx: &Ctx, id: String) {
    downloads::download_resume(&ctx.state::<Downloads>(), id);
}

/// Cancels one in progress, or deletes a finished one.
pub fn remove(ctx: &Ctx, id: String) {
    // A show's heading: all of its finished episodes.
    if let Some(series) = id.strip_prefix("group:") {
        let members: Vec<String> = views().iter().filter(|v| v.status == "done" && v.series_id.as_deref() == Some(series)).map(|v| v.id.clone()).collect();
        for member in members {
            downloads::download_remove(&ctx.state::<Downloads>(), member);
        }
        return;
    }
    downloads::download_remove(&ctx.state::<Downloads>(), id);
}

/// `index` is the position in the download's own list of qualities.
pub fn set_quality(ctx: &Ctx, id: String, index: i32) {
    let label = views().iter().find(|v| v.id == id).and_then(|v| usize::try_from(index).ok().and_then(|i| v.qualities.get(i).cloned()));
    if let Some(label) = label {
        downloads::download_set_quality(&ctx.state::<Downloads>(), id, quality_from(&label));
    }
}

/// Queues a film or an episode.
pub fn download_item(ctx: Ctx, weak: Weak<App>, item_id: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let store = ctx.state::<SettingsStore>();
        match downloads::download_item(&jf, &dl, &store, item_id, None).await {
            Ok(()) => watch::say(&weak, "Download queued.".into()),
            Err(e) => watch::say(&weak, e.to_string()),
        }
    });
}

/// Queues every episode of a season that isn't downloaded yet.
pub fn download_season(ctx: Ctx, weak: Weak<App>, series_id: String, season_id: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let store = ctx.state::<SettingsStore>();
        match downloads::download_season(&jf, &dl, &store, series_id, season_id, None).await {
            Ok(0) => watch::say(&weak, "Every episode of this season is already downloaded or queued.".into()),
            Ok(1) => watch::say(&weak, "1 episode queued.".into()),
            Ok(n) => watch::say(&weak, format!("{n} episodes queued.")),
            Err(e) => watch::say(&weak, e.to_string()),
        }
    });
}

/// Queues every season of a show. Episodes already downloaded or queued are left as they are.
pub fn download_show(ctx: Ctx, weak: Weak<App>, series_id: String, seasons: Vec<String>) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let store = ctx.state::<SettingsStore>();
        let mut queued = 0;
        for season in seasons {
            match downloads::download_season(&jf, &dl, &store, series_id.clone(), season, None).await {
                Ok(n) => queued += n,
                Err(e) => return watch::say(&weak, e.to_string()),
            }
        }
        watch::say(&weak, queued_message(queued, "episode", "Every episode of this show is already downloaded or queued."));
    });
}

/// Queues every film in a collection (the shows in one are downloaded from their own pages).
pub fn download_collection(ctx: Ctx, weak: Weak<App>, collection_id: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let dl = ctx.state::<Downloads>();
        let store = ctx.state::<SettingsStore>();
        let items = match bloom_core::library::collection_items_with(&jf, &collection_id).await {
            Ok(items) => items,
            Err(e) => return watch::say(&weak, e.to_string()),
        };
        let (mut queued, mut skipped_shows) = (0, false);
        for card in items {
            match card.kind.as_str() {
                "Movie" | "Video" if of_item(&card.id).is_none() => match downloads::download_item(&jf, &dl, &store, card.id.clone(), None).await {
                    Ok(()) => queued += 1,
                    Err(e) => return watch::say(&weak, e.to_string()),
                },
                "Series" => skipped_shows = true,
                _ => {}
            }
        }
        let mut message = queued_message(queued, "film", "Every film in this collection is already downloaded or queued.");
        if skipped_shows {
            message.push_str(" Shows in it are downloaded from their own pages.");
        }
        watch::say(&weak, message);
    });
}

/// "3 films queued." and the like.
fn queued_message(queued: u32, noun: &str, none: &str) -> String {
    match queued {
        0 => none.to_string(),
        1 => format!("1 {noun} queued."),
        n => format!("{n} {noun}s queued."),
    }
}

/// A download button was pressed: queue it if it isn't listed, otherwise go to the list.
pub fn pressed(ctx: Ctx, weak: Weak<App>, item_id: String) {
    if of_item(&item_id).is_some() {
        nav::go(ctx, weak, Route::Downloads);
    } else {
        download_item(ctx, weak, item_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_in_the_largest_fitting_unit() {
        assert_eq!(bytes_label(0), "0 B");
        assert_eq!(bytes_label(999), "999 B");
        assert_eq!(bytes_label(12_345), "12.3 KB");
        assert_eq!(bytes_label(340_000_000), "340 MB");
        assert_eq!(bytes_label(2_900_000_000), "2.9 GB");
    }

    fn view(status: &str, done: u64, total: Option<u64>) -> View {
        View { status: status.into(), bytes_done: done, bytes_total: total, quality: "original".into(), ..View::default() }
    }

    #[test]
    fn progress_is_a_fraction_and_unknown_totals_are_zero() {
        assert_eq!(view("downloading", 50, Some(200)).fraction(), 0.25);
        assert_eq!(view("downloading", 500, Some(200)).fraction(), 1.0);
        assert_eq!(view("downloading", 50, None).fraction(), 0.0);
        assert_eq!(view("downloading", 50, Some(0)).fraction(), 0.0);
    }

    #[test]
    fn the_line_under_a_download_says_what_it_is_doing() {
        let dot = "  \u{2022}  ";
        let mut v = view("downloading", 1_200_000_000, Some(2_900_000_000));
        assert_eq!(detail(&v, "Home"), format!("1.2 GB of 2.9 GB{dot}Starting"));
        v.bytes_per_second = Some(5_200_000.0);
        // 1.7 GB at 5.2 MB/s: 327 s.
        assert_eq!(detail(&v, "Home"), format!("1.2 GB of 2.9 GB{dot}5.2 MB/s{dot}5m left"));
        v.bytes_done = 2_899_000_000;
        assert!(detail(&v, "Home").ends_with("Under a minute left"));
        // A converted file's size is a guess, so no time is promised.
        v.estimated = true;
        assert_eq!(detail(&v, "Home"), format!("2.9 GB of about 2.9 GB{dot}5.2 MB/s"));
        let mut q = view("queued", 0, None);
        assert_eq!(detail(&q, "Home"), "Queued");
        q.waiting = Some("network".into());
        assert_eq!(detail(&q, "Home"), "Waits for the local network");
        q.waiting = Some("server".into());
        q.bytes_total = Some(500_000_000);
        assert_eq!(detail(&q, "Home"), format!("Waiting for Home{dot}500 MB"));
        assert_eq!(detail(&view("paused", 100, Some(200)), "Home"), format!("Paused{dot}100 B of 200 B"));
        let mut converted = view("paused", 100, Some(200));
        converted.quality = "720p".into();
        assert_eq!(detail(&converted, "Home"), format!("Paused{dot}Starts again when resumed"));
        assert_eq!(detail(&view("failed", 0, None), "Home"), "Not started");
    }

    #[test]
    fn a_finished_download_says_how_much_is_left_to_watch() {
        let dot = "  \u{2022}  ";
        let mut v = View { runtime_seconds: Some(5_400.0), quality: "720p".into(), ..view("done", 900_000_000, Some(900_000_000)) };
        assert_eq!(detail(&v, "Home"), format!("900 MB{dot}720p{dot}1h 30m"));
        v.position_seconds = 1_800.0;
        assert_eq!(detail(&v, "Home"), format!("900 MB{dot}720p{dot}1h left"));
        v.played = true;
        assert_eq!(detail(&v, "Home"), format!("900 MB{dot}720p{dot}Watched"));
    }

    #[test]
    fn what_is_in_progress_is_ordered_by_what_is_happening() {
        let make = |status: &str, added_at: u64| View { status: status.into(), added_at, ..View::default() };
        let (a, b, c, d) = (make("paused", 1), make("downloading", 5), make("queued", 2), make("downloading", 3));
        let mut list = vec![&a, &b, &c, &d];
        in_progress_order(&mut list);
        let order: Vec<(&str, u64)> = list.iter().map(|v| (v.status.as_str(), v.added_at)).collect();
        assert_eq!(order, [("downloading", 3), ("downloading", 5), ("queued", 2), ("paused", 1)]);
    }

    fn episode(id: &str, series: &str, season: i32, number: i32, finished: u64) -> View {
        View { id: id.into(), item_id: id.into(), kind: "Episode".into(), series_id: Some(series.into()), season_number: Some(season), episode_number: Some(number), title: "Show".into(), status: "done".into(), bytes_done: 100, finished_at: Some(finished), quality: "original".into(), ..View::default() }
    }

    #[test]
    fn a_shows_episodes_become_one_heading_with_them_under_it_when_open() {
        let film = View { id: "f".into(), item_id: "f".into(), kind: "Movie".into(), title: "Film".into(), status: "done".into(), quality: "original".into(), ..View::default() };
        let (a, b, c, lone) = (episode("e2", "s1", 1, 2, 5), episode("e1", "s1", 1, 1, 9), episode("e3", "s1", 2, 1, 3), episode("x", "s2", 1, 1, 1));
        let ready: Vec<&View> = vec![&b, &a, &film, &c, &lone];
        let closed = grouped(&ready, "Home", &std::collections::HashSet::new());
        // The show is one heading at the place of its newest episode; a lone episode and a film stay as they are.
        let names: Vec<(&str, bool)> = closed.iter().map(|r| (r.id.as_str(), r.header)).collect();
        assert_eq!(names, [("group:s1", true), ("f", false), ("x", false)]);
        assert_eq!(closed[0].detail, "3 episodes  \u{2022}  300 B");
        let open: std::collections::HashSet<String> = ["s1".to_string()].into();
        let opened = grouped(&ready, "Home", &open);
        let ids: Vec<&str> = opened.iter().map(|r| r.id.as_str()).collect();
        // Under the heading, in the show's own order, indented.
        assert_eq!(ids, ["group:s1", "e1", "e2", "e3", "f", "x"]);
        assert!(opened[1..4].iter().all(|r| r.indent) && !opened[4].indent);
    }

    #[test]
    fn a_queue_is_announced_by_how_many_went_in() {
        assert_eq!(queued_message(0, "film", "none"), "none");
        assert_eq!(queued_message(1, "film", "none"), "1 film queued.");
        assert_eq!(queued_message(12, "episode", "none"), "12 episodes queued.");
    }

    #[test]
    fn a_button_names_the_state_of_its_download() {
        assert_eq!(button_label(None), "Download");
        assert_eq!(button_label(Some(&("done".into(), 1.0))), "Downloaded");
        assert_eq!(button_label(Some(&("paused".into(), 0.3))), "Download paused");
    }

    #[test]
    fn qualities_map_to_the_cores_and_back() {
        assert_eq!(quality_from("1080p"), Quality::Hd1080);
        assert_eq!(quality_from("720p"), Quality::Hd720);
        assert_eq!(quality_from("Original"), Quality::Original);
        assert_eq!(quality_label("original"), "Original");
        assert_eq!(quality_label("720p"), "720p");
    }
}
