//! Albums, artists and songs: what their pages show.

use crate::home::{plain_text, Card, Image, Item, ItemsPage, Shape};
use crate::jellyfin::{Error, Jellyfin};
use serde::Serialize;

const TICKS_PER_SECOND: f64 = 10_000_000.0;
/// The songs shown on an artist's page.
const ARTIST_SONGS: u32 = 10;
/// More albums than this are cut off on an artist's page.
const ARTIST_ALBUMS: u32 = 200;
const FIELDS: &str = "fields=PrimaryImageAspectRatio,ChildCount,Genres&enableImageTypes=Primary&imageTypeLimit=1&enableUserData=true";

fn check_id(id: &str) -> Result<(), Error> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Err(Error::Status(400))
    } else {
        Ok(())
    }
}

/// One song, as listed and as queued to play.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub title: String,
    /// Who performs it.
    pub artist: String,
    /// The album it is on, and that album's id.
    pub album: String,
    pub album_id: Option<String>,
    pub number: Option<i32>,
    pub disc: Option<i32>,
    pub seconds: Option<f64>,
    pub image: Option<Image>,
}

impl Song {
    fn from(item: Item) -> Self {
        let artist = item
            .artists
            .as_ref()
            .filter(|a| !a.is_empty())
            .map(|a| a.join(", "))
            .or_else(|| item.album_artist.clone())
            .unwrap_or_default();
        Self {
            image: item.album_art(),
            seconds: item.run_time_ticks.map(|t| t as f64 / TICKS_PER_SECOND),
            number: item.index_number,
            disc: item.parent_index_number,
            album: item.album.clone().unwrap_or_default(),
            album_id: item.album_id.clone(),
            title: item.name,
            id: item.id,
            artist,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPage {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: Option<String>,
    pub year: Option<i32>,
    pub genres: Vec<String>,
    pub image: Option<Image>,
    pub songs: Vec<Song>,
}

/// An album and its songs, in the order they are on the disc.
pub async fn album_with(jf: &Jellyfin, album_id: &str) -> Result<AlbumPage, Error> {
    check_id(album_id)?;
    let uid = jf.user_id()?;
    let item: Item = jf.get_json(format!("/Items/{album_id}?userId={uid}")).await?;
    let songs: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&parentId={album_id}&includeItemTypes=Audio&sortBy=ParentIndexNumber,IndexNumber,SortName&{FIELDS}"
        ))
        .await?;
    let artist_id = item.album_artists.as_ref().and_then(|a| a.first()).map(|a| a.id.clone()).filter(|id| !id.is_empty());
    let image = item.image(Shape::Square);
    let mut songs: Vec<Song> = songs.items.into_iter().map(Song::from).collect();
    // A song's own art, when it has none, is the album's, which is this one.
    for song in &mut songs {
        if song.image.is_none() {
            song.image = image.clone();
        }
    }
    Ok(AlbumPage {
        artist: item.album_artist.clone().unwrap_or_default(),
        year: item.production_year,
        genres: item.genres_list(),
        title: item.name,
        id: item.id,
        artist_id,
        image,
        songs,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistPage {
    pub id: String,
    pub name: String,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub image: Option<Image>,
    pub albums: Vec<Card>,
    /// The ones played most, then by name.
    pub songs: Vec<Song>,
}

/// An artist, their albums (newest first) and a few of their songs.
pub async fn artist_with(jf: &Jellyfin, artist_id: &str) -> Result<ArtistPage, Error> {
    check_id(artist_id)?;
    let uid = jf.user_id()?;
    let (item, albums, songs) = futures_util::future::try_join3(
        jf.get_json::<Item>(format!("/Items/{artist_id}?userId={uid}")),
        jf.get_json::<ItemsPage>(format!(
            "/Items?userId={uid}&recursive=true&includeItemTypes=MusicAlbum&artistIds={artist_id}\
             &sortBy=ProductionYear,SortName&sortOrder=Descending&limit={ARTIST_ALBUMS}&{FIELDS}"
        )),
        jf.get_json::<ItemsPage>(format!(
            "/Items?userId={uid}&recursive=true&includeItemTypes=Audio&artistIds={artist_id}\
             &sortBy=PlayCount,SortName&sortOrder=Descending,Ascending&limit={ARTIST_SONGS}&{FIELDS}"
        )),
    )
    .await?;
    Ok(ArtistPage {
        overview: plain_text(item.overview.clone()),
        genres: item.genres_list(),
        image: item.image(Shape::Square),
        name: item.name,
        id: item.id,
        albums: albums.items.into_iter().map(|i| i.card(Shape::Square)).collect(),
        songs: songs.items.into_iter().map(Song::from).collect(),
    })
}

/// An album's songs, or an artist's, as the player would queue them: every one, in order.
pub async fn album_songs_with(jf: &Jellyfin, album_id: &str) -> Result<Vec<Song>, Error> {
    Ok(album_with(jf, album_id).await?.songs)
}

/// Every song of an artist, album by album, for "play all".
pub async fn artist_songs_with(jf: &Jellyfin, artist_id: &str) -> Result<Vec<Song>, Error> {
    check_id(artist_id)?;
    let uid = jf.user_id()?;
    let page: ItemsPage = jf
        .get_json(format!(
            "/Items?userId={uid}&recursive=true&includeItemTypes=Audio&artistIds={artist_id}\
             &sortBy=Album,ParentIndexNumber,IndexNumber,SortName&limit=500&{FIELDS}"
        ))
        .await?;
    Ok(page.items.into_iter().map(Song::from).collect())
}

/// The album a song is on, when it is on one.
pub async fn song_album_with(jf: &Jellyfin, song_id: &str) -> Result<Option<String>, Error> {
    check_id(song_id)?;
    let uid = jf.user_id()?;
    let item: Item = jf.get_json(format!("/Items/{song_id}?userId={uid}")).await?;
    Ok(item.album_id.clone().filter(|id| !id.is_empty()))
}
