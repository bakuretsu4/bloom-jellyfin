//! Playing an item: pick what to play, ask the server how, hand the stream to mpv, and keep the
//! server told where the playhead is so resume points and Continue watching stay in sync.
//!
//! The stream URL and its Authorization header go from here straight to mpv. Neither passes
//! through the page, which only learns titles, the play method, tracks and the playhead.

use crate::jellyfin::{Error, Jellyfin};
use crate::settings::{Settings, SubtitleMode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot};
use url::Url;

const TICKS_PER_SECOND: f64 = 10_000_000.0;
/// Where the ids of a transcode's image subtitles start, so they never clash with mpv's own
/// track ids. Mirrored in src/lib/Watch.svelte.
pub(crate) const SERVER_SUBTITLE_BASE: i64 = 1_000_000;
/// High enough that the server never transcodes for bitrate on a local network.
const MAX_BITRATE: u64 = 200_000_000;

/// How much picture to ask the server for. Original plays the file itself whenever the server
/// allows; the others ask for a transcode no larger and no heavier than their limits, for a slow
/// link. The limits are mirrored in src/lib/Watch.svelte.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Quality {
    #[default]
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "1080p")]
    Hd1080,
    #[serde(rename = "720p")]
    Hd720,
}

impl Quality {
    /// The bitrate cap, and the largest frame as (width, height).
    fn limits(self) -> (u64, Option<(u32, u32)>) {
        match self {
            Quality::Original => (MAX_BITRATE, None),
            Quality::Hd1080 => (12_000_000, Some((1920, 1080))),
            Quality::Hd720 => (6_000_000, Some((1280, 720))),
        }
    }
}
/// How often the server hears the playhead while playing, as Jellyfin's own clients do.
const PROGRESS_INTERVAL: Duration = Duration::from_secs(10);
/// The page doesn't need the playhead at the video's frame rate.
const STATE_INTERVAL: Duration = Duration::from_millis(250);

/// Runs a call into the player where one is built (Linux), and says so everywhere else.
macro_rules! player_call {
    ($call:expr) => {{
        #[cfg(target_os = "linux")]
        let result = $call.map_err(Error::Player);
        #[cfg(not(target_os = "linux"))]
        let result = Err(Error::Player("playback isn't built for this platform yet".into()));
        result
    }};
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Item {
    id: String,
    name: String,
    #[serde(rename = "Type")]
    kind: String,
    series_id: Option<String>,
    series_name: Option<String>,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    production_year: Option<i32>,
    overview: Option<String>,
    image_tags: Option<HashMap<String, String>>,
    backdrop_image_tags: Option<Vec<String>>,
    season_id: Option<String>,
    run_time_ticks: Option<i64>,
    user_data: Option<UserData>,
    /// Seek-bar thumbnails, by media source and then by width.
    trickplay: Option<HashMap<String, HashMap<String, TrickplayDto>>>,
    /// The file's streams, so remembered tracks can be asked for by the server's numbers.
    media_sources: Vec<MediaSource>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct UserData {
    playback_position_ticks: i64,
    played: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct ItemsPage {
    items: Vec<Item>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct PlaybackInfo {
    media_sources: Vec<MediaSource>,
    play_session_id: Option<String>,
    error_code: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct MediaSource {
    id: String,
    container: Option<String>,
    size: Option<i64>,
    supports_direct_play: bool,
    supports_direct_stream: bool,
    transcoding_url: Option<String>,
    run_time_ticks: Option<i64>,
    bitrate: Option<i64>,
    /// The server's pick from the user's Jellyfin language and subtitle settings, as stream
    /// indexes in the file.
    default_audio_stream_index: Option<i64>,
    /// -1 when the user's settings say no subtitles.
    default_subtitle_stream_index: Option<i64>,
    media_streams: Vec<MediaStreamInfo>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct MediaStreamInfo {
    index: i64,
    #[serde(rename = "Type")]
    kind: String,
    language: Option<String>,
    title: Option<String>,
    codec: Option<String>,
    channels: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    is_forced: bool,
    is_hearing_impaired: bool,
    is_external: bool,
    is_text_subtitle_stream: bool,
    delivery_method: Option<String>,
    delivery_url: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Method {
    DirectPlay,
    DirectStream,
    Transcode,
}

impl Method {
    fn api_name(self) -> &'static str {
        match self {
            Method::DirectPlay => "DirectPlay",
            Method::DirectStream => "DirectStream",
            Method::Transcode => "Transcode",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Method::DirectPlay => "Direct play",
            Method::DirectStream => "Direct stream",
            Method::Transcode => "Transcoding",
        }
    }
}

/// What the page is told about what's playing.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    item_id: String,
    /// Jellyfin's item type: "Movie", "Episode" and so on.
    kind: String,
    title: String,
    pub(crate) subtitle: Option<String>,
    overview: Option<String>,
    method: &'static str,
    transcoding: bool,
    pub(crate) duration_seconds: Option<f64>,
    start_seconds: f64,
    pub(crate) quality: Quality,
    /// The file's own picture, for the quality menu.
    source: Option<SourceVideo>,
    /// A downloaded copy, playing from disk.
    pub(crate) downloaded: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceVideo {
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) codec: Option<String>,
    pub(crate) bitrate: Option<i64>,
}

/// A track in the playing file, as mpv lists it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub(crate) id: i64,
    /// "video", "audio" or "sub", as mpv names them.
    pub(crate) kind: String,
    /// The stream's index in the file, which is also how Jellyfin numbers streams.
    pub(crate) ff_index: Option<i64>,
    pub(crate) title: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) codec: Option<String>,
    pub(crate) channels: Option<i64>,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) fps: Option<f64>,
    pub(crate) default: bool,
    pub(crate) forced: bool,
    pub(crate) external: bool,
    /// The file or URL an external track was loaded from.
    pub(crate) external_filename: Option<String>,
    pub(crate) hearing_impaired: bool,
    pub(crate) selected: bool,
}

pub(crate) struct Plan {
    pub(crate) item_id: String,
    /// For an episode, its show.
    series_id: Option<String>,
    media_source_id: String,
    play_session_id: String,
    pub(crate) method: Method,
    /// Absolute. A direct-play URL carries no token; a transcoding URL is issued by the server
    /// with one, which is why a plan's URL only ever goes to mpv.
    pub(crate) url: String,
    pub(crate) start_seconds: f64,
    pub(crate) now_playing: NowPlaying,
    /// Where track choices are remembered: the user plus the show for an episode, or the item.
    scope: String,
    default_audio_index: Option<i64>,
    default_subtitle_index: Option<i64>,
    stream_map: StreamMap,
    /// Subtitle files the server delivers beside the video, for mpv to add once it's open.
    pub(crate) external_subtitles: Vec<ExternalSubtitle>,
    /// While transcoding: the file's audio tracks, numbered by the server. The stream carries
    /// only the one the server was asked for, so choosing another means asking again.
    pub(crate) server_audio: Vec<Track>,
    /// While transcoding: the file's image subtitles, which the stream can only carry drawn into
    /// the picture, so choosing one means asking again (see `burnable_subtitles`).
    pub(crate) server_subtitles: Vec<Track>,
    /// The server's stream number of the subtitle drawn into the picture, if any.
    burned_subtitle: Option<i64>,
    preferences: TrackPreferences,
    /// The download being played, whose resume point is kept on disk too.
    pub(crate) download: Option<String>,
}

/// mpv plays nearly anything, so the profile allows every container and codec for direct play.
/// The transcoding profile only matters when the server's policy or the user's permissions
/// rule direct play out.
fn device_profile(quality: Quality) -> Value {
    // Text subtitles also come as separate files beside the video. Without an "External" method
    // the server's only way to show one is to burn it into the picture, which transcodes the
    // whole film: it did for a third of the dev library.
    const TEXT: [&str; 10] = ["srt", "subrip", "ass", "ssa", "vtt", "webvtt", "sub", "smi", "ttml", "mov_text"];
    const IMAGE: [&str; 5] = ["pgs", "pgssub", "dvdsub", "dvbsub", "idx"];
    let subtitles: Vec<Value> = TEXT
        .iter()
        .chain(IMAGE.iter())
        .map(|f| json!({ "Format": f, "Method": "Embed" }))
        .chain(TEXT.iter().filter(|f| **f != "mov_text").map(|f| json!({ "Format": f, "Method": "External" })))
        .collect();
    let (bitrate, frame) = quality.limits();
    // A frame limit rules direct play out for a larger file, and the server scales the
    // transcode down to fit it.
    let codec_profiles: Vec<Value> = frame
        .map(|(width, height)| {
            json!({ "Type": "Video", "Conditions": [
                { "Condition": "LessThanEqual", "Property": "Width", "Value": width.to_string(), "IsRequired": true },
                { "Condition": "LessThanEqual", "Property": "Height", "Value": height.to_string(), "IsRequired": true }
            ]})
        })
        .into_iter()
        .collect();
    json!({
        "Name": "Bloom",
        "MaxStreamingBitrate": bitrate,
        "DirectPlayProfiles": [{ "Type": "Video" }, { "Type": "Audio" }],
        "TranscodingProfiles": [{
            "Container": "ts", "Type": "Video", "VideoCodec": "h264,hevc", "AudioCodec": "aac,ac3,eac3",
            "Protocol": "hls", "Context": "Streaming", "BreakOnNonKeyFrames": true
        }],
        "ContainerProfiles": [],
        "CodecProfiles": codec_profiles,
        "SubtitleProfiles": subtitles,
    })
}

fn describe(item: &Item) -> (String, Option<String>) {
    if item.kind == "Episode" {
        let code = match (item.parent_index_number, item.index_number) {
            (Some(s), Some(e)) => format!("S{s} E{e}, "),
            (None, Some(e)) => format!("E{e}, "),
            _ => String::new(),
        };
        (item.series_name.clone().unwrap_or_else(|| item.name.clone()), Some(format!("{code}{}", item.name)))
    } else {
        (item.name.clone(), item.production_year.map(|y| y.to_string()))
    }
}

/// The episode to play when a whole show is chosen: the next one up (resuming a half-watched
/// episode), or the first episode once everything has been watched.
async fn episode_to_start(jf: &Jellyfin, uid: &str, series_id: &str) -> Result<Item, Error> {
    let next: ItemsPage = jf
        .get_json(format!(
            "/Shows/NextUp?userId={uid}&seriesId={series_id}&limit=1&enableResumable=true&disableFirstEpisode=false\
             &fields=Overview,MediaSources"
        ))
        .await?;
    if let Some(episode) = next.items.into_iter().next() {
        return Ok(episode);
    }
    let first: ItemsPage = jf
        .get_json(format!("/Shows/{series_id}/Episodes?userId={uid}&limit=1&isMissing=false&fields=Overview,MediaSources"))
        .await?;
    first.items.into_iter().next().ok_or(Error::NothingToPlay)
}

/// How to start an item.
pub(crate) struct PlayOptions<'a> {
    pub(crate) quality: Quality,
    /// None resumes from the server's resume point.
    pub(crate) start_seconds: Option<f64>,
    /// The track choice remembered for a scope (see `Plan::scope`).
    pub(crate) remembered: &'a (dyn Fn(&str) -> Option<TrackChoice> + Sync),
    /// Off, the server is asked to transcode even what this device could play.
    pub(crate) direct_play: bool,
    pub(crate) preferences: TrackPreferences,
}

/// The device's language defaults from Settings, for a show or film nobody has picked tracks for.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TrackPreferences {
    pub(crate) audio_language: Option<String>,
    pub(crate) subtitle_mode: SubtitleMode,
    /// None follows the audio's language.
    pub(crate) subtitle_language: Option<String>,
}

impl TrackPreferences {
    pub(crate) fn from_settings(settings: &Settings) -> Self {
        Self {
            audio_language: settings.audio_language.clone(),
            subtitle_mode: settings.subtitle_mode,
            subtitle_language: settings.subtitle_language.clone(),
        }
    }
}

pub(crate) async fn prepare_with(jf: &Jellyfin, item_id: &str, options: &PlayOptions<'_>) -> Result<Plan, Error> {
    // The id goes into URL paths; refuse anything that could change which endpoint is hit.
    if item_id.is_empty() || !item_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Status(400));
    }
    let uid = jf.user_id()?;
    let mut item: Item = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    if item.kind == "Series" {
        item = episode_to_start(jf, &uid, &item.id).await?;
    }

    let start_seconds = match options.start_seconds.filter(|s| s.is_finite()) {
        Some(seconds) => seconds.max(0.0),
        None => item
            .user_data
            .as_ref()
            .filter(|u| !u.played && u.playback_position_ticks > 0)
            .map_or(0.0, |u| u.playback_position_ticks as f64 / TICKS_PER_SECOND),
    };
    let scope = match item.series_id.as_deref() {
        Some(series) if item.kind == "Episode" => format!("{uid}/{series}"),
        _ => format!("{uid}/{}", item.id),
    };

    let mut request = json!({
        "UserId": uid,
        "MaxStreamingBitrate": options.quality.limits().0,
        "StartTimeTicks": (start_seconds * TICKS_PER_SECOND) as i64,
        "EnableDirectPlay": options.direct_play,
        "EnableDirectStream": options.direct_play,
        "EnableTranscoding": true,
        "AutoOpenLiveStream": true,
        "DeviceProfile": device_profile(options.quality),
    });
    // The remembered tracks go in the request, because a transcode carries only the audio it
    // was asked for. The server ignores stream numbers unless the media source is named too.
    if let Some(file) = item.media_sources.first() {
        request["MediaSourceId"] = json!(file.id);
        let remembered = (options.remembered)(&scope);
        if remembered.is_some() || options.preferences != TrackPreferences::default() {
            let wanted = select_tracks(
                &server_tracks(&file.media_streams, None),
                remembered.as_ref(),
                None,
                None,
                &StreamMap::default(),
                &options.preferences,
            );
            if let Some(audio) = wanted.audio {
                request["AudioStreamIndex"] = json!(audio);
            }
            if let Some(subtitle) = wanted.subtitle {
                request["SubtitleStreamIndex"] = json!(subtitle.unwrap_or(-1));
            }
        }
    }
    let info: PlaybackInfo = jf.post_json(format!("/Items/{}/PlaybackInfo?userId={uid}", item.id), request).await?;
    let refused = |fallback: &str| Error::NotPlayable(info.error_code.clone().unwrap_or_else(|| fallback.to_string()));
    let source = info.media_sources.first().ok_or_else(|| refused("the server offered no media for it"))?;
    let play_session_id = info.play_session_id.clone().unwrap_or_default();

    let (address, _) = jf.media_access()?;
    let static_url = format!(
        "{address}/Videos/{}/stream?static=true&mediaSourceId={}&playSessionId={play_session_id}&deviceId={}",
        item.id,
        source.id,
        jf.device_id()
    );
    let (method, url) = if source.supports_direct_play {
        (Method::DirectPlay, static_url)
    } else if let Some(path) = source.transcoding_url.as_deref() {
        (Method::Transcode, format!("{address}{path}"))
    } else if source.supports_direct_stream {
        (Method::DirectStream, static_url)
    } else {
        return Err(refused("the server offered no way to stream it"));
    };

    let transcoding = method == Method::Transcode;
    // Subtitles the server hands out as files: files beside the video, and during a transcode
    // the file's own text subtitles as well, which the stream can't carry.
    let external_subtitles: Vec<ExternalSubtitle> = source
        .media_streams
        .iter()
        .filter(|s| s.kind == "Subtitle" && s.delivery_method.as_deref() == Some("External"))
        .filter_map(|s| {
            Some(ExternalSubtitle {
                index: s.index,
                url: without_token(&address, s.delivery_url.as_deref()?),
                title: s.title.clone().unwrap_or_else(|| "External file".into()),
                lang: s.language.clone().unwrap_or_default(),
            })
        })
        .collect();
    let stream_map = StreamMap {
        // A transcode shows mpv none of the file's own streams, only the files added beside it.
        external_count: if transcoding { 0 } else { source.media_streams.iter().filter(|s| s.is_external).count() as i64 },
        external: external_subtitles.iter().map(|s| (s.index, s.url.clone())).collect(),
    };
    let server_audio = if transcoding {
        let chosen = query_index(&url, "AudioStreamIndex").or(source.default_audio_stream_index);
        server_tracks(&source.media_streams, chosen).into_iter().filter(|t| t.kind == "audio").collect()
    } else {
        Vec::new()
    };
    // The stream's address names the subtitle it draws into the picture.
    let burned_subtitle = query_index(&url, "SubtitleStreamIndex")
        .filter(|index| source.media_streams.iter().any(|s| s.index == *index && s.delivery_method.as_deref() == Some("Encode")));
    let server_subtitles = if transcoding { burnable_subtitles(&source.media_streams, burned_subtitle) } else { Vec::new() };

    let (title, subtitle) = describe(&item);
    let duration_seconds = source.run_time_ticks.map(|t| t as f64 / TICKS_PER_SECOND);
    let video = source.media_streams.iter().find(|s| s.kind == "Video");
    Ok(Plan {
        now_playing: NowPlaying {
            item_id: item.id.clone(),
            kind: item.kind.clone(),
            title,
            subtitle,
            overview: crate::home::plain_text(item.overview.clone()),
            method: method.label(),
            transcoding: method == Method::Transcode,
            duration_seconds,
            start_seconds,
            quality: options.quality,
            source: video.map(|v| SourceVideo {
                width: v.width,
                height: v.height,
                codec: v.codec.clone(),
                bitrate: source.bitrate,
            }),
            downloaded: false,
        },
        server_audio,
        server_subtitles,
        burned_subtitle,
        download: None,
        preferences: options.preferences.clone(),
        media_source_id: source.id.clone(),
        default_audio_index: source.default_audio_stream_index,
        default_subtitle_index: source.default_subtitle_stream_index,
        scope,
        stream_map,
        external_subtitles,
        series_id: item.series_id.clone(),
        item_id: item.id,
        play_session_id,
        method,
        url,
        start_seconds,
    })
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct TrickplayDto {
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    thumbnail_count: u32,
    /// Milliseconds between thumbnails.
    interval: u32,
}

/// Jellyfin's seek-bar thumbnails for an item: square-ish tiles of `tile_width` by `tile_height`
/// thumbnails, one thumbnail every `interval_ms`, served through `bloom-img://trickplay/`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trickplay {
    item_id: String,
    media_source_id: String,
    /// One thumbnail's size.
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
    count: u32,
    interval_ms: u32,
}

/// The item's thumbnails at the width nearest 320px, or None where the server hasn't made any
/// (it only does when trickplay extraction is turned on for the library).
pub(crate) async fn trickplay_with(jf: &Jellyfin, item_id: &str) -> Result<Option<Trickplay>, Error> {
    if item_id.is_empty() || !item_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Status(400));
    }
    let uid = jf.user_id()?;
    let item: Item = jf.get_json(format!("/Items/{item_id}?userId={uid}&fields=Trickplay,MediaSources")).await?;
    let sets = item.trickplay.unwrap_or_default();
    let usable = |id: &String| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric());
    let preferred = item.media_sources.first().and_then(|s| sets.get_key_value(&s.id));
    let Some((source_id, widths)) = preferred.or_else(|| sets.iter().next()).filter(|(id, _)| usable(id)) else {
        return Ok(None);
    };
    let best = widths
        .values()
        .filter(|t| t.width > 0 && t.height > 0 && t.tile_width > 0 && t.tile_height > 0 && t.thumbnail_count > 0 && t.interval > 0)
        .min_by_key(|t| t.width.abs_diff(320));
    Ok(best.map(|t| Trickplay {
        item_id: item.id.clone(),
        media_source_id: source_id.clone(),
        width: t.width,
        height: t.height,
        tile_width: t.tile_width,
        tile_height: t.tile_height,
        count: t.thumbnail_count,
        interval_ms: t.interval,
    }))
}

#[tauri::command]
pub async fn trickplay(jf: State<'_, Jellyfin>, item_id: String) -> Result<Option<Trickplay>, Error> {
    trickplay_with(&jf, &item_id).await
}

/// The episode after the one playing, for the up-next countdown.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpNext {
    pub(crate) item_id: String,
    pub(crate) title: String,
    pub(crate) subtitle: Option<String>,
    pub(crate) image: Option<crate::home::Image>,
}

/// The next episode in the show's own order, across seasons. None after the last episode, and
/// for anything that isn't an episode.
pub(crate) async fn next_episode_after(jf: &Jellyfin, item_id: &str) -> Result<Option<UpNext>, Error> {
    if item_id.is_empty() || !item_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Status(400));
    }
    let uid = jf.user_id()?;
    let item: Item = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    let (true, Some(series_id)) = (item.kind == "Episode", item.series_id.as_deref()) else {
        return Ok(None);
    };
    // startItemId skips the list up to this episode, so the second item is the next one.
    let page: ItemsPage = jf
        .get_json(format!(
            "/Shows/{series_id}/Episodes?userId={uid}&startItemId={item_id}&limit=2&isMissing=false\
             &enableImageTypes=Primary&imageTypeLimit=1"
        ))
        .await?;
    let next = page.items.into_iter().skip_while(|e| e.id != item.id).nth(1);
    Ok(next.map(|episode| {
        let (title, subtitle) = describe(&episode);
        let image = episode.image_tags.as_ref().and_then(|tags| tags.get("Primary")).map(|tag| crate::home::Image {
            item_id: episode.id.clone(),
            kind: "Primary",
            tag: tag.clone(),
        });
        UpNext { item_id: episode.id, title, subtitle, image }
    }))
}

/// One item from loading to stopping.
pub(crate) struct Session {
    item_id: String,
    media_source_id: String,
    play_session_id: String,
    method: Method,
    /// mpv's id for this file, learned when it starts loading.
    entry: Option<i64>,
    reported_start: bool,
    started: bool,
    pub(crate) position: f64,
    duration: Option<f64>,
    paused: bool,
    buffering: bool,
    decoder: Option<String>,
    cache_end: Option<f64>,
    audio_track: Option<i64>,
    subtitle_track: Option<i64>,
    last_progress: Instant,
    scope: String,
    default_audio_index: Option<i64>,
    default_subtitle_index: Option<i64>,
    tracks: Vec<Track>,
    /// Remembered or default tracks are applied once, when the file has loaded.
    tracks_chosen: bool,
    stream_map: StreamMap,
    /// Taken when the file loads, and added to mpv then.
    pending_subtitles: Vec<ExternalSubtitle>,
    /// See `Plan::server_audio`.
    server_audio: Vec<Track>,
    /// See `Plan::server_subtitles`.
    server_subtitles: Vec<Track>,
    burned_subtitle: Option<i64>,
    preferences: TrackPreferences,
    download: Option<String>,
    /// What Discord presence says is playing.
    kind: String,
    title: String,
    subtitle: Option<String>,
    series_id: Option<String>,
    /// The server hears about it, and Discord shows it. Not for a trailer.
    reports: bool,
}

impl Session {
    pub(crate) fn new(plan: &Plan) -> Self {
        Self {
            item_id: plan.item_id.clone(),
            media_source_id: plan.media_source_id.clone(),
            play_session_id: plan.play_session_id.clone(),
            method: plan.method,
            entry: None,
            reported_start: false,
            started: false,
            position: plan.start_seconds,
            duration: plan.now_playing.duration_seconds,
            paused: false,
            buffering: false,
            decoder: None,
            cache_end: None,
            audio_track: None,
            subtitle_track: None,
            last_progress: Instant::now(),
            scope: plan.scope.clone(),
            default_audio_index: plan.default_audio_index,
            default_subtitle_index: plan.default_subtitle_index,
            tracks: Vec::new(),
            tracks_chosen: false,
            stream_map: plan.stream_map.clone(),
            pending_subtitles: plan.external_subtitles.clone(),
            server_audio: plan.server_audio.clone(),
            server_subtitles: plan.server_subtitles.clone(),
            burned_subtitle: plan.burned_subtitle,
            preferences: plan.preferences.clone(),
            download: plan.download.clone(),
            kind: plan.now_playing.kind.clone(),
            title: plan.now_playing.title.clone(),
            subtitle: plan.now_playing.subtitle.clone(),
            series_id: plan.series_id.clone(),
            reports: plan.now_playing.kind != "Trailer",
        }
    }

    fn activity(&self, speed: Option<f64>) -> crate::discord::Activity {
        crate::discord::Activity {
            kind: self.kind.clone(),
            title: self.title.clone(),
            subtitle: self.subtitle.clone(),
            position: self.position,
            duration: self.duration,
            paused: self.paused,
            speed: speed.unwrap_or(1.0),
            at: Instant::now(),
            // An episode's cover is its show's poster.
            art_item: Some(if self.kind == "Episode" { self.series_id.clone().unwrap_or_else(|| self.item_id.clone()) } else { self.item_id.clone() }),
        }
    }

    /// mpv's tracks, with the file's audio tracks standing in for a transcode's single one, and
    /// the file's image subtitles added to the subtitles mpv has.
    fn merge_tracks(&self, mut tracks: Vec<Track>) -> Vec<Track> {
        // A trailer's separate audio stream is named after its web address by mpv; it's just
        // the trailer's sound, known by its language.
        if !self.reports {
            for track in tracks.iter_mut().filter(|t| t.external && t.kind == "audio") {
                track.title = None;
                track.external = false;
            }
        }
        if !self.server_audio.is_empty() {
            tracks.retain(|t| t.kind != "audio");
            tracks.extend(self.server_audio.iter().cloned());
        }
        if !self.server_subtitles.is_empty() {
            // While the picture carries a subtitle, that's the one selected, whatever mpv says.
            if self.burned_subtitle.is_some() {
                for track in tracks.iter_mut().filter(|t| t.kind == "sub") {
                    track.selected = false;
                }
            }
            tracks.extend(self.server_subtitles.iter().cloned());
        }
        tracks
    }

    fn report_body(&self, event: Option<&str>) -> Value {
        let mut body = json!({
            "ItemId": self.item_id,
            "MediaSourceId": self.media_source_id,
            "PlaySessionId": self.play_session_id,
            "PlayMethod": self.method.api_name(),
            "PositionTicks": (self.position * TICKS_PER_SECOND) as i64,
            "IsPaused": self.paused,
            "CanSeek": true,
        });
        // Lets the server, and so other Jellyfin clients, see which tracks are in use.
        let index_of = |id: Option<i64>, kind: &str| {
            id.and_then(|id| self.tracks.iter().find(|t| t.kind == kind && t.id == id)).and_then(|t| self.stream_map.index_of(t))
        };
        if let Some(index) = index_of(self.audio_track, "audio") {
            body["AudioStreamIndex"] = json!(index);
        }
        if !self.tracks.is_empty() {
            body["SubtitleStreamIndex"] = json!(index_of(self.subtitle_track, "sub").unwrap_or(-1));
        }
        if let Some(name) = event {
            body["EventName"] = json!(name);
        }
        body
    }

    pub(crate) fn playing_report(&self) -> Report {
        Report::Playing(self.report_body(None))
    }

    pub(crate) fn progress_report(&self, event: &str) -> Report {
        Report::Progress(self.report_body(Some(event)))
    }

    pub(crate) fn stopped_report(&self) -> Report {
        Report::Stopped {
            body: self.report_body(None),
            transcode_session: (self.method == Method::Transcode).then(|| self.play_session_id.clone()),
        }
    }

    /// What the server needs to hear when this session ends: Stopped once it heard Playing, and
    /// otherwise, for a transcode, that its encoding can stop (mpv asks for the stream, which
    /// starts ffmpeg, well before the file has loaded).
    fn ending_report(&self) -> Option<Report> {
        if self.reported_start {
            Some(self.stopped_report())
        } else if self.method == Method::Transcode && !self.play_session_id.is_empty() {
            Some(Report::EndEncoding(self.play_session_id.clone()))
        } else {
            None
        }
    }

    fn state(&self, volume: Option<f64>, muted: bool, speed: Option<f64>) -> PlayerState {
        PlayerState {
            item_id: self.item_id.clone(),
            position: self.position,
            duration: self.duration,
            paused: self.paused,
            buffering: self.buffering,
            started: self.started,
            decoder: self.decoder.clone(),
            cache_end: self.cache_end,
            volume: volume.unwrap_or(100.0),
            muted,
            speed: speed.unwrap_or(1.0),
            audio_track: self.audio_track,
            subtitle_track: self.subtitle_track,
        }
    }
}

pub(crate) enum Report {
    Playing(Value),
    Progress(Value),
    Stopped { body: Value, transcode_session: Option<String> },
    /// A transcode's session ended before the server heard it start: only its encoding to stop.
    EndEncoding(String),
    /// Answers once every report queued before it has been sent.
    Flush(oneshot::Sender<()>),
}

/// Sends a report as the account signed in now (the live test's way in).
#[cfg(test)]
pub(crate) async fn send_report(jf: &Jellyfin, report: Report) -> Result<(), Error> {
    send_report_as(jf, &jf.credentials()?, report).await
}

/// Sends a report as `who`, the account that played.
pub(crate) async fn send_report_as(jf: &Jellyfin, who: &crate::jellyfin::Credentials, report: Report) -> Result<(), Error> {
    use reqwest::Method;
    // Otherwise the server keeps its ffmpeg running until it times out.
    let encoding = |play_session_id: &str| format!("/Videos/ActiveEncodings?deviceId={}&playSessionId={play_session_id}", jf.device_id());
    match report {
        Report::Playing(body) => jf.request_as(who, Method::POST, "/Sessions/Playing", Some(body)).await.map(|_| ()),
        Report::Progress(body) => jf.request_as(who, Method::POST, "/Sessions/Playing/Progress", Some(body)).await.map(|_| ()),
        Report::Stopped { body, transcode_session } => {
            let stopped = jf.request_as(who, Method::POST, "/Sessions/Playing/Stopped", Some(body)).await.map(|_| ());
            if let Some(play_session_id) = transcode_session {
                let _ = jf.request_as(who, Method::DELETE, &encoding(&play_session_id), None).await;
            }
            stopped
        }
        Report::EndEncoding(play_session_id) => jf.request_as(who, Method::DELETE, &encoding(&play_session_id), None).await.map(|_| ()),
        Report::Flush(done) => {
            let _ = done.send(());
            Ok(())
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    item_id: String,
    position: f64,
    duration: Option<f64>,
    paused: bool,
    buffering: bool,
    /// The first frame has been shown.
    started: bool,
    decoder: Option<String>,
    /// The media time buffered up to.
    cache_end: Option<f64>,
    volume: f64,
    muted: bool,
    /// 1 is normal.
    speed: f64,
    audio_track: Option<i64>,
    subtitle_track: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerTracks {
    item_id: String,
    tracks: Vec<Track>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerEnded {
    item_id: String,
    /// "finished" or "error". A stop Bloom asked for isn't announced.
    reason: &'static str,
    message: Option<String>,
}

#[derive(Default)]
struct Inner {
    session: Option<Session>,
    /// Bumped by every play and stop request; a play that finds it moved on gives way.
    generation: u64,
    /// The file mpv last said it started, which the events after it are about.
    started_entry: Option<i64>,
    last_state: Option<Instant>,
    /// The mixer belongs to the player rather than to one file.
    volume: Option<f64>,
    muted: bool,
    speed: Option<f64>,
}

pub struct Playback {
    app: AppHandle,
    inner: Mutex<Inner>,
    /// See `loading`.
    loading: Mutex<()>,
    /// Each report with the account it's for, taken when it was queued.
    reports: mpsc::UnboundedSender<(Option<crate::jellyfin::Credentials>, Report)>,
    /// Track choices by scope, kept in `choices_path`.
    choices: Mutex<HashMap<String, TrackChoice>>,
    choices_path: PathBuf,
}

impl Playback {
    pub fn new(app: AppHandle, choices_path: PathBuf) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<(Option<crate::jellyfin::Credentials>, Report)>();
        let worker_app = app.clone();
        // One worker, so the server hears Playing, Progress and Stopped in the order they happened.
        tauri::async_runtime::spawn(async move {
            let app = worker_app;
            while let Some((who, report)) = rx.recv().await {
                let jf = app.state::<Jellyfin>();
                let sent = match (who, report) {
                    (_, Report::Flush(done)) => {
                        let _ = done.send(());
                        Ok(())
                    }
                    (Some(who), report) => send_report_as(&jf, &who, report).await,
                    (None, _) => Err(Error::SignedOut),
                };
                if let Err(e) = sent {
                    eprintln!("bloom: playback report not delivered: {e}");
                }
            }
        });
        let choices = fs::read(&choices_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { app, inner: Mutex::default(), loading: Mutex::default(), reports: tx, choices: Mutex::new(choices), choices_path }
    }

    fn choices(&self) -> MutexGuard<'_, HashMap<String, TrackChoice>> {
        self.choices.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The track choice remembered for a scope, for a converted download picking its one audio track.
    pub(crate) fn remembered(&self, scope: &str) -> Option<TrackChoice> {
        self.choices().get(scope).cloned()
    }

    /// Keep a downloaded copy's resume point on disk as well, so it resumes offline and reaches
    /// the server once it answers. `stopped` is the end of a session rather than a progress tick.
    fn record_local(&self, session: &Session, stopped: bool, finished: bool) {
        let Some(id) = session.download.as_deref().filter(|_| session.reported_start) else { return };
        let downloads = self.app.state::<crate::downloads::Downloads>();
        downloads.record_watch(id, session.position, session.duration, stopped, finished);
    }

    /// Record a track picked by hand for the playing item's scope, so the next episode of the
    /// show (or the next time the film plays) starts with the same choice.
    fn remember(&self, audio: bool, id: Option<i64>) {
        let (scope, hint) = {
            let inner = self.lock();
            let Some(s) = inner.session.as_ref() else { return };
            let kind = if audio { "audio" } else { "sub" };
            let track = id.and_then(|id| s.tracks.iter().find(|t| t.kind == kind && t.id == id));
            // A track not in the list yet can't be described well enough to find again.
            if id.is_some() && track.is_none() {
                return;
            }
            (s.scope.clone(), track.map(|t| TrackHint::of(t, &s.tracks)))
        };
        let mut choices = self.choices();
        let choice = choices.entry(scope).or_default();
        if audio {
            // Turning audio off isn't a preference worth carrying to the next episode.
            let Some(hint) = hint else { return };
            choice.audio = Some(hint);
        } else {
            choice.subtitle = Some(hint.map_or(SubtitleChoice::Off, SubtitleChoice::Track));
        }
        let Ok(bytes) = serde_json::to_vec(&*choices) else { return };
        let tmp = self.choices_path.with_extension("tmp");
        if fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, &self.choices_path)).is_err() {
            eprintln!("bloom: couldn't save track choices to {}", self.choices_path.display());
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Held while a play request swaps in its session and loads its file, and while mpv's events
    /// are handled, so an event is never matched against a session that's half set up. Taken
    /// before `lock` when both are needed. Never held across an await.
    fn loading(&self) -> MutexGuard<'_, ()> {
        self.loading.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Marks a new play or stop request: anything asked for earlier gives way to it.
    fn next_generation(&self) -> u64 {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.generation
    }

    /// Queues a report, as the account signed in now.
    fn report(&self, report: Report) {
        let who = self.app.state::<Jellyfin>().credentials().ok();
        let _ = self.reports.send((who, report));
    }

    /// Everything a session needs when it's over: the server's report, and a downloaded copy's
    /// resume point.
    fn end(&self, session: Session, finished: bool) {
        if let Some(report) = session.ending_report() {
            self.report(report);
        }
        self.record_local(&session, true, finished);
        self.release_video_later();
    }

    /// Frees the video output (about 80 MB of NVIDIA driver memory) and restarts mpv (whose
    /// demuxer cache, up to 150 MB, stays allocated after a file stops) once nothing has played
    /// for `video_kept()`, so browsing after watching goes back to what browsing costs. Not
    /// sooner: coming straight back to the player stays instant. Skipped if anything was asked
    /// to play or stop meanwhile.
    fn release_video_later(&self) {
        let generation = self.lock().generation;
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(video_kept()).await;
            #[cfg(target_os = "linux")]
            crate::player::release_video(move || {
                let pb = app.state::<Playback>();
                let inner = pb.lock();
                inner.session.is_none() && inner.generation == generation
            });
            #[cfg(not(target_os = "linux"))]
            let _ = (app, generation);
        });
    }

    /// A session is open, playing or still loading: closing the window should end it first.
    pub fn is_playing(&self) -> bool {
        self.lock().session.is_some()
    }
}

/// The window is closing mid-playback: tell the server where playback stopped, and wait briefly
/// for it to hear, so the resume point is exact rather than the last ten-second report and the
/// server stops listing the item as playing (or encoding it).
pub async fn report_stop_before_exit(pb: &Playback) {
    let session = {
        let _loading = pb.loading();
        pb.next_generation();
        pb.lock().session.take()
    };
    let Some(session) = session else { return };
    pb.end(session, false);
    let (done, finished) = oneshot::channel();
    pb.report(Report::Flush(done));
    let _ = tokio::time::timeout(Duration::from_secs(3), finished).await;
}

/// Keeps the session in step with mpv, reports to the server, and tells the page.
#[cfg(target_os = "linux")]
pub fn on_player_event(app: &AppHandle, event: crate::player::Event) {
    use crate::player::{EndReason, Event};
    use tauri::Emitter;

    let pb = app.state::<Playback>();
    let now = Instant::now();
    // Not while a play request is swapping in its session (see `Playback::loading`); held to the
    // end, so external subtitles can't be added to a file that replaced this one meanwhile.
    let _loading = pb.loading();
    let mut guard = pb.lock();
    let inner = &mut *guard;

    match &event {
        Event::Volume(volume) => inner.volume = Some(*volume),
        Event::Muted(muted) => inner.muted = *muted,
        Event::Speed(speed) => inner.speed = Some(*speed),
        Event::StartFile { entry } => inner.started_entry = Some(*entry),
        _ => {}
    }
    let (volume, muted, speed, started_entry) = (inner.volume, inner.muted, inner.speed, inner.started_entry);
    let Some(s) = inner.session.as_mut() else { return };
    // Should mpv not have said which entry it loaded, the next file to start is this session's.
    if let (None, Event::StartFile { entry }) = (s.entry, &event) {
        s.entry = Some(*entry);
    }
    // mpv's events come in order, so until this session's file has started, whatever is said about
    // a file is about the one this session replaced, still on its way out. EndFile names its file.
    let about_a_file = !matches!(event, Event::Volume(_) | Event::Muted(_) | Event::Speed(_) | Event::EndFile { .. });
    if about_a_file && (s.entry.is_none() || s.entry != started_entry) {
        return;
    }

    let throttled = matches!(event, Event::Position(_) | Event::CacheEnd(_));
    let mut changed = false;
    // Discord presence changes with a start, a seek, a pause or the speed, not with the clock.
    let mut presence = false;
    match event {
        Event::Volume(_) | Event::Muted(_) => changed = true,
        Event::Speed(_) => {
            changed = true;
            presence = true;
        }
        Event::StartFile { .. } => {}
        Event::FileLoaded => {
            if !s.reported_start && s.reports {
                s.reported_start = true;
                s.last_progress = now;
                pb.report(s.playing_report());
            }
            let subtitles = std::mem::take(&mut s.pending_subtitles);
            drop(guard);
            // Subtitle files join the file's own tracks before anything is chosen, so a remembered
            // external subtitle can be found again.
            for subtitle in &subtitles {
                if let Err(e) = crate::player::add_subtitle(&subtitle.url, &subtitle.title, &subtitle.lang) {
                    eprintln!("bloom: couldn't load an external subtitle: {e}");
                }
            }
            apply_track_choice(&pb);
            return;
        }
        Event::PlaybackRestart => {
            s.started = true;
            changed = true;
            presence = true;
        }
        Event::Position(seconds) => {
            s.position = seconds;
            if s.reported_start && !s.paused && now.duration_since(s.last_progress) >= PROGRESS_INTERVAL {
                s.last_progress = now;
                pb.report(s.progress_report("TimeUpdate"));
                pb.record_local(s, false, false);
            }
        }
        Event::CacheEnd(seconds) => s.cache_end = Some(seconds),
        Event::Duration(seconds) => {
            s.duration = Some(seconds);
            changed = true;
        }
        Event::Paused(paused) => {
            if paused != s.paused {
                s.paused = paused;
                changed = true;
                presence = true;
                if s.reported_start {
                    s.last_progress = now;
                    pb.report(s.progress_report(if paused { "Pause" } else { "Unpause" }));
                }
            }
        }
        Event::Buffering(buffering) => {
            s.buffering = buffering;
            changed = true;
        }
        Event::Decoder(decoder) => {
            s.decoder = decoder;
            changed = true;
        }
        Event::Tracks(tracks) => {
            // mpv re-sends the list on every track switch, so it's the source of truth for what's
            // selected (see OBSERVED in player.rs).
            let tracks = s.merge_tracks(tracks);
            let selected = |kind: &str| tracks.iter().find(|t| t.kind == kind && t.selected).map(|t| t.id);
            s.audio_track = selected("audio");
            s.subtitle_track = selected("sub");
            s.tracks = tracks.clone();
            inner.last_state = Some(now);
            let state = s.state(volume, muted, speed);
            let payload = PlayerTracks { item_id: s.item_id.clone(), tracks };
            drop(guard);
            let _ = app.emit("player:tracks", payload);
            let _ = app.emit("player:state", state);
            return;
        }
        Event::EndFile { entry, reason } => {
            // The end of a file this session replaced, still on its way out.
            if s.entry != Some(entry) {
                return;
            }
            let mut ended = inner.session.take().expect("matched above");
            drop(guard);
            let finished = matches!(reason, EndReason::Finished);
            if finished {
                if let Some(duration) = ended.duration {
                    ended.position = duration;
                }
            }
            let item_id = ended.item_id.clone();
            pb.end(ended, finished);
            // The next episode, if one starts, sets it again.
            app.state::<crate::discord::Presence>().show(None);
            let (reason, message) = match reason {
                EndReason::Error(message) => ("error", Some(message)),
                // Bloom takes the session before stopping on purpose, so a stop here is mpv's.
                EndReason::Finished | EndReason::Stopped => ("finished", None),
            };
            let _ = app.emit("player:ended", PlayerEnded { item_id, reason, message });
            return;
        }
    }

    if presence && s.reports {
        app.state::<crate::discord::Presence>().show(Some(s.activity(speed)));
    }
    let due = inner.last_state.is_none_or(|t| now.duration_since(t) >= STATE_INTERVAL);
    if changed || (throttled && due) {
        inner.last_state = Some(now);
        let state = s.state(volume, muted, speed);
        drop(guard);
        let _ = app.emit("player:state", state);
    }
}

/// Select the remembered tracks for this show or film, or the server's defaults, once per file.
#[cfg(target_os = "linux")]
fn apply_track_choice(pb: &Playback) {
    let tracks = crate::player::tracks();
    let (selection, transcoding, burned) = {
        let mut guard = pb.lock();
        let Some(s) = guard.session.as_mut() else { return };
        if s.tracks_chosen {
            return;
        }
        s.tracks_chosen = true;
        let tracks = s.merge_tracks(tracks);
        s.tracks = tracks.clone();
        let choices = pb.choices();
        let selection = select_tracks(
            &tracks,
            choices.get(&s.scope),
            s.default_audio_index,
            s.default_subtitle_index,
            &s.stream_map,
            &s.preferences,
        );
        (selection, !s.server_audio.is_empty(), s.burned_subtitle.is_some())
    };
    // A transcode's audio was picked in the request, and its stream has no other to switch to.
    if let Some(audio) = selection.audio.filter(|_| !transcoding) {
        let _ = crate::player::set_track(true, Some(audio));
    }
    // A subtitle drawn into the picture was also picked in the request: mpv shows no other over
    // it, and an image subtitle can't be selected in mpv at all.
    let for_mpv = |choice: &Option<i64>| !burned && choice.is_none_or(|id| id < SERVER_SUBTITLE_BASE);
    if let Some(subtitle) = selection.subtitle.filter(for_mpv) {
        let _ = crate::player::set_track(false, subtitle);
    }
}

/// What a download keeps about its item: enough to list it and to play it with no server.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct DownloadFacts {
    pub(crate) item_id: String,
    pub(crate) kind: String,
    /// The show for an episode, as the player titles it.
    pub(crate) title: String,
    /// "S1 E4, Name" for an episode, the year for a film.
    pub(crate) subtitle: Option<String>,
    /// The item's own name: an episode's title, a film's.
    pub(crate) name: String,
    pub(crate) year: Option<i32>,
    pub(crate) overview: Option<String>,
    pub(crate) series_id: Option<String>,
    pub(crate) season_id: Option<String>,
    pub(crate) season_number: Option<i32>,
    pub(crate) episode_number: Option<i32>,
    /// Artwork for the downloads list: an episode's still, or a film's backdrop.
    pub(crate) image: Option<StoredImage>,
    pub(crate) runtime_seconds: Option<f64>,
    pub(crate) media_source_id: String,
    pub(crate) container: Option<String>,
    pub(crate) size: Option<u64>,
    pub(crate) default_audio_index: Option<i64>,
    pub(crate) default_subtitle_index: Option<i64>,
    /// How many of the server's streams are files beside the video (see `StreamMap`).
    pub(crate) external_count: i64,
    pub(crate) source: Option<SourceVideo>,
    /// The audio and subtitle streams, numbered by the server.
    pub(crate) streams: Vec<StoredStream>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct StoredImage {
    pub(crate) item_id: String,
    pub(crate) kind: String,
    pub(crate) tag: String,
}

impl StoredImage {
    pub(crate) fn image(&self) -> Option<crate::home::Image> {
        let kind = ["Primary", "Thumb", "Backdrop", "Logo"].into_iter().find(|k| *k == self.kind)?;
        Some(crate::home::Image { item_id: self.item_id.clone(), kind, tag: self.tag.clone() })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct StoredStream {
    pub(crate) index: i64,
    /// "Audio" or "Subtitle", as Jellyfin names them.
    pub(crate) kind: String,
    pub(crate) codec: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) channels: Option<i64>,
    pub(crate) external: bool,
    /// A text subtitle, which the server can hand out as a file.
    pub(crate) text: bool,
    pub(crate) forced: bool,
    pub(crate) hearing_impaired: bool,
}

/// A subtitle saved beside a downloaded video.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalSubtitle {
    /// The server's stream number for it.
    pub(crate) index: i64,
    /// The file's name in the download's folder.
    pub(crate) file: String,
    pub(crate) title: String,
    pub(crate) lang: String,
}

pub(crate) async fn download_facts_with(jf: &Jellyfin, item_id: &str) -> Result<DownloadFacts, Error> {
    if item_id.is_empty() || !item_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Status(400));
    }
    let uid = jf.user_id()?;
    let item: Item = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    if !matches!(item.kind.as_str(), "Movie" | "Episode" | "Video" | "MusicVideo") {
        return Err(Error::Download("Only films and episodes can be downloaded.".into()));
    }
    let source = item.media_sources.first().ok_or_else(|| Error::NotPlayable("the server has no file for it".into()))?;
    let (title, subtitle) = describe(&item);
    let tag = |kind: &str| item.image_tags.as_ref().and_then(|tags| tags.get(kind)).cloned().map(|tag| (kind.to_string(), tag));
    let backdrop = || item.backdrop_image_tags.as_ref().and_then(|tags| tags.first()).map(|tag| ("Backdrop".to_string(), tag.clone()));
    let image = if item.kind == "Episode" { tag("Primary") } else { backdrop().or_else(|| tag("Thumb")).or_else(|| tag("Primary")) };
    let video = source.media_streams.iter().find(|s| s.kind == "Video");
    Ok(DownloadFacts {
        item_id: item.id.clone(),
        kind: item.kind.clone(),
        title,
        subtitle,
        name: item.name.clone(),
        year: item.production_year,
        overview: crate::home::plain_text(item.overview.clone()),
        series_id: item.series_id.clone(),
        season_id: item.season_id.clone(),
        season_number: item.parent_index_number,
        episode_number: item.index_number,
        image: image.map(|(kind, tag)| StoredImage { item_id: item.id.clone(), kind, tag }),
        runtime_seconds: source.run_time_ticks.or(item.run_time_ticks).map(|t| t as f64 / TICKS_PER_SECOND),
        media_source_id: source.id.clone(),
        container: source.container.clone(),
        size: source.size.and_then(|s| u64::try_from(s).ok()),
        default_audio_index: source.default_audio_stream_index,
        default_subtitle_index: source.default_subtitle_stream_index,
        external_count: source.media_streams.iter().filter(|s| s.is_external).count() as i64,
        source: video.map(|v| SourceVideo { width: v.width, height: v.height, codec: v.codec.clone(), bitrate: source.bitrate }),
        streams: source
            .media_streams
            .iter()
            .filter(|s| matches!(s.kind.as_str(), "Audio" | "Subtitle"))
            .map(|s| StoredStream {
                index: s.index,
                kind: s.kind.clone(),
                codec: s.codec.clone(),
                language: s.language.clone(),
                title: s.title.clone(),
                channels: s.channels,
                external: s.is_external,
                text: s.is_text_subtitle_stream,
                forced: s.is_forced,
                hearing_impaired: s.is_hearing_impaired,
            })
            .collect(),
    })
}

/// Where track choices for a downloaded item are remembered: the same scope as when streaming.
pub(crate) fn download_scope(uid: &str, facts: &DownloadFacts) -> String {
    match facts.series_id.as_deref() {
        Some(series) if facts.kind == "Episode" => format!("{uid}/{series}"),
        _ => format!("{uid}/{}", facts.item_id),
    }
}

/// The one audio track a converted download carries: the remembered one, then the device's
/// language, then the server's default.
pub(crate) fn download_audio(facts: &DownloadFacts, remembered: Option<&TrackChoice>, preferences: &TrackPreferences) -> Option<i64> {
    let tracks: Vec<Track> = facts
        .streams
        .iter()
        .map(|s| Track {
            id: s.index,
            kind: if s.kind == "Audio" { "audio" } else { "sub" }.into(),
            ff_index: Some(s.index),
            title: s.title.clone(),
            lang: s.language.clone(),
            codec: s.codec.clone(),
            channels: s.channels,
            width: None,
            height: None,
            fps: None,
            default: false,
            forced: s.forced,
            external: false,
            external_filename: None,
            hearing_impaired: s.hearing_impaired,
            selected: false,
        })
        .collect();
    select_tracks(&tracks, remembered, facts.default_audio_index, None, &StreamMap::default(), preferences)
        .audio
        .or(facts.default_audio_index)
}

/// Playing a downloaded copy from disk. `converted` is a download the server re-encoded: one
/// video and one audio stream in the file, with every text subtitle saved beside it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn local_plan(
    uid: &str,
    download_id: &str,
    facts: &DownloadFacts,
    file: &str,
    converted: bool,
    subtitles: &[(LocalSubtitle, String)],
    quality: Quality,
    start_seconds: f64,
    preferences: TrackPreferences,
) -> Plan {
    let external_subtitles: Vec<ExternalSubtitle> = subtitles
        .iter()
        .map(|(s, path)| ExternalSubtitle { index: s.index, url: path.clone(), title: s.title.clone(), lang: s.lang.clone() })
        .collect();
    let stream_map = StreamMap {
        external_count: if converted { 0 } else { facts.external_count },
        external: external_subtitles.iter().map(|s| (s.index, s.url.clone())).collect(),
    };
    Plan {
        item_id: facts.item_id.clone(),
        series_id: facts.series_id.clone(),
        media_source_id: facts.media_source_id.clone(),
        play_session_id: uuid::Uuid::new_v4().simple().to_string(),
        method: Method::DirectPlay,
        url: file.to_string(),
        start_seconds,
        now_playing: NowPlaying {
            item_id: facts.item_id.clone(),
            kind: facts.kind.clone(),
            title: facts.title.clone(),
            subtitle: facts.subtitle.clone(),
            overview: facts.overview.clone(),
            method: "Downloaded",
            transcoding: false,
            duration_seconds: facts.runtime_seconds,
            start_seconds,
            quality,
            source: facts.source.clone(),
            downloaded: true,
        },
        scope: download_scope(uid, facts),
        // A converted file holds only the audio it was made with.
        default_audio_index: if converted { None } else { facts.default_audio_index },
        default_subtitle_index: facts
            .default_subtitle_index
            .filter(|i| *i < 0 || !converted || subtitles.iter().any(|(s, _)| s.index == *i)),
        stream_map,
        external_subtitles,
        server_audio: Vec::new(),
        server_subtitles: Vec::new(),
        burned_subtitle: None,
        preferences,
        download: Some(download_id.to_string()),
    }
}

#[tauri::command]
pub async fn play(
    jf: State<'_, Jellyfin>,
    pb: State<'_, Playback>,
    store: State<'_, crate::settings::SettingsStore>,
    dl: State<'_, crate::downloads::Downloads>,
    item_id: String,
    quality: Option<Quality>,
    start_seconds: Option<f64>,
) -> Result<NowPlaying, Error> {
    let mine = pb.next_generation();
    let settings = store.get();
    let preferences = TrackPreferences::from_settings(&settings);
    // A downloaded copy plays from disk, whether or not the server is answering.
    let plan = match dl.playable(&jf, &item_id) {
        Some(local) => {
            let start = match start_seconds.filter(|s| s.is_finite()) {
                Some(seconds) => seconds.max(0.0),
                None => crate::downloads::resume_point(&jf, &local).await,
            };
            local.plan(&jf.user_id()?, start, preferences)
        }
        None => {
            let remembered = |scope: &str| pb.choices().get(scope).cloned();
            let options = PlayOptions {
                quality: quality.unwrap_or(settings.max_quality),
                start_seconds,
                remembered: &remembered,
                direct_play: settings.direct_play,
                preferences,
            };
            prepare_with(&jf, &item_id, &options).await?
        }
    };
    // A file on disk needs no sign-in, and must never be sent the token.
    let headers = if plan.now_playing.downloaded {
        Vec::new()
    } else {
        let (_, authorization) = jf.media_access()?;
        vec![format!("Authorization: {authorization}")]
    };

    start_video().await?;
    let decoder = settings.hwdec.mpv_value();
    let loaded = install_and_load(&pb, mine, &plan, |plan| {
        use_decoder(decoder);
        player_call!(crate::player::load(&plan.url, plan.start_seconds, &headers, None))
    });
    loaded.map(|()| plan.now_playing)
}

/// How long the video output is kept after playback ends (see `Playback::release_video_later`).
/// A development build takes `BLOOM_VIDEO_KEPT_SECS`, to try it without waiting.
fn video_kept() -> Duration {
    const KEPT: Duration = Duration::from_secs(5 * 60);
    #[cfg(debug_assertions)]
    if let Some(secs) = std::env::var("BLOOM_VIDEO_KEPT_SECS").ok().and_then(|s| s.parse().ok()) {
        return Duration::from_secs(secs);
    }
    KEPT
}

/// mpv's video output, made the first time anything plays rather than at launch (see
/// `player::start_video`).
async fn start_video() -> Result<(), Error> {
    #[cfg(target_os = "linux")]
    let started = crate::player::start_video().await.map_err(Error::Player);
    #[cfg(not(target_os = "linux"))]
    let started = Ok(());
    started
}

/// The saved hardware decoder, set as a file is about to load. Setting the same value again for
/// the next file changes nothing.
fn use_decoder(decoder: &str) {
    let set: Result<(), Error> = player_call!(crate::player::set_hwdec(decoder));
    if let Err(e) = set {
        eprintln!("bloom: couldn't set the decoder: {e}");
    }
}

/// Swaps in `plan`'s session and loads its file, unless a newer play or stop was asked for while
/// this one waited on the server (`mine` is its generation): only the latest request plays, and a
/// stop pressed during a slow load stays stopped. Whatever played before is ended with the server.
fn install_and_load(
    pb: &Playback,
    mine: u64,
    plan: &Plan,
    load: impl FnOnce(&Plan) -> Result<Option<i64>, Error>,
) -> Result<(), Error> {
    let _loading = pb.loading();
    let previous = {
        let mut inner = pb.lock();
        if inner.generation != mine {
            return Err(Error::Superseded);
        }
        inner.session.replace(Session::new(plan))
    };
    if let Some(old) = previous {
        pb.end(old, false);
    }
    let loaded = load(plan);
    let mut inner = pb.lock();
    match loaded {
        Ok(entry) => {
            if let Some(s) = inner.session.as_mut() {
                s.entry = entry;
            }
            Ok(())
        }
        Err(e) => {
            inner.session = None;
            Err(e)
        }
    }
}

/// A trailer as the player plays it: a stream from the web, with nothing reported to the server
/// and no tracks, chapters or quality of the item's own.
fn trailer_plan(item_id: &str, title: &str, url: String, duration: Option<f64>) -> Plan {
    Plan {
        item_id: item_id.to_string(),
        series_id: None,
        media_source_id: String::new(),
        play_session_id: String::new(),
        method: Method::DirectStream,
        url,
        start_seconds: 0.0,
        now_playing: NowPlaying {
            item_id: item_id.to_string(),
            kind: "Trailer".into(),
            title: title.to_string(),
            subtitle: Some("Trailer".into()),
            overview: None,
            method: "Trailer",
            transcoding: false,
            duration_seconds: duration,
            start_seconds: 0.0,
            quality: Quality::Original,
            source: None,
            downloaded: false,
        },
        scope: format!("trailer/{item_id}"),
        default_audio_index: None,
        default_subtitle_index: None,
        stream_map: StreamMap::default(),
        external_subtitles: Vec::new(),
        server_audio: Vec::new(),
        server_subtitles: Vec::new(),
        burned_subtitle: None,
        preferences: TrackPreferences::default(),
        download: None,
    }
}

/// Plays an item's trailer number `index` in the player, through yt-dlp. Fails when yt-dlp
/// isn't installed or can't find the streams; the page then opens it in the browser instead.
#[tauri::command]
pub async fn play_trailer(
    jf: State<'_, Jellyfin>,
    pb: State<'_, Playback>,
    store: State<'_, crate::settings::SettingsStore>,
    item_id: String,
    index: usize,
) -> Result<NowPlaying, Error> {
    let mine = pb.next_generation();
    let decoder = store.get().hwdec.mpv_value();
    let (title, page) = crate::library::trailer_with(&jf, &item_id, index).await?;
    let found = crate::trailer::resolve(&page).await.ok_or_else(|| Error::Player("couldn't find this trailer's video".into()))?;
    let plan = trailer_plan(&item_id, &title, found.video.clone(), found.duration);
    start_video().await?;
    let loaded = install_and_load(&pb, mine, &plan, |plan| {
        use_decoder(decoder);
        player_call!(crate::player::load(&plan.url, 0.0, &found.headers, found.audio.as_deref()))
    });
    loaded.map(|()| plan.now_playing)
}

/// Stops playback and waits until the server has heard, so a screen that reloads next already
/// sees the new resume point.
#[tauri::command]
pub async fn playback_stop(pb: State<'_, Playback>) -> Result<(), Error> {
    // A play request still waiting on the server gives way to this stop.
    pb.next_generation();
    let session = {
        let _loading = pb.loading();
        let session = pb.lock().session.take();
        let _: Result<(), Error> = player_call!(crate::player::stop());
        session
    };
    // Speed is picked for one sitting: whatever is watched next starts at normal speed.
    let _: Result<(), Error> = player_call!(crate::player::set_speed(1.0));
    // The next screen may not have a player box; give the picture the whole window back.
    let _: Result<(), Error> = player_call!(crate::player::set_viewport(0.0, 0.0, 0.0, 0.0, 16.0 / 9.0));
    if let Some(s) = session {
        pb.end(s, false);
    }
    pb.app.state::<crate::discord::Presence>().show(None);
    let (done, finished) = oneshot::channel();
    pb.report(Report::Flush(done));
    let _ = tokio::time::timeout(Duration::from_secs(5), finished).await;
    Ok(())
}

#[tauri::command]
pub async fn next_episode(
    jf: State<'_, Jellyfin>,
    dl: State<'_, crate::downloads::Downloads>,
    item_id: String,
) -> Result<Option<UpNext>, Error> {
    match next_episode_after(&jf, &item_id).await {
        Err(Error::SignedOut) => Err(Error::SignedOut),
        // No server: the next episode among the downloads, when it's there.
        Err(e) => dl.next_downloaded(&jf, &item_id).map(Some).ok_or(e),
        found => found,
    }
}

#[tauri::command]
pub async fn playback_toggle_pause() -> Result<(), Error> {
    player_call!(crate::player::toggle_pause())
}

#[tauri::command]
pub async fn playback_seek(seconds: f64, relative: bool) -> Result<(), Error> {
    if !seconds.is_finite() {
        return Err(Error::Status(400));
    }
    player_call!(crate::player::seek(seconds, relative))
}

/// The volume is remembered for the next launch.
#[tauri::command]
pub async fn playback_set_volume(store: State<'_, crate::settings::SettingsStore>, volume: f64) -> Result<(), Error> {
    if !volume.is_finite() {
        return Err(Error::Status(400));
    }
    let volume = (volume.clamp(0.0, 100.0) * 10.0).round() / 10.0;
    let set: Result<(), Error> = player_call!(crate::player::set_volume(volume));
    set?;
    if store.get().volume != volume {
        store.update(|settings| settings.volume = volume);
    }
    Ok(())
}

/// 0.25 to 4; 1 is normal.
#[tauri::command]
pub async fn playback_set_speed(speed: f64) -> Result<(), Error> {
    if !speed.is_finite() {
        return Err(Error::Status(400));
    }
    player_call!(crate::player::set_speed(speed))
}

#[tauri::command]
pub async fn playback_set_muted(muted: bool) -> Result<(), Error> {
    player_call!(crate::player::set_muted(muted))
}

/// `kind` is "audio" or "subtitle"; `id` is a track id from the list, or None to turn it off. A
/// choice made here is remembered for the show or film.
///
/// Resolves to true when the stream has to be asked for again to carry the choice: another audio
/// track during a transcode. The page then restarts playback where it is.
#[tauri::command]
pub async fn playback_set_track(pb: State<'_, Playback>, kind: String, id: Option<i64>) -> Result<bool, Error> {
    let audio = match kind.as_str() {
        "audio" => true,
        "subtitle" => false,
        _ => return Err(Error::Status(400)),
    };
    let transcoded = pb.lock().session.as_ref().filter(|s| !s.server_audio.is_empty()).map(|s| s.audio_track);
    if let (true, Some(current)) = (audio, transcoded) {
        if id.is_none() || id == current {
            return Ok(false);
        }
        pb.remember(true, id);
        return Ok(true);
    }
    // A transcode's image subtitles live in the picture, not in mpv.
    let burning = pb.lock().session.as_ref().filter(|s| !s.server_subtitles.is_empty()).map(|s| s.burned_subtitle);
    if let (false, Some(burned)) = (audio, burning) {
        if subtitle_needs_new_stream(burned, id) {
            pb.remember(false, id);
            return Ok(true);
        }
        if id.is_some_and(|id| id >= SERVER_SUBTITLE_BASE) {
            // Already the one drawn in.
            return Ok(false);
        }
    }
    let set: Result<(), Error> = player_call!(crate::player::set_track(audio, id));
    set?;
    pb.remember(audio, id);
    Ok(false)
}

/// Where the page's player box is, as fractions of the window left free on each side (negative
/// past the window's edge), with the window's width over its height.
/// Async, so it runs off GTK's main thread: each call waits on mpv's core, which is busy while a
/// file loads or the decoder restarts, and the page sends one on every resize.
#[tauri::command]
pub async fn player_set_viewport(left: f64, top: f64, right: f64, bottom: f64, aspect: Option<f64>) -> Result<(), Error> {
    player_call!(crate::player::set_viewport(left, top, right, bottom, aspect.unwrap_or(16.0 / 9.0)))
}

#[tauri::command]
pub async fn player_set_fullscreen(window: tauri::WebviewWindow, fullscreen: bool) -> Result<(), Error> {
    window.set_fullscreen(fullscreen).map_err(|e| Error::Player(e.to_string()))
}

/// Enough about a track to find its equivalent in another file. Episodes of one show often list
/// their tracks in a different order, but keep their languages and names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrackHint {
    lang: Option<String>,
    title: Option<String>,
    codec: Option<String>,
    forced: bool,
    hearing_impaired: bool,
    /// Which of the file's tracks with this same description it was, counting from 0. Two English
    /// PGS subtitles can differ in nothing else, and without it the first was always picked again.
    #[serde(default)]
    nth: usize,
}

impl TrackHint {
    /// `track`, described among the file's `tracks`.
    fn of(track: &Track, tracks: &[Track]) -> Self {
        let mut hint = Self {
            lang: track.lang.clone(),
            title: track.title.clone(),
            codec: track.codec.clone(),
            forced: track.forced,
            hearing_impaired: track.hearing_impaired,
            nth: 0,
        };
        hint.nth = tracks.iter().filter(|t| t.kind == track.kind && t.id < track.id && hint.describes(t)).count();
        hint
    }

    /// The track looks exactly like the one described, its place among look-alikes aside.
    fn describes(&self, t: &Track) -> bool {
        t.lang == self.lang && t.title == self.title && t.codec == self.codec && t.forced == self.forced && t.hearing_impaired == self.hearing_impaired
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SubtitleChoice {
    Off,
    Track(TrackHint),
}

/// The tracks someone picked for a show (all its episodes) or a film.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct TrackChoice {
    audio: Option<TrackHint>,
    subtitle: Option<SubtitleChoice>,
}

/// What to select when a file's tracks arrive. None leaves mpv's own pick in place; for
/// subtitles, Some(None) turns them off.
#[derive(Debug, PartialEq)]
pub(crate) struct Selection {
    audio: Option<i64>,
    subtitle: Option<Option<i64>>,
}

/// The track that best matches a remembered one. It must share the language or the name;
/// among those, matching forced and hearing-impaired flags matters more than the codec, so a
/// "Signs & Songs" track is never taken for full subtitles in the same language.
fn best_match<'a>(tracks: &'a [Track], kind: &str, hint: &TrackHint) -> Option<&'a Track> {
    let scored: Vec<(u32, &Track)> = tracks
        .iter()
        .filter(|t| t.kind == kind)
        .filter_map(|t| {
            let same_lang = hint.lang.is_some() && t.lang == hint.lang;
            let same_title = hint.title.is_some() && t.title == hint.title;
            if !same_lang && !same_title {
                return None;
            }
            let score = 8 * same_lang as u32
                + 4 * same_title as u32
                + 3 * (t.forced == hint.forced) as u32
                + (t.hearing_impaired == hint.hearing_impaired) as u32
                + (t.codec == hint.codec) as u32;
            Some((score, t))
        })
        .collect();
    let best = scored.iter().map(|(score, _)| *score).max()?;
    let mut top: Vec<&Track> = scored.into_iter().filter(|(score, _)| *score == best).map(|(_, t)| t).collect();
    top.sort_by_key(|t| t.id);
    // Where several look exactly like the remembered track, the one in the same place among them;
    // otherwise the first of the best.
    let alike: Vec<&Track> = top.iter().copied().filter(|t| hint.describes(t)).collect();
    alike.get(hint.nth).or_else(|| top.first()).copied()
}

/// A subtitle the server delivers as its own file.
#[derive(Clone, Debug)]
pub(crate) struct ExternalSubtitle {
    /// The server's stream number for it.
    index: i64,
    /// Absolute, with the server's token removed: mpv sends the Authorization header instead.
    pub(crate) url: String,
    title: String,
    lang: String,
}

/// Translates between Jellyfin's stream numbers and mpv's tracks. Jellyfin numbers external
/// streams first, so a file's own streams sit `external_count` above their index in the file.
/// External subtitles are matched by the URL mpv loaded them from.
#[derive(Clone, Debug, Default)]
pub(crate) struct StreamMap {
    external_count: i64,
    external: Vec<(i64, String)>,
}

impl StreamMap {
    fn track_for(&self, tracks: &[Track], kind: &str, index: i64) -> Option<i64> {
        if let Some((_, url)) = self.external.iter().find(|(i, _)| *i == index) {
            return tracks
                .iter()
                .find(|t| t.kind == kind && t.external && t.external_filename.as_deref() == Some(url.as_str()))
                .map(|t| t.id);
        }
        let in_file = index - self.external_count;
        tracks.iter().find(|t| t.kind == kind && !t.external && t.ff_index == Some(in_file)).map(|t| t.id)
    }

    fn index_of(&self, track: &Track) -> Option<i64> {
        if track.external {
            return self.external.iter().find(|(_, url)| Some(url.as_str()) == track.external_filename.as_deref()).map(|(i, _)| *i);
        }
        track.ff_index.map(|i| i + self.external_count)
    }
}

/// The file's audio and subtitle streams as tracks numbered by the server, so a remembered choice
/// can be matched before mpv has opened anything, and a transcode can list the audio it left out.
fn server_tracks(streams: &[MediaStreamInfo], selected_audio: Option<i64>) -> Vec<Track> {
    streams
        .iter()
        .filter_map(|s| {
            let kind = match s.kind.as_str() {
                "Audio" => "audio",
                "Subtitle" => "sub",
                _ => return None,
            };
            Some(Track {
                id: s.index,
                kind: kind.into(),
                ff_index: Some(s.index),
                title: s.title.clone(),
                lang: s.language.clone(),
                codec: s.codec.clone(),
                channels: s.channels,
                width: None,
                height: None,
                fps: None,
                default: false,
                forced: s.is_forced,
                external: false,
                external_filename: None,
                hearing_impaired: s.is_hearing_impaired,
                selected: kind == "audio" && selected_audio == Some(s.index),
            })
        })
        .collect()
}

/// A transcode's image subtitles as tracks. The server delivers them only drawn into the
/// picture ("Encode"), so mpv never sees them; they're listed so they can be chosen, which asks
/// for the stream again. Ids start at `SERVER_SUBTITLE_BASE`.
fn burnable_subtitles(streams: &[MediaStreamInfo], burned: Option<i64>) -> Vec<Track> {
    let encoded = |index: Option<i64>| streams.iter().any(|s| Some(s.index) == index && s.delivery_method.as_deref() == Some("Encode"));
    server_tracks(streams, None)
        .into_iter()
        .filter(|t| t.kind == "sub" && encoded(t.ff_index))
        .map(|mut t| {
            t.selected = burned.is_some() && burned == t.ff_index;
            t.id += SERVER_SUBTITLE_BASE;
            t
        })
        .collect()
}

/// Whether a subtitle choice during a transcode needs the stream asked for again: another image
/// subtitle to draw in, or none (or a text one) while one is drawn into the picture.
fn subtitle_needs_new_stream(burned: Option<i64>, chosen: Option<i64>) -> bool {
    match chosen.filter(|id| *id >= SERVER_SUBTITLE_BASE) {
        Some(id) => burned != Some(id - SERVER_SUBTITLE_BASE),
        None => burned.is_some(),
    }
}

/// A stream number from a URL the server issued, such as a transcode's AudioStreamIndex.
fn query_index(url: &str, key: &str) -> Option<i64> {
    let url = Url::parse(url).ok()?;
    let (_, value) = url.query_pairs().find(|(k, _)| k.eq_ignore_ascii_case(key))?;
    value.parse().ok()
}

/// `path` as the server gave it (relative, with its token in the query), made absolute and
/// token-free.
fn without_token(address: &str, path: &str) -> String {
    let joined = format!("{address}{path}");
    let Ok(mut url) = Url::parse(&joined) else { return joined };
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.eq_ignore_ascii_case("api_key") && !key.eq_ignore_ascii_case("apikey"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
    url.to_string()
}

/// One key per language whichever code form a file or the server uses ("ja" or "jpn", "fr",
/// "fre" or "fra"), for the languages the Settings screen offers. Others compare as written.
fn language_key(code: &str) -> String {
    let code = code.trim().to_ascii_lowercase();
    let key = match code.as_str() {
        "en" | "eng" => "eng",
        "ja" | "jpn" => "jpn",
        "es" | "spa" => "spa",
        "fr" | "fre" | "fra" => "fre",
        "de" | "ger" | "deu" => "ger",
        "it" | "ita" => "ita",
        "pt" | "por" => "por",
        "ko" | "kor" => "kor",
        "zh" | "chi" | "zho" => "chi",
        "ru" | "rus" => "rus",
        _ => return code,
    };
    key.to_string()
}

fn same_language(lang: Option<&str>, wanted: &str) -> bool {
    lang.is_some_and(|lang| language_key(lang) == language_key(wanted))
}

/// A remembered choice wins. Then the device's language defaults from Settings, then the
/// server's defaults from the user's Jellyfin settings.
pub(crate) fn select_tracks(
    tracks: &[Track],
    remembered: Option<&TrackChoice>,
    default_audio: Option<i64>,
    default_subtitle: Option<i64>,
    streams: &StreamMap,
    preferences: &TrackPreferences,
) -> Selection {
    let by_index = |kind: &str, index: i64| streams.track_for(tracks, kind, index);
    let server_subtitle = || match default_subtitle {
        Some(index) if index < 0 => Some(None),
        Some(index) => by_index("sub", index).map(Some),
        None => None,
    };

    let audio = remembered
        .and_then(|c| c.audio.as_ref())
        .and_then(|hint| best_match(tracks, "audio", hint))
        .map(|t| t.id)
        .or_else(|| {
            let wanted = preferences.audio_language.as_deref()?;
            tracks.iter().find(|t| t.kind == "audio" && same_language(t.lang.as_deref(), wanted)).map(|t| t.id)
        })
        .or_else(|| default_audio.and_then(|index| by_index("audio", index)));
    let audio_language = audio.and_then(|id| tracks.iter().find(|t| t.kind == "audio" && t.id == id)).and_then(|t| t.lang.clone());

    let preferred_subtitle = || -> Option<Option<i64>> {
        let wanted = preferences.subtitle_language.as_deref().or(audio_language.as_deref());
        let in_wanted_language = |t: &&Track| wanted.is_some_and(|lang| same_language(t.lang.as_deref(), lang));
        match preferences.subtitle_mode {
            SubtitleMode::Server => server_subtitle(),
            SubtitleMode::Off => Some(None),
            SubtitleMode::Forced => {
                let forced: Vec<&Track> = tracks.iter().filter(|t| t.kind == "sub" && t.forced).collect();
                let pick = forced.iter().find(|t| in_wanted_language(t)).or(forced.first());
                Some(pick.map(|t| t.id))
            }
            SubtitleMode::Always => {
                let full: Vec<&Track> = tracks.iter().filter(|t| t.kind == "sub" && !t.forced).collect();
                let from_server = server_subtitle().flatten().and_then(|id| full.iter().find(|t| t.id == id));
                let pick = full.iter().find(|t| in_wanted_language(t)).or(from_server).or(full.first());
                Some(pick.map(|t| t.id))
            }
        }
    };
    let subtitle = match remembered.and_then(|c| c.subtitle.as_ref()) {
        Some(SubtitleChoice::Off) => Some(None),
        Some(SubtitleChoice::Track(hint)) => best_match(tracks, "sub", hint).map(|t| Some(t.id)).or_else(preferred_subtitle),
        None => preferred_subtitle(),
    };
    Selection { audio, subtitle }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: &str, ff_index: i64, lang: &str, title: Option<&str>, forced: bool) -> Track {
        Track {
            id,
            kind: kind.into(),
            ff_index: Some(ff_index),
            title: title.map(String::from),
            lang: Some(lang.into()),
            codec: Some(if kind == "sub" { "ass" } else { "aac" }.into()),
            channels: None,
            width: None,
            height: None,
            fps: None,
            default: false,
            forced,
            external: false,
            external_filename: None,
            hearing_impaired: false,
            selected: false,
        }
    }

    #[test]
    fn image_subtitles_during_a_transcode_ask_for_the_stream_again() {
        // As the dev server described a PGS file at 720p: every image subtitle "Encode".
        let pgs = |index: i64| MediaStreamInfo {
            index,
            kind: "Subtitle".into(),
            codec: Some("PGSSUB".into()),
            language: Some("eng".into()),
            delivery_method: Some("Encode".into()),
            ..MediaStreamInfo::default()
        };
        let text = MediaStreamInfo {
            index: 5,
            kind: "Subtitle".into(),
            codec: Some("ass".into()),
            delivery_method: Some("External".into()),
            is_text_subtitle_stream: true,
            ..MediaStreamInfo::default()
        };
        let listed = burnable_subtitles(&[pgs(3), pgs(4), text], Some(3));
        assert_eq!(listed.iter().map(|t| t.id).collect::<Vec<_>>(), [SERVER_SUBTITLE_BASE + 3, SERVER_SUBTITLE_BASE + 4]);
        assert!(listed[0].selected && !listed[1].selected);
        assert_eq!(listed[1].ff_index, Some(4), "the server's number is kept for reports and remembering");

        assert!(!subtitle_needs_new_stream(Some(3), Some(SERVER_SUBTITLE_BASE + 3)), "already drawn in");
        assert!(subtitle_needs_new_stream(Some(3), Some(SERVER_SUBTITLE_BASE + 4)));
        assert!(subtitle_needs_new_stream(Some(3), None), "turning it off");
        assert!(subtitle_needs_new_stream(Some(3), Some(2)), "a text subtitle while one is drawn in");
        assert!(subtitle_needs_new_stream(None, Some(SERVER_SUBTITLE_BASE + 4)));
        assert!(!subtitle_needs_new_stream(None, Some(2)));
        assert!(!subtitle_needs_new_stream(None, None));
    }

    /// Files with no external streams, where the server's numbers are the file's own.
    fn choose_tracks(
        tracks: &[Track],
        remembered: Option<&TrackChoice>,
        default_audio: Option<i64>,
        default_subtitle: Option<i64>,
    ) -> Selection {
        select_tracks(tracks, remembered, default_audio, default_subtitle, &StreamMap::default(), &TrackPreferences::default())
    }

    fn preferring(audio: Option<&str>, subtitle_mode: SubtitleMode, subtitle: Option<&str>) -> TrackPreferences {
        TrackPreferences {
            audio_language: audio.map(String::from),
            subtitle_mode,
            subtitle_language: subtitle.map(String::from),
        }
    }

    #[test]
    fn device_defaults_apply_when_nothing_was_picked() {
        let tracks = episode_one();
        let streams = StreamMap::default();
        // Japanese audio asked for as "ja"; the file says "jpn". The server would pick English.
        let prefs = preferring(Some("ja"), SubtitleMode::Always, Some("en"));
        let selection = select_tracks(&tracks, None, Some(2), None, &streams, &prefs);
        assert_eq!(selection, Selection { audio: Some(1), subtitle: Some(Some(2)) }, "full English subtitles, not signs");

        // Forced only: the signs track, in the audio's language when none is set.
        let mut english = tracks.clone();
        english.iter_mut().filter(|t| t.kind == "sub").for_each(|t| t.lang = Some("eng".into()));
        let forced = select_tracks(&english, None, None, None, &streams, &preferring(Some("eng"), SubtitleMode::Forced, None));
        assert_eq!(forced, Selection { audio: Some(2), subtitle: Some(Some(1)) });

        let off = select_tracks(&tracks, None, None, Some(4), &streams, &preferring(None, SubtitleMode::Off, None));
        assert_eq!(off.subtitle, Some(None));
        // A language the file lacks falls back to the server.
        let missing = select_tracks(&tracks, None, Some(2), None, &streams, &preferring(Some("de"), SubtitleMode::Server, None));
        assert_eq!(missing.audio, Some(2));
    }

    #[test]
    fn a_remembered_choice_beats_the_device_defaults() {
        let prefs = preferring(Some("eng"), SubtitleMode::Off, None);
        let selection =
            select_tracks(&episode_one(), Some(&picked_japanese_with_full_subtitles()), None, None, &StreamMap::default(), &prefs);
        assert_eq!(selection, Selection { audio: Some(1), subtitle: Some(Some(2)) });
    }

    /// One episode: Japanese and English audio; signs-only, full English and Spanish subtitles.
    fn episode_one() -> Vec<Track> {
        vec![
            track(1, "video", 0, "jpn", None, false),
            track(1, "audio", 1, "jpn", None, false),
            track(2, "audio", 2, "eng", None, false),
            track(1, "sub", 3, "eng", Some("Signs & Songs"), true),
            track(2, "sub", 4, "eng", Some("Full Subtitles"), false),
            track(3, "sub", 5, "spa", None, false),
        ]
    }

    fn picked_japanese_with_full_subtitles() -> TrackChoice {
        let tracks = episode_one();
        TrackChoice {
            audio: Some(TrackHint::of(&tracks[1], &tracks)),
            subtitle: Some(SubtitleChoice::Track(TrackHint::of(&tracks[4], &tracks))),
        }
    }

    #[test]
    fn two_subtitles_that_look_the_same_are_told_apart_by_their_place() {
        let tracks = vec![
            track(1, "audio", 1, "jpn", None, false),
            track(1, "sub", 2, "eng", None, false),
            track(2, "sub", 3, "eng", None, false),
        ];
        let second = TrackChoice { audio: None, subtitle: Some(SubtitleChoice::Track(TrackHint::of(&tracks[2], &tracks))) };
        let wanted = select_tracks(&tracks, Some(&second), None, None, &StreamMap::default(), &TrackPreferences::default());
        assert_eq!(wanted.subtitle, Some(Some(2)), "the first of the two was picked again");
        // An episode with only one such track still gets it.
        let one = vec![track(1, "audio", 1, "jpn", None, false), track(1, "sub", 2, "eng", None, false)];
        let wanted = select_tracks(&one, Some(&second), None, None, &StreamMap::default(), &TrackPreferences::default());
        assert_eq!(wanted.subtitle, Some(Some(1)));
    }

    #[test]
    fn remembered_choice_carries_to_an_episode_with_tracks_in_another_order() {
        let next_episode = vec![
            track(1, "video", 0, "jpn", None, false),
            track(1, "audio", 1, "eng", None, false),
            track(2, "audio", 2, "jpn", None, false),
            track(1, "sub", 3, "eng", Some("Full Subtitles"), false),
            track(2, "sub", 4, "eng", Some("Signs & Songs"), true),
        ];
        let selection = choose_tracks(&next_episode, Some(&picked_japanese_with_full_subtitles()), None, None);
        assert_eq!(selection, Selection { audio: Some(2), subtitle: Some(Some(1)) });
    }

    #[test]
    fn signs_track_is_not_taken_for_full_subtitles_when_names_differ() {
        // Same language, and this file names the full track differently.
        let next_episode = vec![
            track(1, "audio", 1, "jpn", None, false),
            track(1, "sub", 2, "eng", Some("Signs"), true),
            track(2, "sub", 3, "eng", Some("Dialogue"), false),
        ];
        let selection = choose_tracks(&next_episode, Some(&picked_japanese_with_full_subtitles()), None, None);
        assert_eq!(selection.subtitle, Some(Some(2)));
    }

    #[test]
    fn subtitles_turned_off_stay_off() {
        let choice = TrackChoice { audio: None, subtitle: Some(SubtitleChoice::Off) };
        assert_eq!(choose_tracks(&episode_one(), Some(&choice), None, Some(4)).subtitle, Some(None));
    }

    #[test]
    fn without_a_choice_the_servers_defaults_apply_by_stream_index() {
        let selection = choose_tracks(&episode_one(), None, Some(2), Some(4));
        assert_eq!(selection, Selection { audio: Some(2), subtitle: Some(Some(2)) });
        assert_eq!(choose_tracks(&episode_one(), None, None, Some(-1)).subtitle, Some(None));
        // An index mpv doesn't have (an external subtitle on the server) leaves mpv's pick.
        assert_eq!(choose_tracks(&episode_one(), None, None, Some(9)).subtitle, None);
    }

    #[test]
    fn a_language_the_file_lacks_falls_back_to_the_server() {
        let mut choice = picked_japanese_with_full_subtitles();
        if let Some(hint) = choice.audio.as_mut() {
            hint.lang = Some("fra".into());
        }
        assert_eq!(choose_tracks(&episode_one(), Some(&choice), Some(1), None).audio, Some(1));
        assert_eq!(choose_tracks(&episode_one(), Some(&choice), None, None).audio, None);
    }

    #[test]
    fn choices_survive_a_round_trip_through_the_file_format() {
        let mut all = HashMap::new();
        all.insert("user/show".to_string(), picked_japanese_with_full_subtitles());
        all.insert("user/film".to_string(), TrackChoice { audio: None, subtitle: Some(SubtitleChoice::Off) });
        let back: HashMap<String, TrackChoice> = serde_json::from_slice(&serde_json::to_vec(&all).unwrap()).unwrap();
        assert_eq!(back["user/show"].subtitle, all["user/show"].subtitle);
        assert_eq!(back["user/film"].subtitle, Some(SubtitleChoice::Off));
    }

    #[test]
    fn server_numbers_count_external_subtitles_first() {
        // As the server numbered Demon Slayer: Infinity Castle: #0 an external .srt, #1 video,
        // #2 Spanish and #3 Japanese audio, #4 an embedded Spanish ASS.
        let url = "http://server/Videos/x/Subtitles/0/0/Stream.subrip".to_string();
        let mut srt = track(3, "sub", 0, "eng", Some("External file"), false);
        srt.external = true;
        srt.external_filename = Some(url.clone());
        let tracks = vec![
            track(1, "video", 0, "jpn", None, false),
            track(1, "audio", 1, "spa", None, false),
            track(2, "audio", 2, "jpn", None, false),
            track(1, "sub", 3, "spa", None, false),
            srt,
        ];
        let streams = StreamMap { external_count: 1, external: vec![(0, url)] };
        let selection = select_tracks(&tracks, None, Some(3), Some(0), &streams, &TrackPreferences::default());
        assert_eq!(selection, Selection { audio: Some(2), subtitle: Some(Some(3)) });
        assert_eq!(streams.index_of(&tracks[3]), Some(4));
        assert_eq!(streams.index_of(&tracks[4]), Some(0));
    }

    #[test]
    fn subtitle_urls_lose_the_servers_token() {
        let url = without_token("http://server:8096", "/Videos/a/b/Subtitles/0/0/Stream.subrip?ApiKey=secret&x=1");
        assert_eq!(url, "http://server:8096/Videos/a/b/Subtitles/0/0/Stream.subrip?x=1");
        assert_eq!(without_token("http://server", "/y?api_key=secret"), "http://server/y");
    }

    #[test]
    fn quality_limits_reach_the_device_profile() {
        let original = device_profile(Quality::Original);
        assert_eq!(original["MaxStreamingBitrate"], MAX_BITRATE);
        assert_eq!(original["CodecProfiles"].as_array().map(Vec::len), Some(0));
        let hd720 = device_profile(Quality::Hd720);
        assert_eq!(hd720["MaxStreamingBitrate"], 6_000_000);
        assert_eq!(hd720["CodecProfiles"][0]["Conditions"][0]["Value"], "1280");
        assert_eq!(hd720["CodecProfiles"][0]["Conditions"][1]["Value"], "720");
        assert_eq!(serde_json::from_str::<Quality>("\"1080p\"").unwrap(), Quality::Hd1080);
    }

    #[test]
    fn remembered_choice_is_found_among_the_servers_streams() {
        // As the server lists Charlotte: English and Japanese audio, signs and full subtitles.
        let stream = |index: i64, kind: &str, lang: &str, title: Option<&str>| MediaStreamInfo {
            index,
            kind: kind.into(),
            language: Some(lang.into()),
            title: title.map(String::from),
            codec: Some(if kind == "Subtitle" { "ass" } else { "opus" }.into()),
            ..Default::default()
        };
        let streams = vec![
            MediaStreamInfo { index: 0, kind: "Video".into(), ..Default::default() },
            stream(1, "Audio", "eng", None),
            stream(2, "Audio", "jpn", None),
            stream(3, "Subtitle", "eng", Some("S&S")),
            stream(4, "Subtitle", "eng", Some("Dialog - ENG")),
        ];
        let choice = TrackChoice {
            audio: Some(TrackHint {
                lang: Some("jpn".into()),
                title: None,
                codec: Some("opus".into()),
                forced: false,
                hearing_impaired: false,
                nth: 0,
            }),
            subtitle: Some(SubtitleChoice::Track(TrackHint {
                lang: Some("eng".into()),
                title: Some("Dialog - ENG".into()),
                codec: Some("ass".into()),
                forced: false,
                hearing_impaired: false,
                nth: 0,
            })),
        };
        let tracks = server_tracks(&streams, Some(2));
        assert_eq!(tracks.len(), 4, "the video stream isn't a choice");
        assert!(tracks.iter().find(|t| t.id == 2).is_some_and(|t| t.selected));
        let wanted = select_tracks(&tracks, Some(&choice), None, None, &StreamMap::default(), &TrackPreferences::default());
        assert_eq!(wanted, Selection { audio: Some(2), subtitle: Some(Some(4)) });
    }

    #[test]
    fn transcode_urls_name_their_audio() {
        let url = "http://server/videos/x/master.m3u8?DeviceId=d&AudioStreamIndex=2&VideoBitrate=5809812";
        assert_eq!(query_index(url, "AudioStreamIndex"), Some(2));
        assert_eq!(query_index(url, "SubtitleStreamIndex"), None);
    }
}
