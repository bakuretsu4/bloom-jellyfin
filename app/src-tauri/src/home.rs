//! The home screen: libraries, Continue watching, Next up, and Recently added per library.
//!
//! Rust shapes the server's items into cards so the page never needs Jellyfin's DTOs, and
//! picks each card's artwork here, where the fallbacks between image types are easier to read.

use crate::jellyfin::{Error, Jellyfin};
use futures_util::future::{join_all, try_join4};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

const FIELDS: &str = "fields=PrimaryImageAspectRatio&enableImageTypes=Primary,Backdrop,Thumb&imageTypeLimit=1";
const RAIL_LIMIT: u32 = 16;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ItemsPage {
    #[serde(default)]
    pub(crate) items: Vec<Item>,
    /// The size of the whole result, when a page was asked for.
    #[serde(default)]
    pub(crate) total_record_count: u32,
}

/// Also what the library grids and search results are built from (see library.rs), so a card
/// looks the same wherever it appears.
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub(crate) struct Item {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "Type")]
    pub(crate) kind: String,
    pub(crate) collection_type: Option<String>,
    pub(crate) production_year: Option<i32>,
    pub(crate) index_number: Option<i32>,
    pub(crate) parent_index_number: Option<i32>,
    pub(crate) series_id: Option<String>,
    series_name: Option<String>,
    series_primary_image_tag: Option<String>,
    album_artist: Option<String>,
    image_tags: Option<HashMap<String, String>>,
    backdrop_image_tags: Option<Vec<String>>,
    parent_thumb_item_id: Option<String>,
    parent_thumb_image_tag: Option<String>,
    parent_backdrop_item_id: Option<String>,
    parent_backdrop_image_tags: Option<Vec<String>>,
    user_data: Option<UserData>,
    pub(crate) overview: Option<String>,
    /// ISO 8601; the first ten characters are the date.
    pub(crate) premiere_date: Option<String>,
    genres: Option<Vec<String>>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    pub(crate) run_time_ticks: Option<i64>,
    child_count: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct UserData {
    played_percentage: Option<f64>,
    played: bool,
    is_favorite: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Me {
    configuration: UserConfig,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct UserConfig {
    /// Libraries the user turned off under "Latest media" in their Jellyfin home settings.
    latest_items_excludes: Vec<String>,
}

/// Enough to fetch one piece of artwork through `bloom-img://`. Tags change whenever the
/// artwork does, which is what lets the cache keep files forever.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub(crate) item_id: String,
    pub(crate) kind: &'static str,
    pub(crate) tag: String,
}

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Poster,
    Wide,
    Square,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    id: String,
    kind: String,
    title: String,
    meta: Option<String>,
    pub(crate) image: Option<Image>,
    /// 0..1 when partly watched.
    progress: Option<f64>,
    /// For an episode: its show, whose page the card's text opens.
    series_id: Option<String>,
}

impl Card {
    /// A card made from something other than a server item: a download, with no server.
    pub(crate) fn new(id: String, kind: String, title: String, meta: Option<String>, image: Option<Image>, progress: Option<f64>) -> Self {
        Self { id, kind, title, meta, image, progress, series_id: None }
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub(crate) id: String,
    pub(crate) title: String,
    shape: Shape,
    pub(crate) cards: Vec<Card>,
}

impl Item {
    fn own(&self, kind: &'static str) -> Option<Image> {
        let tag = match kind {
            "Backdrop" => self.backdrop_image_tags.as_ref()?.first()?.clone(),
            _ => self.image_tags.as_ref()?.get(kind)?.clone(),
        };
        Some(Image { item_id: self.id.clone(), kind, tag })
    }

    fn series_poster(&self) -> Option<Image> {
        Some(Image { item_id: self.series_id.clone()?, kind: "Primary", tag: self.series_primary_image_tag.clone()? })
    }

    fn parent_thumb(&self) -> Option<Image> {
        Some(Image { item_id: self.parent_thumb_item_id.clone()?, kind: "Thumb", tag: self.parent_thumb_image_tag.clone()? })
    }

    fn parent_backdrop(&self) -> Option<Image> {
        Some(Image {
            item_id: self.parent_backdrop_item_id.clone()?,
            kind: "Backdrop",
            tag: self.parent_backdrop_image_tags.as_ref()?.first()?.clone(),
        })
    }

    fn is_episode(&self) -> bool {
        self.kind == "Episode"
    }

    /// 0..1 when partly watched.
    pub(crate) fn progress(&self) -> Option<f64> {
        self.user_data
            .as_ref()
            .filter(|u| !u.played)
            .and_then(|u| u.played_percentage)
            .filter(|p| *p > 0.0)
            .map(|p| (p / 100.0).min(1.0))
    }

    pub(crate) fn played(&self) -> bool {
        self.user_data.as_ref().is_some_and(|u| u.played)
    }

    pub(crate) fn image(&self, shape: Shape) -> Option<Image> {
        match shape {
            // A new episode in "Recently added" stands for its show.
            Shape::Poster if self.is_episode() => self.series_poster().or_else(|| self.own("Primary")),
            Shape::Poster | Shape::Square => self.own("Primary"),
            // An episode's own still first, then the show's landscape art.
            Shape::Wide if self.is_episode() => {
                self.own("Primary").or_else(|| self.parent_thumb()).or_else(|| self.parent_backdrop())
            }
            // Library tiles: the server renders a landscape Primary for each library.
            Shape::Wide if matches!(self.kind.as_str(), "CollectionFolder" | "UserView") => self.own("Primary"),
            // A film's Primary is its portrait poster, so it is the last resort for a wide card.
            Shape::Wide => self
                .own("Thumb")
                .or_else(|| self.own("Backdrop"))
                .or_else(|| self.parent_thumb())
                .or_else(|| self.parent_backdrop())
                .or_else(|| self.own("Primary")),
        }
    }

    pub(crate) fn card(self, shape: Shape) -> Card {
        let image = self.image(shape);
        let progress = self.progress();

        let (title, meta) = if self.is_episode() {
            let code = match (self.parent_index_number, self.index_number) {
                (Some(s), Some(e)) => Some(format!("S{s} E{e}")),
                (None, Some(e)) => Some(format!("E{e}")),
                _ => None,
            };
            let meta = match (shape, code) {
                (Shape::Wide, Some(c)) => Some(format!("{c}, {}", self.name)),
                (Shape::Wide, None) => Some(self.name.clone()),
                (_, c) => c,
            };
            (self.series_name.clone().unwrap_or_else(|| self.name.clone()), meta)
        } else if self.kind == "MusicAlbum" {
            (self.name.clone(), self.album_artist.clone())
        } else if self.kind == "BoxSet" {
            let count = self.child_count.filter(|n| *n > 0).map(|n| format!("{n} {}", if n == 1 { "title" } else { "titles" }));
            (self.name.clone(), count)
        } else {
            (self.name.clone(), self.production_year.map(|y| y.to_string()))
        };

        let series_id = self.series_id.clone().filter(|_| self.is_episode());
        Card { id: self.id, kind: self.kind, title, meta, image, progress, series_id }
    }
}

/// Overviews sometimes arrive as HTML from a metadata provider ("<br>", "<i>", "&amp;"). The page
/// shows text, so line-breaking tags become line breaks, other tags go, common entities decode,
/// blank lines collapse to one, and nothing left means no overview.
pub(crate) fn plain_text(text: Option<String>) -> Option<String> {
    let text = text?;
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        // Only a letter or a slash starts a tag; "a < b" and "<3" are text.
        let (true, Some(len)) = (after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/'), after.find('>')) else {
            out.push('<');
            rest = after;
            continue;
        };
        let name = after[..len].trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        if matches!(name.to_ascii_lowercase().as_str(), "br" | "p" | "div" | "li") {
            out.push('\n');
        }
        rest = &after[len + 1..];
    }
    out.push_str(rest);
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    let decoded = strip_markdown(&decoded);
    let mut result = String::with_capacity(decoded.len());
    let mut blank = false;
    for line in decoded.lines().map(str::trim) {
        if line.is_empty() {
            if blank || result.is_empty() {
                continue;
            }
            blank = true;
        } else {
            blank = false;
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
    }
    let result = result.trim_end().to_string();
    (!result.is_empty()).then_some(result)
}

/// Markdown some metadata providers write into overviews (AniDB's character links, bold notes):
/// a `[text](http…)` link keeps its text, and `**` markers go. Brackets that aren't a link to a
/// web address are left as written.
fn strip_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|close| {
            let (label, tail) = (&after[..close], &after[close + 2..]);
            let end = tail.find(')')?;
            let url = &tail[..end];
            let is_link = !label.is_empty()
                && !label.contains(['[', ']', '\n'])
                && (url.starts_with("http://") || url.starts_with("https://"))
                && !url.contains(char::is_whitespace);
            is_link.then_some((label, &tail[end + 1..]))
        });
        match link {
            Some((label, remaining)) => {
                out.push_str(&rest[..open]);
                out.push_str(label);
                rest = remaining;
            }
            None => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "")
}

fn latest_shape(collection_type: &str) -> Option<Shape> {
    match collection_type {
        "movies" | "tvshows" => Some(Shape::Poster),
        "music" => Some(Shape::Square),
        "homevideos" | "musicvideos" => Some(Shape::Wide),
        // Playlists, collections, Live TV, books and photos get their own screens later.
        _ => None,
    }
}

#[tauri::command]
pub async fn home(jf: State<'_, Jellyfin>) -> Result<Vec<Section>, Error> {
    home_with(&jf).await
}

pub async fn home_with(jf: &Jellyfin) -> Result<Vec<Section>, Error> {
    let uid = jf.user_id()?;
    let (views, resume, next_up, me) = try_join4(
        jf.get_json::<ItemsPage>(format!("/UserViews?userId={uid}")),
        jf.get_json::<ItemsPage>(format!("/UserItems/Resume?userId={uid}&limit={RAIL_LIMIT}&mediaTypes=Video&{FIELDS}")),
        // Resumable episodes are already in Continue watching.
        jf.get_json::<ItemsPage>(format!("/Shows/NextUp?userId={uid}&limit={RAIL_LIMIT}&enableResumable=false&{FIELDS}")),
        jf.get_json::<Me>("/Users/Me".into()),
    )
    .await?;

    // /UserViews already follows the order the user set in Jellyfin.
    let latest_targets: Vec<(String, String, Shape)> = views
        .items
        .iter()
        .filter(|v| !me.configuration.latest_items_excludes.contains(&v.id))
        .filter_map(|v| Some((v.id.clone(), v.name.clone(), latest_shape(v.collection_type.as_deref()?)?)))
        .collect();
    let latest = join_all(latest_targets.iter().map(|(id, _, _)| {
        jf.get_json::<Vec<Item>>(format!("/Items/Latest?userId={uid}&parentId={id}&limit={RAIL_LIMIT}&{FIELDS}"))
    }))
    .await;

    let mut sections = Vec::new();
    let mut push = |id: String, title: String, shape: Shape, items: Vec<Item>| {
        if !items.is_empty() {
            let cards = items.into_iter().map(|i| i.card(shape)).collect();
            sections.push(Section { id, title, shape, cards });
        }
    };
    push("libraries".into(), "Libraries".into(), Shape::Wide, views.items);
    push("resume".into(), "Continue watching".into(), Shape::Wide, resume.items);
    push("next-up".into(), "Next up".into(), Shape::Wide, next_up.items);
    for ((id, name, shape), result) in latest_targets.into_iter().zip(latest) {
        match result {
            Ok(items) => push(format!("latest-{id}"), format!("Recently added in {name}"), shape, items),
            Err(Error::SignedOut) => return Err(Error::SignedOut),
            // One library failing shouldn't blank the whole home screen.
            Err(_) => {}
        }
    }
    Ok(sections)
}

/// One slide of the spotlight at the top of the home screen.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spotlight {
    pub(crate) id: String,
    kind: String,
    title: String,
    overview: Option<String>,
    backdrop: Image,
    pub(crate) logo: Option<Image>,
    year: Option<i32>,
    official_rating: Option<String>,
    /// Out of 10, to one decimal.
    community_rating: Option<f64>,
    genres: Vec<String>,
    /// Films only; an episode's length says little about a show.
    runtime_minutes: Option<u32>,
    seasons: Option<u32>,
    pub(crate) favorite: bool,
}

const SPOTLIGHT_COUNT: u32 = 6;

#[tauri::command]
pub async fn spotlight(jf: State<'_, Jellyfin>) -> Result<Vec<Spotlight>, Error> {
    spotlight_with(&jf).await
}

/// A random handful of films and shows that have a backdrop and a synopsis, so every slide can
/// look finished. A new draw on every visit to Home.
pub async fn spotlight_with(jf: &Jellyfin) -> Result<Vec<Spotlight>, Error> {
    let uid = jf.user_id()?;
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&includeItemTypes=Movie,Series&recursive=true&sortBy=Random\
             &limit={SPOTLIGHT_COUNT}&imageTypes=Backdrop&hasOverview=true\
             &fields=Overview,Genres,ChildCount&enableImageTypes=Backdrop,Logo&imageTypeLimit=1&enableUserData=true"
        ))
        .await?;

    Ok(page
        .items
        .into_iter()
        .filter_map(|i| {
            let backdrop = i.own("Backdrop")?;
            let logo = i.own("Logo");
            let is_series = i.kind == "Series";
            let genres = i.genres.clone().unwrap_or_default().iter().take(3).map(|g| capitalise(g)).collect();
            Some(Spotlight {
                backdrop,
                logo,
                year: i.production_year,
                official_rating: i.official_rating.clone(),
                community_rating: i.community_rating.filter(|r| *r > 0.0).map(|r| (r * 10.0).round() / 10.0),
                genres,
                runtime_minutes: (!is_series)
                    .then_some(i.run_time_ticks)
                    .flatten()
                    .map(|t| (t / 600_000_000) as u32)
                    .filter(|m| *m > 0),
                seasons: is_series.then_some(i.child_count).flatten().filter(|c| *c > 0),
                favorite: i.user_data.as_ref().is_some_and(|u| u.is_favorite),
                overview: plain_text(i.overview),
                title: i.name,
                kind: i.kind,
                id: i.id,
            })
        })
        .collect())
}

/// Some libraries tag genres in lower case ("fantasy", "magic").
pub(crate) fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|first| first.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

#[tauri::command]
pub async fn set_favorite(jf: State<'_, Jellyfin>, item_id: String, favorite: bool) -> Result<bool, Error> {
    set_favorite_with(&jf, item_id, favorite).await
}

/// Returns whether the item is now a favourite, as the server reports it.
pub async fn set_favorite_with(jf: &Jellyfin, item_id: String, favorite: bool) -> Result<bool, Error> {
    // The id goes into a URL path; refuse anything that could change which endpoint is hit.
    if item_id.is_empty() || !item_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Status(400));
    }
    let uid = jf.user_id()?;
    let method = if favorite { reqwest::Method::POST } else { reqwest::Method::DELETE };
    let res = jf.request(method, &format!("/UserFavoriteItems/{item_id}?userId={uid}")).await?;
    let data: UserData = res.json().await.map_err(|_| Error::Unreadable)?;
    Ok(data.is_favorite)
}
