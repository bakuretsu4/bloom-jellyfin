//! Albums, artists and songs: their pages, and playing songs one after another.
//!
//! A song plays through the same player as a film, but with no screen of its own: the page that is
//! open stays, and a player at the bottom of the window (see ui/music.slint) shows what is playing.

use crate::cards;
use crate::nav::{self, Route};
use crate::text;
use crate::{App, Music, NavTarget, Screen, SongData};
use bloom_core::jellyfin::Jellyfin;
use bloom_core::music::{self as core, Song};
use bloom_core::playback;
use bloom_core::Ctx;
use serde_json::Value;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

/// What is playing and what follows it.
struct Queue {
    songs: Vec<Song>,
    index: usize,
}

static QUEUE: Mutex<Option<Queue>> = Mutex::new(None);
/// Counts the songs started, so a slow start that was overtaken is left alone.
static STARTED: AtomicU64 = AtomicU64::new(0);

fn queue() -> MutexGuard<'static, Option<Queue>> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The page on screen, for what its buttons play.
#[derive(Default)]
struct Shown {
    id: String,
    artist: bool,
    artist_id: Option<String>,
    songs: Vec<Song>,
}

thread_local! {
    static SHOWN: RefCell<Shown> = RefCell::new(Shown::default());
}

/// Whether a song is the one playing, or waiting its turn after it.
pub fn active() -> bool {
    queue().is_some()
}

fn on_ui(weak: &Weak<App>, f: impl FnOnce(&App) + Send + 'static) {
    let _ = weak.upgrade_in_event_loop(move |app| f(&app));
}

fn reset_page(app: &App, kind: &str, initial: &str, round: bool) {
    let music = app.global::<Music>();
    music.set_kind_label(kind.into());
    music.set_title("".into());
    music.set_subtitle("".into());
    music.set_meta("".into());
    music.set_overview("".into());
    music.set_initial(initial.into());
    music.set_has_image(false);
    music.set_round(round);
    music.set_artist_link(false);
    music.set_songs(ModelRc::new(VecModel::<SongData>::default()));
    music.set_songs_heading("".into());
    music.set_albums(ModelRc::new(VecModel::<crate::CardData>::default()));
    music.set_error("".into());
    music.set_loading(true);
    app.set_nav_target(NavTarget::Libraries);
    app.set_screen(Screen::Music);
    SHOWN.with(|s| *s.borrow_mut() = Shown::default());
}

/// "2021, 12 songs, 48 min".
fn album_meta(year: Option<i32>, songs: &[Song]) -> String {
    let mut parts = Vec::new();
    if let Some(year) = year {
        parts.push(year.to_string());
    }
    parts.push(text::count(songs.len() as u64, "song"));
    let seconds: f64 = songs.iter().filter_map(|s| s.seconds).sum();
    if seconds >= 60.0 {
        parts.push(text::runtime((seconds / 60.0).round() as u32));
    }
    parts.join(", ")
}

/// Rows for a list of songs. On an album's page the number leads and the artist is named only when
/// it differs from the album's; on an artist's page the album's cover leads and its name follows.
fn rows(songs: &[Song], on_album: Option<&str>) -> Vec<SongData> {
    songs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let subtitle = match on_album {
                Some(album_artist) if s.artist == album_artist => String::new(),
                Some(_) => s.artist.clone(),
                None => s.album.clone(),
            };
            SongData {
                id: s.id.clone().into(),
                number: s.number.filter(|n| *n > 0).unwrap_or(i as i32 + 1).to_string().into(),
                title: s.title.clone().into(),
                subtitle: subtitle.into(),
                duration: s.seconds.map(text::clock).unwrap_or_default().into(),
                image: Image::default(),
                has_image: false,
            }
        })
        .collect()
}

/// Shows an album's page. UI thread.
pub fn show_album(ctx: &Ctx, weak: &Weak<App>, app: &App, id: String) {
    reset_page(app, "Album", "\u{266a}", false);
    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match core::album_with(&jf, &id).await {
            Ok(album) => {
                let cover = album.image.as_ref().map(|i| i.path(600));
                let (ctx2, weak2) = (ctx.clone(), weak.clone());
                on_ui(&weak, move |app| {
                    if cards::generation() != generation {
                        return;
                    }
                    let music = app.global::<Music>();
                    music.set_loading(false);
                    music.set_title(album.title.clone().into());
                    music.set_subtitle(album.artist.clone().into());
                    music.set_artist_link(album.artist_id.is_some());
                    music.set_meta(album_meta(album.year, &album.songs).into());
                    music.set_songs(ModelRc::new(VecModel::from(rows(&album.songs, Some(&album.artist)))));
                    SHOWN.with(|s| *s.borrow_mut() = Shown { id: album.id, artist: false, artist_id: album.artist_id, songs: album.songs });
                    if let Some(path) = cover {
                        cards::fetch_one(&ctx2, &weak2, generation, path, |app, image| {
                            let music = app.global::<Music>();
                            music.set_image(image);
                            music.set_has_image(true);
                        });
                    }
                });
            }
            Err(e) => failed(&weak, generation, e.to_string()),
        }
    });
}

/// Shows an artist's page. UI thread.
pub fn show_artist(ctx: &Ctx, weak: &Weak<App>, app: &App, id: String) {
    reset_page(app, "Artist", "\u{266b}", true);
    let (ctx, weak, generation) = (ctx.clone(), weak.clone(), cards::generation());
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        match core::artist_with(&jf, &id).await {
            Ok(artist) => {
                let portrait = artist.image.as_ref().map(|i| i.path(600));
                let plain: Vec<cards::PlainCard> = artist.albums.iter().map(|c| cards::PlainCard::from_core(c, cards::card_width(false))).collect();
                let cover_jobs: Vec<(usize, String)> = artist.songs.iter().enumerate().filter_map(|(i, s)| s.image.as_ref().map(|im| (i, im.path(120)))).collect();
                let (ctx2, weak2) = (ctx.clone(), weak.clone());
                on_ui(&weak, move |app| {
                    if cards::generation() != generation {
                        return;
                    }
                    let music = app.global::<Music>();
                    music.set_loading(false);
                    music.set_title(artist.name.clone().into());
                    music.set_subtitle(artist.genres.join(", ").into());
                    music.set_overview(artist.overview.clone().unwrap_or_default().into());
                    music.set_initial(artist.name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into());
                    if !artist.songs.is_empty() {
                        music.set_songs_heading("Songs".into());
                    }
                    let mut song_rows = rows(&artist.songs, None);
                    for row in &mut song_rows {
                        row.number = SharedString::default();
                    }
                    let model = Rc::new(VecModel::from(song_rows));
                    let tracker = cards::track(&model, |row: &mut SongData, image| {
                        row.image = image;
                        row.has_image = true;
                    });
                    music.set_songs(ModelRc::from(model));
                    cards::fetch_rows(&ctx2, &weak2, tracker, cover_jobs);
                    let (album_tracker, album_model) = cards::card_model(&plain);
                    music.set_albums(ModelRc::from(album_model));
                    cards::fetch_rows(&ctx2, &weak2, album_tracker, cards::jobs(&plain, 0));
                    SHOWN.with(|s| *s.borrow_mut() = Shown { id: artist.id, artist: true, artist_id: None, songs: artist.songs });
                    if let Some(path) = portrait {
                        cards::fetch_one(&ctx2, &weak2, generation, path, |app, image| {
                            let music = app.global::<Music>();
                            music.set_image(image);
                            music.set_has_image(true);
                        });
                    }
                });
            }
            Err(e) => failed(&weak, generation, e.to_string()),
        }
    });
}

fn failed(weak: &Weak<App>, generation: u64, message: String) {
    if crate::session::is_signed_out(&message) {
        return crate::session::ended(weak);
    }
    on_ui(weak, move |app| {
        if cards::generation() == generation {
            let music = app.global::<Music>();
            music.set_loading(false);
            music.set_error(message.into());
        }
    });
}

/// "Play" or "Shuffle" on the page: all of an album, or all of an artist's songs.
pub fn play_all(ctx: Ctx, weak: Weak<App>, shuffle: bool) {
    let w = weak.clone();
    on_ui(&w, move |_| {
        let (id, artist, songs) = SHOWN.with(|s| {
            let s = s.borrow();
            (s.id.clone(), s.artist, s.songs.clone())
        });
        if !artist {
            return start(ctx, weak, songs, 0, shuffle);
        }
        bloom_core::rt::spawn(async move {
            let jf = ctx.state::<Jellyfin>();
            match core::artist_songs_with(&jf, &id).await {
                Ok(all) => {
                    let weak2 = weak.clone();
                    on_ui(&weak2, move |_| start(ctx, weak, all, 0, shuffle));
                }
                Err(e) => crate::watch::say(&weak, e.to_string()),
            }
        });
    });
}

/// A row of the page was pressed: it plays, and what follows it on the page plays after it.
pub fn play_song(ctx: Ctx, weak: Weak<App>, index: i32) {
    let songs = SHOWN.with(|s| s.borrow().songs.clone());
    let Ok(index) = usize::try_from(index) else { return };
    if index < songs.len() {
        start(ctx, weak, songs, index, false);
    }
}

/// A song's card was pressed (in a library, or a search): its album plays from it.
pub fn play_card(ctx: Ctx, weak: Weak<App>, song_id: String) {
    bloom_core::rt::spawn(async move {
        let jf = ctx.state::<Jellyfin>();
        let item = playback_item_album(&jf, &song_id).await;
        let songs = match item {
            Some(album) => core::album_songs_with(&jf, &album).await.unwrap_or_default(),
            None => Vec::new(),
        };
        let index = songs.iter().position(|s| s.id == song_id);
        let weak2 = weak.clone();
        match index {
            Some(index) => on_ui(&weak2, move |_| start(ctx, weak, songs, index, false)),
            // It isn't on an album: it plays alone.
            None => {
                let alone = Song { id: song_id, title: String::new(), artist: String::new(), album: String::new(), album_id: None, number: None, disc: None, seconds: None, image: None };
                on_ui(&weak2, move |_| start(ctx, weak, vec![alone], 0, false));
            }
        }
    });
}

/// The album a song is on.
async fn playback_item_album(jf: &Jellyfin, song_id: &str) -> Option<String> {
    core::song_album_with(jf, song_id).await.ok().flatten()
}

/// Queues `songs` from `index` (shuffled when asked) and starts the first. UI thread.
fn start(ctx: Ctx, weak: Weak<App>, mut songs: Vec<Song>, index: usize, shuffle: bool) {
    if songs.is_empty() {
        return;
    }
    let mut index = index.min(songs.len() - 1);
    if shuffle {
        shuffle_in_place(&mut songs);
        index = 0;
    }
    *queue() = Some(Queue { songs, index });
    play_current(ctx, weak);
}

/// A shuffle that needs no randomness beyond the clock: Fisher-Yates over a xorshift.
fn shuffle_in_place<T>(items: &mut [T]) {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9e37_79b9_7f4a_7c15) | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// Plays the queue's current song and shows it at the bottom of the window.
fn play_current(ctx: Ctx, weak: Weak<App>) {
    let Some((song, has_prev, has_next)) = queue().as_ref().map(|q| (q.songs[q.index].clone(), q.index > 0, q.index + 1 < q.songs.len())) else { return };
    let mine = STARTED.fetch_add(1, Ordering::SeqCst) + 1;
    bloom_core::rt::spawn(async move {
        match playback::play(&ctx, song.id.clone(), None, Some(0.0)).await {
            Ok(now) => {
                if STARTED.load(Ordering::SeqCst) != mine {
                    return;
                }
                let (title, artist) = (now.title.clone(), now.subtitle.clone().unwrap_or_default());
                let cover = song.image.as_ref().map(|i| i.path(120));
                let (id, ctx2, weak2) = (song.id.clone(), ctx.clone(), weak.clone());
                on_ui(&weak, move |app| {
                    let music = app.global::<Music>();
                    music.set_active(true);
                    music.set_playing_id(id.clone().into());
                    music.set_now_title(title.into());
                    music.set_now_artist(artist.into());
                    music.set_has_prev(has_prev);
                    music.set_has_next(has_next);
                    music.set_now_has_image(false);
                    app.set_position(0.0);
                    app.set_paused(false);
                    if let Some(path) = cover {
                        bloom_core::rt::spawn(async move {
                            let Some(pixels) = cards::pixels(&ctx2, &path).await else { return };
                            on_ui(&weak2, move |app| {
                                // Only if that song is still the one playing.
                                let music = app.global::<Music>();
                                if music.get_playing_id() == id.as_str() {
                                    music.set_now_image(Image::from_rgba8(pixels));
                                    music.set_now_has_image(true);
                                }
                            });
                        });
                    }
                });
            }
            Err(e) => {
                if STARTED.load(Ordering::SeqCst) == mine {
                    clear(&weak);
                    crate::watch::say(&weak, e.to_string());
                }
            }
        }
    });
}

/// Previous or next song.
pub fn step(ctx: Ctx, weak: Weak<App>, delta: i32) {
    let moved = {
        let mut q = queue();
        let Some(q) = q.as_mut() else { return };
        let next = q.index as i64 + i64::from(delta);
        if next < 0 || next as usize >= q.songs.len() {
            false
        } else {
            q.index = next as usize;
            true
        }
    };
    if moved {
        play_current(ctx, weak);
    }
}

/// The player's close button: playback stops.
pub fn stop(ctx: Ctx, weak: Weak<App>) {
    STARTED.fetch_add(1, Ordering::SeqCst);
    clear(&weak);
    bloom_core::rt::spawn(async move {
        let pb = ctx.state::<playback::Playback>();
        if let Err(e) = playback::playback_stop(&ctx, &pb).await {
            crate::watch::say(&weak, e.to_string());
        }
    });
}

pub fn toggle(ctx: Ctx, weak: Weak<App>) {
    crate::watch::toggle_pause(ctx, weak);
}

/// Forgets the queue and hides the player at the bottom.
pub fn clear(weak: &Weak<App>) {
    *queue() = None;
    on_ui(weak, |app| {
        let music = app.global::<Music>();
        music.set_active(false);
        music.set_playing_id("".into());
    });
}

/// A song ended (or failed): the next one starts, or the player goes.
pub fn on_ended(ctx: Ctx, weak: &Weak<App>, payload: &Value) {
    let ended = payload["item_id"].as_str().or_else(|| payload["itemId"].as_str()).unwrap_or("");
    let current = queue().as_ref().map(|q| q.songs[q.index].id.clone()).unwrap_or_default();
    if ended != current {
        return;
    }
    if payload["reason"].as_str() == Some("error") {
        let message = payload["message"].as_str().map(|m| format!("The player reported: {m}.")).unwrap_or_else(|| "Playback stopped with an error.".into());
        clear(weak);
        return crate::watch::say(weak, message);
    }
    let has_next = queue().as_ref().is_some_and(|q| q.index + 1 < q.songs.len());
    if has_next {
        step(ctx, weak.clone(), 1);
    } else {
        clear(weak);
    }
}

/// The page of the artist whose album is on screen.
pub fn open_artist(ctx: Ctx, weak: Weak<App>) {
    if let Some(id) = SHOWN.with(|s| s.borrow().artist_id.clone()) {
        nav::go(ctx, weak, Route::Artist(id));
    }
}

/// The album of the song that is playing.
pub fn open_playing(ctx: Ctx, weak: Weak<App>) {
    let album = queue().as_ref().and_then(|q| q.songs[q.index].album_id.clone());
    if let Some(id) = album {
        nav::go(ctx, weak, Route::Album(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, number: Option<i32>, artist: &str) -> Song {
        Song { id: id.into(), title: id.into(), artist: artist.into(), album: "Album".into(), album_id: None, number, disc: None, seconds: Some(125.0), image: None }
    }

    #[test]
    fn an_album_meta_line_counts_songs_and_minutes() {
        let songs = [song("a", Some(1), "X"), song("b", Some(2), "X"), song("c", Some(3), "X")];
        assert_eq!(album_meta(Some(2021), &songs), "2021, 3 songs, 6m");
        assert_eq!(album_meta(None, &songs[..1]), "1 song, 2m");
    }

    #[test]
    fn album_rows_name_an_artist_only_when_it_differs() {
        let songs = [song("a", Some(1), "Aurora Vale"), song("b", Some(2), "Guest"), song("c", None, "Aurora Vale")];
        let rows = rows(&songs, Some("Aurora Vale"));
        let subtitles: Vec<String> = rows.iter().map(|r| r.subtitle.to_string()).collect();
        assert_eq!(subtitles, ["", "Guest", ""]);
        // A song with no track number takes its place in the list.
        assert_eq!(rows[2].number, "3");
        assert_eq!(rows[0].duration, "2:05");
    }

    #[test]
    fn a_shuffle_keeps_every_song() {
        let mut items: Vec<u32> = (0..50).collect();
        shuffle_in_place(&mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(items, sorted, "fifty songs came out in order");
    }
}
