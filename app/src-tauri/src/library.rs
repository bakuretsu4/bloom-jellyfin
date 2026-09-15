//! The data behind the navigation shell's pages: the list of libraries, a library's grid,
//! search results, and the series and film pages with their seasons and episodes.
//!
//! Grids reuse Home's cards (home.rs), so artwork and labels read the same everywhere.

use crate::home::{capitalise, plain_text, Card, Image, Item, ItemsPage, Shape};
use crate::jellyfin::{Error, Jellyfin};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

/// Cards per page in a library grid or search results. Mirrored in `src/lib/api.ts`.
pub(crate) const GRID_PAGE: u32 = 60;
/// Episodes read to find a show's audio and subtitle languages.
const LANGUAGE_SAMPLE: u32 = 24;
/// Titles in an item page's More like this rail.
const SIMILAR_LIMIT: u32 = 16;
/// Some shows credit dozens of people; a page lists the first ones the server gives.
const PEOPLE_LIMIT: usize = 40;
const GRID_FIELDS: &str = "fields=PrimaryImageAspectRatio,ChildCount&enableImageTypes=Primary&imageTypeLimit=1&enableUserData=true";
/// Collections looked through for the ones a title is in: a server with more is unusual.
const COLLECTIONS_LIMIT: u32 = 200;
const TICKS_PER_MINUTE: i64 = 600_000_000;
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Ids go into URL paths; refuse anything that could change which endpoint is hit.
fn check_id(id: &str) -> Result<(), Error> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Err(Error::Status(400))
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid {
    pub(crate) total: u32,
    shape: Shape,
    pub(crate) cards: Vec<Card>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct DetailUserData {
    played: bool,
    played_percentage: Option<f64>,
    playback_position_ticks: i64,
    is_favorite: bool,
    unplayed_item_count: Option<u32>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct NameOnly {
    name: String,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct PersonDto {
    id: String,
    name: String,
    role: Option<String>,
    #[serde(rename = "Type")]
    kind: Option<String>,
    primary_image_tag: Option<String>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct StreamDto {
    #[serde(rename = "Type")]
    kind: String,
    display_title: Option<String>,
    language: Option<String>,
}

/// The distinct languages among streams of one kind ("Audio", "Subtitle"), in the order met.
fn languages(streams: &[StreamDto], kind: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for lang in streams.iter().filter(|s| s.kind == kind).filter_map(|s| s.language.as_deref()) {
        if !lang.is_empty() && lang != "und" && !found.iter().any(|f| f == lang) {
            found.push(lang.to_string());
        }
    }
    found
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct SourceDto {
    container: Option<String>,
    size: Option<i64>,
    bitrate: Option<i64>,
    media_streams: Option<Vec<StreamDto>>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct ChapterDto {
    name: Option<String>,
    start_position_ticks: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct DetailDto {
    id: String,
    name: String,
    #[serde(rename = "Type")]
    kind: String,
    production_year: Option<i32>,
    end_date: Option<String>,
    status: Option<String>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    genres: Option<Vec<String>>,
    studios: Option<Vec<NameOnly>>,
    taglines: Option<Vec<String>>,
    overview: Option<String>,
    image_tags: Option<HashMap<String, String>>,
    backdrop_image_tags: Option<Vec<String>>,
    user_data: Option<DetailUserData>,
    run_time_ticks: Option<i64>,
    recursive_item_count: Option<u32>,
    people: Option<Vec<PersonDto>>,
    media_sources: Option<Vec<SourceDto>>,
    chapters: Option<Vec<ChapterDto>>,
    series_id: Option<String>,
    series_name: Option<String>,
    series_primary_image_tag: Option<String>,
    season_id: Option<String>,
    season_name: Option<String>,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    premiere_date: Option<String>,
    child_count: Option<u32>,
    remote_trailers: Option<Vec<RemoteTrailerDto>>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteTrailerDto {
    url: Option<String>,
    name: Option<String>,
}

/// A trailer on the web (almost always YouTube), as the server's metadata lists it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrailerLink {
    name: Option<String>,
    url: String,
}

/// The item's web trailers: https addresses only, each once.
fn web_trailers(dto: &DetailDto) -> Vec<TrailerLink> {
    let mut found: Vec<TrailerLink> = Vec::new();
    for trailer in dto.remote_trailers.clone().unwrap_or_default() {
        let Some(url) = trailer.url.filter(|u| url::Url::parse(u).is_ok_and(|parsed| parsed.scheme() == "https")) else { continue };
        if !found.iter().any(|t| t.url == url) {
            found.push(TrailerLink { name: trailer.name.filter(|n| !n.trim().is_empty()), url });
        }
    }
    found
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct SeasonDto {
    id: String,
    name: String,
    index_number: Option<i32>,
    child_count: Option<u32>,
    user_data: Option<DetailUserData>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct SeasonsPage {
    items: Vec<SeasonDto>,
}

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct EpisodeDto {
    id: String,
    name: String,
    index_number: Option<i32>,
    parent_index_number: Option<i32>,
    season_id: Option<String>,
    overview: Option<String>,
    run_time_ticks: Option<i64>,
    image_tags: Option<HashMap<String, String>>,
    user_data: Option<DetailUserData>,
    premiere_date: Option<String>,
    media_streams: Option<Vec<StreamDto>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct EpisodesPage {
    items: Vec<EpisodeDto>,
    total_record_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Season {
    pub(crate) id: String,
    pub(crate) name: String,
    number: Option<i32>,
    episode_count: u32,
    unplayed: u32,
}

/// What a page's Play button starts: the next episode of a show, or the film itself.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayTarget {
    item_id: String,
    label: String,
    resume: bool,
    /// For an episode: the season to open the episode list at.
    season_id: Option<String>,
    episode_number: Option<i32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    id: String,
    name: String,
    /// The character for an actor, or the job ("Director") for anyone else.
    role: Option<String>,
    image: Option<Image>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    container: Option<String>,
    size_bytes: Option<i64>,
    bitrate: Option<i64>,
    video: Vec<String>,
    audio: Vec<String>,
    subtitles: Vec<String>,
}

/// Everything a series or film page shows above and in its tabs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    id: String,
    kind: String,
    title: String,
    year: Option<i32>,
    end_year: Option<i32>,
    status: Option<String>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    genres: Vec<String>,
    studios: Vec<String>,
    tagline: Option<String>,
    overview: Option<String>,
    backdrop: Option<Image>,
    poster: Option<Image>,
    /// The title as artwork, where the server has one.
    logo: Option<Image>,
    /// Language codes: a film's own streams, or those of a show's first episodes.
    pub(crate) audio_languages: Vec<String>,
    pub(crate) subtitle_languages: Vec<String>,
    favorite: bool,
    pub(crate) played: bool,
    runtime_minutes: Option<u32>,
    episode_count: Option<u32>,
    unplayed_count: Option<u32>,
    pub(crate) seasons: Vec<Season>,
    pub(crate) play: Option<PlayTarget>,
    pub(crate) people: Vec<Person>,
    pub(crate) media: Option<MediaInfo>,
    /// Where each chapter starts, in the file's order.
    pub(crate) chapters: Vec<Chapter>,
    /// For an episode: its show, which the watch screen links back to.
    series: Option<SeriesLink>,
    season_id: Option<String>,
    season_name: Option<String>,
    /// Web trailers, which play in Bloom's player with yt-dlp or open in the browser.
    trailers: Vec<TrailerLink>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    name: Option<String>,
    pub(crate) start_seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesLink {
    id: String,
    title: String,
    poster: Option<Image>,
}

/// The list beside the player: the episodes after this one, or films like this one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub(crate) heading: &'static str,
    pub(crate) entries: Vec<QueueEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    pub(crate) id: String,
    title: String,
    /// "S1 E4" for an episode, the year for a film.
    meta: Option<String>,
    image: Option<Image>,
    runtime_minutes: Option<u32>,
    progress: Option<f64>,
    played: bool,
    overview: Option<String>,
    /// "2026-04-03".
    premiere_date: Option<String>,
}

/// The watch screen's episode list: one season in full, then the start of the next, so the
/// step into a new season shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonList {
    /// Every season with episodes, for the season picker.
    pub(crate) seasons: Vec<Season>,
    pub(crate) season_id: String,
    pub(crate) entries: Vec<QueueEntry>,
    pub(crate) next: Option<NextSeason>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextSeason {
    season_id: String,
    name: String,
    pub(crate) entries: Vec<QueueEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub(crate) id: String,
    number: Option<i32>,
    title: String,
    overview: Option<String>,
    runtime_minutes: Option<u32>,
    image: Option<Image>,
    played: bool,
    /// 0..1 when partly watched.
    progress: Option<f64>,
    /// "2026-04-03".
    premiere_date: Option<String>,
    /// Language codes of the audio tracks.
    audio_languages: Vec<String>,
    has_subtitles: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpisodePage {
    pub(crate) total: u32,
    pub(crate) episodes: Vec<Episode>,
}

/// "42:10", or "1:02:03" past an hour.
fn clock(ticks: i64) -> String {
    let total = ticks / TICKS_PER_SECOND;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub(crate) async fn libraries_with(jf: &Jellyfin) -> Result<Vec<Card>, Error> {
    let uid = jf.user_id()?;
    let views: ItemsPage = jf.get_json(format!("/UserViews?userId={uid}")).await?;
    Ok(views.items.into_iter().map(|v| v.card(Shape::Wide)).collect())
}

/// A page of a library, sorted by "added", "name", "year" or "rating". `only` narrows it to
/// those items (the downloaded ones).
pub(crate) async fn library_items_with(
    jf: &Jellyfin,
    library_id: &str,
    sort: &str,
    start: u32,
    only: Option<&[String]>,
) -> Result<Grid, Error> {
    check_id(library_id)?;
    for id in only.unwrap_or_default() {
        check_id(id)?;
    }
    let uid = jf.user_id()?;
    let library: Item = jf.get_json(format!("/Items/{library_id}?userId={uid}")).await?;
    let (types, shape) = match library.collection_type.as_deref() {
        Some("movies") => ("Movie", Shape::Poster),
        Some("tvshows") => ("Series", Shape::Poster),
        Some("music") => ("MusicAlbum", Shape::Square),
        Some("homevideos") => ("Video", Shape::Wide),
        Some("musicvideos") => ("MusicVideo", Shape::Wide),
        Some("boxsets") => ("BoxSet", Shape::Poster),
        _ => ("Movie,Series,Video", Shape::Poster),
    };
    let (sort_by, order) = match sort {
        "name" => ("SortName", "Ascending"),
        "year" => ("ProductionYear,SortName", "Descending"),
        "rating" => ("CommunityRating,SortName", "Descending"),
        _ => ("DateCreated,SortName", "Descending"),
    };
    let ids = match only {
        // An empty id list would read as no filter at all.
        Some([]) => return Ok(Grid { total: 0, shape, cards: Vec::new() }),
        Some(ids) => format!("&ids={}", ids.join(",")),
        None => String::new(),
    };
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&parentId={library_id}&recursive=true&includeItemTypes={types}\
             &sortBy={sort_by}&sortOrder={order}&startIndex={start}&limit={GRID_PAGE}&{GRID_FIELDS}{ids}"
        ))
        .await?;
    Ok(Grid { total: page.total_record_count, shape, cards: page.items.into_iter().map(|i| i.card(shape)).collect() })
}

/// Films, shows and episodes whose names match, at most `limit` (and never more than a grid
/// page). Fewer than two characters matches nothing.
pub(crate) async fn search_with(jf: &Jellyfin, query: &str, limit: u32) -> Result<Vec<Card>, Error> {
    let limit = limit.clamp(1, GRID_PAGE);
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let uid = jf.user_id()?;
    let term: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&searchTerm={term}&includeItemTypes=Movie,Series,Episode&recursive=true\
             &limit={limit}&{GRID_FIELDS}"
        ))
        .await?;
    Ok(page.items.into_iter().map(|i| i.card(Shape::Poster)).collect())
}

/// The next episode up for a show, or its first episode once everything has been watched.
async fn series_play_target(jf: &Jellyfin, uid: &str, series_id: &str) -> Result<Option<PlayTarget>, Error> {
    let next: EpisodesPage = jf
        .get_json(format!(
            "/Shows/NextUp?userId={uid}&seriesId={series_id}&limit=1&enableResumable=true&disableFirstEpisode=false"
        ))
        .await?;
    let episode = match next.items.into_iter().next() {
        Some(episode) => Some(episode),
        None => {
            let first: EpisodesPage =
                jf.get_json(format!("/Shows/{series_id}/Episodes?userId={uid}&limit=1&isMissing=false")).await?;
            first.items.into_iter().next()
        }
    };
    Ok(episode.map(|e| {
        let user = e.user_data.clone().unwrap_or_default();
        let resume = !user.played && user.playback_position_ticks > 0;
        let code = match (e.parent_index_number, e.index_number) {
            (Some(s), Some(n)) => format!("S{s} E{n}"),
            (None, Some(n)) => format!("E{n}"),
            _ => e.name.clone(),
        };
        PlayTarget {
            label: format!("{} {code}", if resume { "Resume" } else { "Play" }),
            resume,
            season_id: e.season_id.clone(),
            episode_number: e.index_number,
            item_id: e.id,
        }
    }))
}

pub(crate) async fn item_detail_with(jf: &Jellyfin, item_id: &str) -> Result<ItemDetail, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let dto: DetailDto = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    let is_series = dto.kind == "Series";
    let user = dto.user_data.clone().unwrap_or_default();
    let image = |kind: &'static str| -> Option<Image> {
        let tag = match kind {
            "Backdrop" => dto.backdrop_image_tags.as_ref()?.first()?.clone(),
            _ => dto.image_tags.as_ref()?.get(kind)?.clone(),
        };
        Some(Image { item_id: dto.id.clone(), kind, tag })
    };

    let (seasons, play) = if is_series {
        (show_seasons(jf, &uid, item_id).await?, series_play_target(jf, &uid, item_id).await?)
    } else {
        let resume = !user.played && user.playback_position_ticks > 0;
        let target = matches!(dto.kind.as_str(), "Movie" | "Video" | "MusicVideo").then(|| PlayTarget {
            item_id: dto.id.clone(),
            label: if resume { format!("Resume from {}", clock(user.playback_position_ticks)) } else { "Play".into() },
            resume,
            season_id: None,
            episode_number: None,
        });
        (Vec::new(), target)
    };

    let people = dto
        .people
        .clone()
        .unwrap_or_default()
        .into_iter()
        .take(PEOPLE_LIMIT)
        .map(|p| Person {
            image: p.primary_image_tag.map(|tag| Image { item_id: p.id.clone(), kind: "Primary", tag }),
            role: p.role.filter(|r| !r.trim().is_empty()).or(p.kind.filter(|k| k.as_str() != "Actor")),
            id: p.id,
            name: p.name,
        })
        .collect();

    let media = if is_series {
        None
    } else {
        dto.media_sources.clone().unwrap_or_default().into_iter().next().map(|source| {
            let streams = source.media_streams.unwrap_or_default();
            let titles = |kind: &str| streams.iter().filter(|s| s.kind == kind).filter_map(|s| s.display_title.clone()).collect();
            MediaInfo {
                container: source.container.map(|c| c.to_uppercase()),
                size_bytes: source.size,
                bitrate: source.bitrate,
                video: titles("Video"),
                audio: titles("Audio"),
                subtitles: titles("Subtitle"),
            }
        })
    };

    let (audio_languages, subtitle_languages) = if is_series {
        let sample: EpisodesPage = jf
            .get_json(format!(
                "/Shows/{item_id}/Episodes?userId={uid}&limit={LANGUAGE_SAMPLE}&isMissing=false&fields=MediaStreams"
            ))
            .await?;
        let streams: Vec<StreamDto> = sample.items.into_iter().flat_map(|e| e.media_streams.unwrap_or_default()).collect();
        (languages(&streams, "Audio"), languages(&streams, "Subtitle"))
    } else {
        let streams = dto.media_sources.as_ref().and_then(|s| s.first()).and_then(|s| s.media_streams.clone()).unwrap_or_default();
        (languages(&streams, "Audio"), languages(&streams, "Subtitle"))
    };

    let runtime_seconds = dto.run_time_ticks.map(|t| t as f64 / TICKS_PER_SECOND as f64);
    let chapters = dto
        .chapters
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|c| Chapter {
            name: c.name.filter(|n| !n.trim().is_empty()),
            start_seconds: c.start_position_ticks.max(0) as f64 / TICKS_PER_SECOND as f64,
        })
        // Some rips carry a trailing chapter at or past the end of the file.
        .filter(|c| runtime_seconds.is_none_or(|end| c.start_seconds < end))
        .collect();
    let series = match (dto.kind.as_str(), dto.series_id.clone()) {
        ("Episode", Some(id)) => Some(SeriesLink {
            poster: dto.series_primary_image_tag.clone().map(|tag| Image { item_id: id.clone(), kind: "Primary", tag }),
            title: dto.series_name.clone().unwrap_or_default(),
            id,
        }),
        _ => None,
    };

    Ok(ItemDetail {
        chapters,
        series,
        season_id: dto.season_id.clone(),
        season_name: dto.season_name.clone(),
        trailers: web_trailers(&dto),
        backdrop: image("Backdrop"),
        logo: image("Logo"),
        audio_languages,
        subtitle_languages,
        poster: image("Primary"),
        year: dto.production_year,
        end_year: dto.end_date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        status: dto.status.clone(),
        official_rating: dto.official_rating.clone(),
        community_rating: dto.community_rating.filter(|r| *r > 0.0).map(|r| (r * 10.0).round() / 10.0),
        genres: dto.genres.clone().unwrap_or_default().iter().take(4).map(|g| capitalise(g)).collect(),
        studios: dto.studios.clone().unwrap_or_default().into_iter().map(|s| s.name).collect(),
        tagline: dto.taglines.clone().and_then(|t| t.into_iter().next()),
        overview: plain_text(dto.overview.clone()),
        favorite: user.is_favorite,
        played: user.played,
        runtime_minutes: if is_series { None } else { dto.run_time_ticks.map(|t| (t / TICKS_PER_MINUTE) as u32).filter(|m| *m > 0) },
        episode_count: if is_series { dto.recursive_item_count } else { None },
        unplayed_count: if is_series { user.unplayed_item_count } else { None },
        seasons,
        play,
        people,
        media,
        title: dto.name.clone(),
        kind: dto.kind.clone(),
        id: dto.id.clone(),
    })
}

/// A collection a title is in, with its films (or shows) in the order they came out.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRow {
    id: String,
    title: String,
    cards: Vec<Card>,
}

/// A collection's titles, oldest first.
async fn collection_children(jf: &Jellyfin, uid: &str, collection_id: &str) -> Result<Vec<Item>, Error> {
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&parentId={collection_id}&sortBy=ProductionYear,SortName&sortOrder=Ascending&{GRID_FIELDS}"
        ))
        .await?;
    Ok(page.items)
}

pub(crate) async fn collection_items_with(jf: &Jellyfin, collection_id: &str) -> Result<Vec<Card>, Error> {
    check_id(collection_id)?;
    let uid = jf.user_id()?;
    Ok(collection_children(jf, &uid, collection_id).await?.into_iter().map(|i| i.card(Shape::Poster)).collect())
}

/// The collections a title is in. Jellyfin's items don't say, and a title's ancestors don't
/// include its collections, so the collections are looked through, all at once.
pub(crate) async fn item_collections_with(jf: &Jellyfin, item_id: &str) -> Result<Vec<CollectionRow>, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let sets: ItemsPage = jf
        .get_json(format!("/Items?userId={uid}&recursive=true&includeItemTypes=BoxSet&sortBy=SortName&limit={COLLECTIONS_LIMIT}"))
        .await?;
    let contents = join_all(sets.items.iter().map(|set| collection_children(jf, &uid, &set.id))).await;
    let mut rows = Vec::new();
    for (set, children) in sets.items.into_iter().zip(contents) {
        let children = match children {
            Ok(children) => children,
            Err(Error::SignedOut) => return Err(Error::SignedOut),
            // One collection failing to load shouldn't hide the others.
            Err(_) => continue,
        };
        if children.iter().any(|child| child.id == item_id) {
            rows.push(CollectionRow {
                id: set.id,
                title: set.name,
                cards: children.into_iter().map(|child| child.card(Shape::Poster)).collect(),
            });
        }
    }
    Ok(rows)
}

#[tauri::command]
pub async fn item_collections(jf: State<'_, Jellyfin>, item_id: String) -> Result<Vec<CollectionRow>, Error> {
    item_collections_with(&jf, &item_id).await
}

#[tauri::command]
pub async fn collection_items(jf: State<'_, Jellyfin>, collection_id: String) -> Result<Vec<Card>, Error> {
    collection_items_with(&jf, &collection_id).await
}

/// An item's trailer number `index`, as (the item's title, the trailer's address). The page
/// names a trailer by its number, so it can only ever reach an address the server listed.
pub(crate) async fn trailer_with(jf: &Jellyfin, item_id: &str, index: usize) -> Result<(String, String), Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let dto: DetailDto = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    let trailer = web_trailers(&dto).into_iter().nth(index).ok_or(Error::Status(404))?;
    Ok((dto.name, trailer.url))
}

/// A whole season, in order.
pub(crate) async fn season_episodes_with(jf: &Jellyfin, series_id: &str, season_id: &str) -> Result<EpisodePage, Error> {
    check_id(series_id)?;
    check_id(season_id)?;
    let uid = jf.user_id()?;
    let page: EpisodesPage = jf
        .get_json(format!(
            "/Shows/{series_id}/Episodes?userId={uid}&seasonId={season_id}&isMissing=false\
             &fields=Overview,MediaStreams&enableImageTypes=Primary&imageTypeLimit=1"
        ))
        .await?;
    let episodes = page
        .items
        .into_iter()
        .map(|e| {
            let user = e.user_data.clone().unwrap_or_default();
            let streams = e.media_streams.clone().unwrap_or_default();
            Episode {
                premiere_date: e.premiere_date.as_deref().and_then(|d| d.get(..10)).map(String::from),
                audio_languages: languages(&streams, "Audio"),
                has_subtitles: streams.iter().any(|s| s.kind == "Subtitle"),
                image: e.image_tags.as_ref().and_then(|tags| tags.get("Primary")).map(|tag| Image {
                    item_id: e.id.clone(),
                    kind: "Primary",
                    tag: tag.clone(),
                }),
                number: e.index_number,
                runtime_minutes: e.run_time_ticks.map(|t| (t / TICKS_PER_MINUTE) as u32).filter(|m| *m > 0),
                played: user.played,
                progress: if user.played { None } else { user.played_percentage.filter(|p| *p > 0.0).map(|p| (p / 100.0).min(1.0)) },
                overview: plain_text(e.overview),
                title: e.name,
                id: e.id,
            }
        })
        .collect();
    Ok(EpisodePage { total: page.total_record_count, episodes })
}

/// Returns whether the item is now marked watched, as the server reports it.
pub(crate) async fn set_played_with(jf: &Jellyfin, item_id: &str, played: bool) -> Result<bool, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let method = if played { reqwest::Method::POST } else { reqwest::Method::DELETE };
    let res = jf.request(method, &format!("/UserPlayedItems/{item_id}?userId={uid}")).await?;
    let data: DetailUserData = res.json().await.map_err(|_| Error::Unreadable)?;
    Ok(data.played)
}

/// Entries in the watch screen's list.
const QUEUE_LIMIT: usize = 12;

/// How many of the next season's episodes follow a season in the watch screen's list.
const NEXT_SEASON_PREVIEW: u32 = 3;

/// Films the server finds similar to this one, for the list beside the player.
pub(crate) async fn watch_queue_with(jf: &Jellyfin, item_id: &str) -> Result<Queue, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items/{item_id}/Similar?userId={uid}&limit={QUEUE_LIMIT}&fields=PrimaryImageAspectRatio,Overview"
        ))
        .await?;
    let entries = page.items.into_iter().filter(|i| i.kind == "Movie").map(queue_entry).collect();
    Ok(Queue { heading: "More like this", entries })
}

/// A show's seasons that have episodes in the library.
async fn show_seasons(jf: &Jellyfin, uid: &str, series_id: &str) -> Result<Vec<Season>, Error> {
    let page: SeasonsPage = jf.get_json(format!("/Shows/{series_id}/Seasons?userId={uid}&fields=ChildCount")).await?;
    Ok(page
        .items
        .into_iter()
        // Empty seasons are placeholders for episodes the library doesn't have.
        .filter(|s| s.child_count.unwrap_or(0) > 0)
        .map(|s| Season {
            number: s.index_number,
            episode_count: s.child_count.unwrap_or(0),
            unplayed: s.user_data.and_then(|u| u.unplayed_item_count).unwrap_or(0),
            name: s.name,
            id: s.id,
        })
        .collect())
}

/// The season in the show's list that `season_id` is. A server can keep a second item for a
/// season, which its episodes point to but the show's list doesn't carry (Dexter's Season 1 on
/// the dev server, with the same episodes under both): that one is found by its number.
fn listed_season<'a>(seasons: &'a [Season], season_id: &'a str, number: Option<i32>) -> &'a str {
    if seasons.iter().any(|s| s.id == season_id) {
        return season_id;
    }
    match seasons.iter().find(|s| number.is_some() && s.number == number) {
        Some(season) => &season.id,
        None => season_id,
    }
}

/// One season of a show in full, with the first episodes of the season after it.
pub(crate) async fn season_list_with(jf: &Jellyfin, series_id: &str, season_id: &str) -> Result<SeasonList, Error> {
    check_id(series_id)?;
    check_id(season_id)?;
    let uid = jf.user_id()?;
    let episodes = |season: &str, limit: Option<u32>| {
        let limit = limit.map(|n| format!("&limit={n}")).unwrap_or_default();
        format!(
            "/Shows/{series_id}/Episodes?userId={uid}&seasonId={season}&isMissing=false{limit}&fields=Overview\
             &enableImageTypes=Primary,Thumb,Backdrop&imageTypeLimit=1&enableUserData=true"
        )
    };
    let seasons = show_seasons(jf, &uid, series_id).await?;
    let page: ItemsPage = jf.get_json(episodes(season_id, None)).await?;
    let season_id = listed_season(&seasons, season_id, page.items.first().and_then(|e| e.parent_index_number)).to_string();
    let next = match seasons.iter().position(|s| s.id == season_id).and_then(|i| seasons.get(i + 1)) {
        Some(season) => {
            let page: ItemsPage = jf.get_json(episodes(&season.id, Some(NEXT_SEASON_PREVIEW))).await?;
            Some(NextSeason {
                season_id: season.id.clone(),
                name: season.name.clone(),
                entries: page.items.into_iter().map(queue_entry).collect(),
            })
        }
        None => None,
    };
    Ok(SeasonList {
        seasons,
        season_id,
        entries: page.items.into_iter().map(queue_entry).collect(),
        next,
    })
}

#[tauri::command]
pub async fn season_list(jf: State<'_, Jellyfin>, series_id: String, season_id: String) -> Result<SeasonList, Error> {
    season_list_with(&jf, &series_id, &season_id).await
}

fn queue_entry(item: Item) -> QueueEntry {
    let meta = if item.kind == "Episode" {
        match (item.parent_index_number, item.index_number) {
            (Some(s), Some(e)) => Some(format!("S{s} E{e}")),
            (None, Some(e)) => Some(format!("E{e}")),
            _ => None,
        }
    } else {
        item.production_year.map(|y| y.to_string())
    };
    QueueEntry {
        image: item.image(Shape::Wide),
        overview: plain_text(item.overview.clone()),
        premiere_date: item.premiere_date.as_deref().and_then(|d| d.get(..10)).map(String::from),
        progress: item.progress(),
        played: item.played(),
        runtime_minutes: item.run_time_ticks.map(|t| (t / TICKS_PER_MINUTE) as u32).filter(|m| *m > 0),
        meta,
        title: item.name,
        id: item.id,
    }
}

#[tauri::command]
pub async fn watch_queue(jf: State<'_, Jellyfin>, item_id: String) -> Result<Queue, Error> {
    watch_queue_with(&jf, &item_id).await
}

#[tauri::command]
pub async fn libraries(jf: State<'_, Jellyfin>) -> Result<Vec<Card>, Error> {
    libraries_with(&jf).await
}

#[tauri::command]
pub async fn library_items(
    jf: State<'_, Jellyfin>,
    library_id: String,
    sort: String,
    start: u32,
    ids: Option<Vec<String>>,
) -> Result<Grid, Error> {
    library_items_with(&jf, &library_id, &sort, start, ids.as_deref()).await
}

/// `limit` defaults to a grid page; the top bar's suggestions ask for fewer. With no server, the
/// downloaded films and shows that match.
#[tauri::command]
pub async fn search(
    jf: State<'_, Jellyfin>,
    dl: State<'_, crate::downloads::Downloads>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<Card>, Error> {
    let limit = limit.unwrap_or(GRID_PAGE);
    match search_with(&jf, &query, limit).await {
        Err(Error::SignedOut) => Err(Error::SignedOut),
        Err(e) => {
            let found = dl.search_titles(&jf, &query, limit);
            if found.is_empty() {
                Err(e)
            } else {
                Ok(found)
            }
        }
        found => found,
    }
}

/// A stretch of a file the player offers to skip.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// "intro", "recap", "credits" or "preview".
    pub(crate) kind: &'static str,
    pub(crate) start_seconds: f64,
    pub(crate) end_seconds: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct SegmentDto {
    #[serde(rename = "Type")]
    kind: String,
    start_ticks: i64,
    end_ticks: i64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct SegmentsPage {
    items: Vec<SegmentDto>,
}

/// Shorter than this isn't worth a button: it would be gone before it could be pressed.
const MIN_SEGMENT_SECONDS: f64 = 5.0;
/// A chapter called "Intro" longer than this is part of the episode, not its opening.
const MAX_INTRO_SECONDS: f64 = 240.0;

/// Where a file's intro, recap, credits and preview are. The server's media segments (from its
/// intro and credits detection) come first; a file without them falls back on chapter names.
pub(crate) async fn skip_segments_with(jf: &Jellyfin, item_id: &str) -> Result<Vec<Segment>, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let from_server: Result<SegmentsPage, Error> = jf.get_json(format!("/MediaSegments/{item_id}")).await;
    let mut segments: Vec<Segment> = match from_server {
        Ok(page) => page
            .items
            .into_iter()
            .filter_map(|s| {
                let kind = match s.kind.as_str() {
                    "Intro" => "intro",
                    "Recap" => "recap",
                    "Outro" => "credits",
                    "Preview" => "preview",
                    _ => return None,
                };
                let (start, end) = (s.start_ticks.max(0) as f64 / TICKS_PER_SECOND as f64, s.end_ticks as f64 / TICKS_PER_SECOND as f64);
                (end - start >= MIN_SEGMENT_SECONDS).then_some(Segment { kind, start_seconds: start, end_seconds: end })
            })
            .collect(),
        Err(e @ Error::SignedOut) => return Err(e),
        // Servers before Jellyfin 10.10 have no segments at all; that's the same as none here.
        Err(_) => Vec::new(),
    };
    if segments.is_empty() {
        let dto: DetailDto = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
        let runtime = dto.run_time_ticks.map(|t| t as f64 / TICKS_PER_SECOND as f64);
        segments = segments_from_chapters(&dto.chapters.unwrap_or_default(), runtime);
    }
    segments.sort_by(|a, b| a.start_seconds.total_cmp(&b.start_seconds));
    Ok(segments)
}

/// Skippable stretches guessed from chapter names. Rips name them inconsistently: "OP" and
/// "Opening" are always the opening, but "Intro" is the opening in some ("Intro > Scene 1 >
/// Credits") and a cold open before it in others ("Intro > OP > Part A"), so it only counts
/// where there's no opening chapter, and only if it's short.
fn segments_from_chapters(chapters: &[ChapterDto], runtime: Option<f64>) -> Vec<Segment> {
    const OPENING: [&str; 4] = ["op", "opening", "opening credits", "opening song"];
    const CREDITS: [&str; 6] = ["ed", "ending", "credits", "end credits", "closing credits", "outro"];
    const PREVIEW: [&str; 2] = ["preview", "next episode preview"];

    let name = |c: &ChapterDto| c.name.as_deref().unwrap_or("").trim().to_ascii_lowercase();
    let find = |names: &[&str]| chapters.iter().position(|c| names.contains(&name(c).as_str()));
    let span = |i: usize| -> Option<(f64, f64)> {
        let start = chapters[i].start_position_ticks.max(0) as f64 / TICKS_PER_SECOND as f64;
        let end = chapters.get(i + 1).map(|c| c.start_position_ticks as f64 / TICKS_PER_SECOND as f64).or(runtime)?;
        (end - start >= MIN_SEGMENT_SECONDS).then_some((start, end))
    };

    let mut segments = Vec::new();
    let intro = find(&OPENING[..])
        .and_then(span)
        .or_else(|| find(&["intro"]).and_then(span).filter(|(start, end)| end - start <= MAX_INTRO_SECONDS));
    if let Some((start, end)) = intro {
        segments.push(Segment { kind: "intro", start_seconds: start, end_seconds: end });
    }
    let credits = chapters.iter().rposition(|c| CREDITS.contains(&name(c).as_str())).and_then(span);
    if let Some((start, end)) = credits {
        segments.push(Segment { kind: "credits", start_seconds: start, end_seconds: end });
    }
    if let Some((start, end)) = find(&PREVIEW[..]).and_then(span) {
        segments.push(Segment { kind: "preview", start_seconds: start, end_seconds: end });
    }
    segments
}

/// With no server, a downloaded item's segments as they were saved.
#[tauri::command]
pub async fn skip_segments(
    jf: State<'_, Jellyfin>,
    dl: State<'_, crate::downloads::Downloads>,
    item_id: String,
) -> Result<serde_json::Value, Error> {
    match skip_segments_with(&jf, &item_id).await {
        Ok(segments) => serde_json::to_value(segments).map_err(|_| Error::Unreadable),
        Err(Error::SignedOut) => Err(Error::SignedOut),
        Err(e) => dl.saved_segments(&jf, &item_id).ok_or(e),
    }
}

/// With no server, a downloaded item's page as it was saved.
#[tauri::command]
pub async fn item_detail(
    jf: State<'_, Jellyfin>,
    dl: State<'_, crate::downloads::Downloads>,
    item_id: String,
) -> Result<serde_json::Value, Error> {
    match item_detail_with(&jf, &item_id).await {
        Ok(detail) => serde_json::to_value(detail).map_err(|_| Error::Unreadable),
        Err(Error::SignedOut) => Err(Error::SignedOut),
        Err(e) => dl.saved_detail(&jf, &item_id).ok_or(e),
    }
}

/// With no server, the season's downloaded episodes.
#[tauri::command]
pub async fn season_episodes(
    jf: State<'_, Jellyfin>,
    dl: State<'_, crate::downloads::Downloads>,
    series_id: String,
    season_id: String,
) -> Result<serde_json::Value, Error> {
    match season_episodes_with(&jf, &series_id, &season_id).await {
        Ok(page) => serde_json::to_value(page).map_err(|_| Error::Unreadable),
        Err(Error::SignedOut) => Err(Error::SignedOut),
        Err(e) => dl.saved_episodes(&jf, &series_id, &season_id).ok_or(e),
    }
}

/// Films and shows the server finds similar, for the rail at the bottom of an item's page.
pub(crate) async fn similar_with(jf: &Jellyfin, item_id: &str) -> Result<Vec<Card>, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let page: ItemsPage = jf
        .get_json(format!("/Items/{item_id}/Similar?userId={uid}&limit={SIMILAR_LIMIT}&fields=PrimaryImageAspectRatio"))
        .await?;
    Ok(page.items.into_iter().filter(|i| matches!(i.kind.as_str(), "Movie" | "Series")).map(|i| i.card(Shape::Poster)).collect())
}

#[tauri::command]
pub async fn similar(jf: State<'_, Jellyfin>, item_id: String) -> Result<Vec<Card>, Error> {
    similar_with(&jf, &item_id).await
}

/// What a hover card shows for any title: a film, a show or an episode.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDetail {
    id: String,
    pub(crate) kind: String,
    title: String,
    /// For an episode: its show, and "S1 E4".
    series_title: Option<String>,
    code: Option<String>,
    overview: Option<String>,
    /// "2026-04-03".
    premiere_date: Option<String>,
    year: Option<i32>,
    end_year: Option<i32>,
    status: Option<String>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    runtime_minutes: Option<u32>,
    season_count: Option<u32>,
    pub(crate) episode_count: Option<u32>,
    favorite: bool,
    played: bool,
    /// Partly watched, so Play picks up where it stopped.
    resume: bool,
}

/// A hover card's detail, in one request.
pub(crate) async fn card_detail_with(jf: &Jellyfin, item_id: &str) -> Result<CardDetail, Error> {
    check_id(item_id)?;
    let uid = jf.user_id()?;
    let dto: DetailDto = jf.get_json(format!("/Items/{item_id}?userId={uid}")).await?;
    let user = dto.user_data.clone().unwrap_or_default();
    let is_series = dto.kind == "Series";
    let is_episode = dto.kind == "Episode";
    let code = match (is_episode, dto.parent_index_number, dto.index_number) {
        (true, Some(s), Some(e)) => Some(format!("S{s} E{e}")),
        (true, None, Some(e)) => Some(format!("E{e}")),
        _ => None,
    };
    Ok(CardDetail {
        series_title: if is_episode { dto.series_name.clone() } else { None },
        code,
        overview: plain_text(dto.overview.clone()),
        premiere_date: dto.premiere_date.as_deref().and_then(|d| d.get(..10)).map(String::from),
        year: dto.production_year,
        end_year: dto.end_date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        status: dto.status.clone(),
        official_rating: dto.official_rating.clone(),
        community_rating: dto.community_rating.filter(|r| *r > 0.0).map(|r| (r * 10.0).round() / 10.0),
        runtime_minutes: if is_series { None } else { dto.run_time_ticks.map(|t| (t / TICKS_PER_MINUTE) as u32).filter(|m| *m > 0) },
        season_count: if is_series { dto.child_count } else { None },
        episode_count: if is_series { dto.recursive_item_count } else { None },
        favorite: user.is_favorite,
        played: user.played,
        resume: !user.played && user.playback_position_ticks > 0,
        title: dto.name,
        kind: dto.kind,
        id: dto.id,
    })
}

#[tauri::command]
pub async fn card_detail(jf: State<'_, Jellyfin>, item_id: String) -> Result<CardDetail, Error> {
    card_detail_with(&jf, &item_id).await
}

#[tauri::command]
pub async fn set_played(jf: State<'_, Jellyfin>, item_id: String, played: bool) -> Result<bool, Error> {
    set_played_with(&jf, &item_id, played).await
}

#[cfg(test)]
mod tests {
    use super::{clock, languages, listed_season, segments_from_chapters, ChapterDto, Season, Segment, StreamDto};
    use crate::home::plain_text;

    #[test]
    fn a_season_under_another_id_is_found_by_its_number() {
        let season = |id: &str, number: i32| Season { id: id.into(), name: format!("Season {number}"), number: Some(number), episode_count: 12, unplayed: 0 };
        let seasons = [season("listed1", 1), season("listed2", 2)];
        assert_eq!(listed_season(&seasons, "listed2", Some(2)), "listed2");
        // As Dexter's Season 1 on the dev server: its episodes name a season the show's list doesn't.
        assert_eq!(listed_season(&seasons, "stale1", Some(1)), "listed1");
        assert_eq!(listed_season(&seasons, "stale9", Some(9)), "stale9", "no season with that number");
        assert_eq!(listed_season(&seasons, "stale", None), "stale");
    }

    fn chapters(list: &[(&str, i64)]) -> Vec<ChapterDto> {
        list.iter().map(|(name, s)| ChapterDto { name: Some(name.to_string()), start_position_ticks: s * 10_000_000 }).collect()
    }

    #[test]
    fn an_opening_chapter_beats_a_cold_open_called_intro() {
        let list = chapters(&[("Intro", 0), ("OP", 90), ("Part A", 180), ("Part B", 700), ("ED", 1300), ("Preview", 1390)]);
        assert_eq!(
            segments_from_chapters(&list, Some(1440.0)),
            vec![
                Segment { kind: "intro", start_seconds: 90.0, end_seconds: 180.0 },
                Segment { kind: "credits", start_seconds: 1300.0, end_seconds: 1390.0 },
                Segment { kind: "preview", start_seconds: 1390.0, end_seconds: 1440.0 },
            ]
        );
    }

    #[test]
    fn intro_counts_when_its_short_and_theres_no_opening() {
        let list = chapters(&[("Intro", 0), ("Scene 1", 60), ("Credits", 2500)]);
        let found = segments_from_chapters(&list, Some(2600.0));
        assert_eq!(found[0], Segment { kind: "intro", start_seconds: 0.0, end_seconds: 60.0 });
        assert_eq!(found[1], Segment { kind: "credits", start_seconds: 2500.0, end_seconds: 2600.0 });
        // A ten-minute "Intro" is part of the film.
        assert!(segments_from_chapters(&chapters(&[("Intro", 0), ("Chapter 2", 600)]), Some(5400.0)).is_empty());
        assert!(segments_from_chapters(&chapters(&[("Chapter 1", 0), ("Chapter 2", 300)]), Some(900.0)).is_empty());
    }

    #[test]
    fn overviews_lose_their_html() {
        let text = |s: &str| plain_text(Some(s.to_string()));
        assert_eq!(
            text("Based on the manga by Uda Nozomi.<br>This <i>surreal</i> comedy &amp; more."),
            Some("Based on the manga by Uda Nozomi.\nThis surreal comedy & more.".into())
        );
        assert_eq!(text("<p>One.</p><p>Two.</p><br/><br />"), Some("One.\n\nTwo.".into()));
        assert_eq!(text("Hearts <3 and a < b"), Some("Hearts <3 and a < b".into()));
        assert_eq!(text("<br>  "), None);
        assert_eq!(plain_text(None), None);
    }

    #[test]
    fn overviews_lose_anidb_markdown() {
        let text = |s: &str| plain_text(Some(s.to_string())).unwrap();
        assert_eq!(
            text("[Kaneki Ken](http://anidb.net/ch72229) meets [ghoul](http://anidb.net/t7110)s and [Eto](https://anidb.net/ch98561)'s owl."),
            "Kaneki Ken meets ghouls and Eto's owl."
        );
        assert_eq!(text("**Note:** The first episode aired early."), "Note: The first episode aired early.");
        // Brackets that aren't links stay as written.
        assert_eq!(text("[Season 2] starts [here](not a link) and [x]"), "[Season 2] starts [here](not a link) and [x]");
        assert_eq!(text("[a] b [c](http://anidb.net/c)"), "[a] b c");
    }

    #[test]
    fn languages_are_distinct_and_in_order() {
        let stream = |kind: &str, lang: Option<&str>| StreamDto {
            kind: kind.into(),
            language: lang.map(String::from),
            ..Default::default()
        };
        let streams = vec![
            stream("Audio", Some("jpn")),
            stream("Subtitle", Some("eng")),
            stream("Audio", Some("eng")),
            stream("Audio", Some("jpn")),
            stream("Audio", Some("und")),
            stream("Audio", None),
        ];
        assert_eq!(languages(&streams, "Audio"), ["jpn", "eng"]);
        assert_eq!(languages(&streams, "Subtitle"), ["eng"]);
    }

    #[test]
    fn resume_times() {
        assert_eq!(clock(42 * 60 * 10_000_000 + 10 * 10_000_000), "42:10");
        assert_eq!(clock((3600 + 2 * 60 + 3) * 10_000_000), "1:02:03");
    }
}
