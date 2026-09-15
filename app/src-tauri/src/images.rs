//! `bloom-img://` serves library artwork to the page.
//!
//! The page asks for `/{itemId}/{Primary|Thumb|Backdrop|Logo}/{tag}/{width}` and never sees the
//! server address or the token. Rust fetches the image as the signed-in user and keeps a copy in
//! a cache folder for that server, keyed by the image tag, which changes whenever the artwork
//! does. A cached file therefore never goes stale, and artwork already seen still shows when the
//! server is not reachable.
//!
//! `/{serverId}/{itemId}/...` names the server instead, and only ever reads the cache: the
//! sign-in wall uses it with nobody signed in, when a request would have no token to make it.
//! Removing a server deletes its folder, so its artwork doesn't outlive it.
//!
//! Only those path shapes are accepted, so the scheme can't be used to make any other request
//! with the user's token or to read files outside the cache.

use crate::home::Image;
use crate::jellyfin::{Error, Jellyfin};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tauri::http::{header, Request, Response};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

const KINDS: [&str; 4] = ["Primary", "Thumb", "Backdrop", "Logo"];
/// Requested widths are rounded up to one of these, so the cache holds a few sizes per image
/// rather than one for every layout width and screen scale. Mirrored in `src/lib/api.ts`.
const WIDTHS: [u32; 8] = [160, 240, 360, 480, 720, 960, 1280, 1920];

pub fn protocol<R: Runtime>(ctx: UriSchemeContext<'_, R>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = req.uri().path().to_string();
    tauri::async_runtime::spawn(async move {
        let jf = app.state::<Jellyfin>();
        let downloads = app.state::<crate::downloads::Downloads>();
        let response = match fetch_with(&jf, Some(&downloads), &path).await {
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, mime(&bytes))
                .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                .body(bytes),
            Err(e) => Response::builder().status(status(&e)).body(Vec::new()),
        };
        responder.respond(response.unwrap_or_else(|_| Response::new(Vec::new())));
    });
}

struct Key {
    /// Named only by cache-only requests (see the module notes).
    server: Option<String>,
    item: String,
    kind: &'static str,
    tag: String,
    width: u32,
}

/// Ids and tags are alphanumeric, which also keeps them safe as file and folder names.
fn token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl Key {
    fn parse(path: &str) -> Option<Key> {
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let (server, item, kind, tag, width) = match parts.as_slice() {
            [item, kind, tag, width] => (None, *item, *kind, *tag, *width),
            [server, item, kind, tag, width] => (Some(*server), *item, *kind, *tag, *width),
            _ => return None,
        };
        if !token(item) || !token(tag) || server.is_some_and(|s| !token(s)) {
            return None;
        }
        let kind = KINDS.into_iter().find(|k| *k == kind)?;
        let asked: u32 = width.parse().ok()?;
        let width = WIDTHS.into_iter().find(|w| *w >= asked).unwrap_or(WIDTHS[WIDTHS.len() - 1]);
        Some(Key { server: server.map(String::from), item: item.into(), kind, tag: tag.into(), width })
    }

    fn file_name(&self) -> String {
        format!("{}-{}-{}-{}", self.item, self.kind, self.tag, self.width)
    }
}

/// Where a server's artwork is cached. None for an id that isn't safe as a folder name.
pub(crate) fn server_folder(cache: &Path, server_id: &str) -> Option<PathBuf> {
    token(server_id).then(|| cache.join("images").join(server_id))
}

#[cfg(test)]
pub async fn fetch(jf: &Jellyfin, path: &str) -> Result<Vec<u8>, Error> {
    fetch_with(jf, None, path).await
}

/// `downloads` supplies artwork saved with a download, for pages opened with no server.
pub async fn fetch_with(jf: &Jellyfin, downloads: Option<&crate::downloads::Downloads>, path: &str) -> Result<Vec<u8>, Error> {
    if let Some(rest) = path.strip_prefix("/trickplay/") {
        return if rest.split('/').count() == 6 { fetch_trickplay_thumb(jf, rest).await } else { fetch_trickplay(jf, rest).await };
    }
    let key = Key::parse(path).ok_or(Error::Status(400))?;
    let server = match &key.server {
        Some(server) => server.clone(),
        None => jf.server_id()?,
    };
    let dir = server_folder(&jf.cache, &server).ok_or(Error::Status(400))?;
    let file = dir.join(key.file_name());
    if let Some(bytes) = read_cached(file.clone()).await {
        return Ok(bytes);
    }
    // Cache-only: there's no sign-in to fetch it with.
    if key.server.is_some() {
        return Err(Error::Status(404));
    }
    if let Some(bytes) = downloads.and_then(|d| d.artwork(&server, &key.item, key.kind, &key.tag)) {
        return Ok(bytes);
    }

    let res = jf
        .get(&format!("/Items/{}/Images/{}?tag={}&maxWidth={}&quality=90", key.item, key.kind, key.tag, key.width))
        .await?;
    let bytes: Vec<u8> = res.bytes().await.map_err(|_| Error::Unreadable)?.into();
    store_cached(dir, file, bytes).await
}

/// A cached file, read on the blocking pool: a page of cards asks for many at once, and the async
/// threads also carry the live connection and downloads. Reading one marks it recently used, for
/// `prune_cache`.
async fn read_cached(file: PathBuf) -> Option<Vec<u8>> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = fs::read(&file).ok()?;
        let _ = fs::File::options().append(true).open(&file).and_then(|f| f.set_modified(SystemTime::now()));
        Some(bytes)
    })
    .await
    .ok()
    .flatten()
}

/// Keeps a fetched file, on the blocking pool, and hands its bytes back. Best effort: a failed
/// write only costs a refetch next time. The temp name is unique so two requests for the same
/// image can't interleave their writes.
async fn store_cached(dir: PathBuf, file: PathBuf, bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
    tauri::async_runtime::spawn_blocking(move || {
        write_cached(&dir, &file, &bytes);
        bytes
    })
    .await
    .map_err(|_| Error::Unreadable)
}

/// Writes a cache file through a uniquely named temporary one. Best effort.
fn write_cached(dir: &Path, file: &Path, bytes: &[u8]) {
    let _ = fs::create_dir_all(dir);
    let tmp = dir.join(format!(".{}", uuid::Uuid::new_v4().simple()));
    if fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, file)).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// How much artwork the cache keeps before the least recently used goes, down to the target, and
/// how long seek-bar tiles are kept. All of it is disposable: anything removed downloads again when
/// it's next seen.
const CACHE_LIMIT: u64 = 1024 * 1024 * 1024;
const CACHE_TARGET: u64 = 768 * 1024 * 1024;
const TRICKPLAY_KEPT: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// Trims the artwork cache, once at startup.
pub fn prune_cache(cache: &Path) {
    prune_cache_to(cache, CACHE_LIMIT, CACHE_TARGET, TRICKPLAY_KEPT);
}

/// Seek-bar tiles not used for `trickplay_kept`, then, if the rest is over `limit`, the least
/// recently used files until it's under `target`.
fn prune_cache_to(cache: &Path, limit: u64, target: u64, trickplay_kept: Duration) {
    let Ok(servers) = fs::read_dir(cache.join("images")) else { return };
    let now = SystemTime::now();
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for server in servers.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
        let Ok(entries) = fs::read_dir(server.path()) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let used = meta.modified().unwrap_or(now);
            let unused_for = now.duration_since(used).unwrap_or_default();
            if entry.file_name().to_string_lossy().starts_with("trickplay-") && unused_for > trickplay_kept {
                let _ = fs::remove_file(entry.path());
                continue;
            }
            files.push((used, meta.len(), entry.path()));
        }
    }
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    if total <= limit {
        return;
    }
    files.sort_by_key(|(used, _, _)| *used);
    for (_, size, path) in files {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

/// `{itemId}/{mediaSourceId}/{width}/{index}` of a trickplay tile.
fn trickplay_key(rest: &str) -> Option<(&str, &str, u32, u32)> {
    let parts: Vec<&str> = rest.split('/').collect();
    let [item, source, width, index] = parts.as_slice() else { return None };
    if !token(item) || !token(source) {
        return None;
    }
    Some((item, source, width.parse().ok()?, index.parse().ok()?))
}

/// One tile of seek-bar thumbnails, cached beside the server's artwork. Tiles carry no tag, but
/// they only change if the server makes them again.
async fn fetch_trickplay(jf: &Jellyfin, rest: &str) -> Result<Vec<u8>, Error> {
    let (item, source, width, index) = trickplay_key(rest).ok_or(Error::Status(400))?;
    let dir = server_folder(&jf.cache, &jf.server_id()?).ok_or(Error::Status(400))?;
    let file = dir.join(format!("trickplay-{item}-{width}-{index}"));
    if let Some(bytes) = read_cached(file.clone()).await {
        return Ok(bytes);
    }
    let res = jf.get(&format!("/Videos/{item}/Trickplay/{width}/{index}.jpg?mediaSourceId={source}")).await?;
    let bytes: Vec<u8> = res.bytes().await.map_err(|_| Error::Unreadable)?.into();
    store_cached(dir, file, bytes).await
}

/// Thumbnail `n` of a tile: `{itemId}/{mediaSourceId}/{width}/{columns}x{rows}/{tile}/{n}`.
#[derive(Debug, PartialEq)]
struct ThumbKey<'a> {
    item: &'a str,
    source: &'a str,
    width: u32,
    columns: u32,
    rows: u32,
    tile: u32,
    n: u32,
}

fn trickplay_thumb_key(rest: &str) -> Option<ThumbKey<'_>> {
    let parts: Vec<&str> = rest.split('/').collect();
    let [item, source, width, grid, tile, n] = parts.as_slice() else { return None };
    if !token(item) || !token(source) {
        return None;
    }
    let (columns, rows) = grid.split_once('x')?;
    let key = ThumbKey {
        item,
        source,
        width: width.parse().ok()?,
        columns: columns.parse().ok()?,
        rows: rows.parse().ok()?,
        tile: tile.parse().ok()?,
        n: n.parse().ok()?,
    };
    // Jellyfin makes 10 by 10; a grid far off that isn't a tile worth cutting up.
    let sane = (1..=32).contains(&key.columns) && (1..=32).contains(&key.rows) && key.n < key.columns * key.rows;
    sane.then_some(key)
}

/// One seek-bar thumbnail. The server only serves whole tiles (10 by 10 thumbnails in one image,
/// 3200 by 1800 pixels at the usual width), and the page showing a tile as a background decoded
/// all of it, about 23 MB, to show one 320 by 180 frame. Here the tile is decoded (about 20 ms)
/// and the thumbnail asked for is answered at once; the tile's other thumbnails are then cached
/// as small JPEGs in the background, nearest first, and the tile itself isn't kept. Encoding all
/// of them before answering took nearly a second, felt as a lag while scrubbing.
async fn fetch_trickplay_thumb(jf: &Jellyfin, rest: &str) -> Result<Vec<u8>, Error> {
    let key = trickplay_thumb_key(rest).ok_or(Error::Status(400))?;
    let dir = server_folder(&jf.cache, &jf.server_id()?).ok_or(Error::Status(400))?;
    let name = |n: u32| format!("trickplay-{}-{}-{}-{n}", key.item, key.width, key.tile);
    if let Some(bytes) = read_cached(dir.join(name(key.n))).await {
        return Ok(bytes);
    }
    let tile = fetch_trickplay(jf, &format!("{}/{}/{}/{}", key.item, key.source, key.width, key.tile)).await?;
    let tile_file = dir.join(format!("trickplay-{}-{}-{}", key.item, key.width, key.tile));
    let files: Vec<PathBuf> = (0..key.columns * key.rows).map(|n| dir.join(name(n))).collect();
    let (columns, rows, wanted) = (key.columns, key.rows, key.n);
    tauri::async_runtime::spawn_blocking(move || {
        let image = decode_tile(&tile)?;
        let answer = thumbnail(&image, columns, rows, wanted)?;
        write_cached(&dir, &files[wanted as usize], &answer);
        // The rest after answering, nearest to this one first, for scrubbing on from here.
        tauri::async_runtime::spawn_blocking(move || {
            let mut rest: Vec<u32> = (0..columns * rows).filter(|n| *n != wanted).collect();
            rest.sort_by_key(|n| n.abs_diff(wanted));
            for n in rest {
                let file = &files[n as usize];
                if file.exists() {
                    continue;
                }
                if let Some(jpeg) = thumbnail(&image, columns, rows, n) {
                    write_cached(&dir, file, &jpeg);
                }
            }
            let _ = fs::remove_file(&tile_file);
        });
        Some(answer)
    })
    .await
    .ok()
    .flatten()
    .ok_or(Error::Unreadable)
}

fn decode_tile(tile: &[u8]) -> Option<image::RgbImage> {
    Some(image::load_from_memory_with_format(tile, image::ImageFormat::Jpeg).ok()?.to_rgb8())
}

/// Thumbnail `n` of a decoded tile, counting left to right and top to bottom, as a JPEG.
fn thumbnail(tile: &image::RgbImage, columns: u32, rows: u32, n: u32) -> Option<Vec<u8>> {
    let (w, h) = (tile.width() / columns, tile.height() / rows);
    if w == 0 || h == 0 || n >= columns * rows {
        return None;
    }
    let thumb = image::imageops::crop_imm(tile, (n % columns) * w, (n / columns) * h, w, h).to_image();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&thumb).ok()?;
    Some(jpeg)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedPoster {
    /// The server whose cache folder it's in, which the page names to read it.
    server_id: String,
    image: Image,
    /// The cached file's width, which the page asks for exactly so the file is used as it is.
    width: u32,
}

/// Width and height from an image file's first bytes, for the formats the server sends.
fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    let be16 = |i: usize| Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32);
    let le16 = |i: usize| Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32);
    let le24 = |i: usize| Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, 0]));
    let be32 = |i: usize| Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?));
    match b {
        [0x89, b'P', b'N', b'G', ..] => Some((be32(16)?, be32(20)?)),
        [0xFF, 0xD8, ..] => {
            // Walk the segments to the frame header, which holds the size.
            let mut i = 2;
            while i + 9 < b.len() {
                if b[i] != 0xFF {
                    return None;
                }
                let marker = b[i + 1];
                if marker == 0xFF {
                    i += 1;
                } else if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(i + 7)?, be16(i + 5)?));
                } else if matches!(marker, 0xD0..=0xD9 | 0x01) {
                    i += 2;
                } else {
                    i += 2 + be16(i + 2)? as usize;
                }
            }
            None
        }
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', b'V', b'P', b'8', kind, ..] => match kind {
            b' ' => Some((le16(26)? & 0x3FFF, le16(28)? & 0x3FFF)),
            b'L' => {
                let bits = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
                Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            b'X' => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether a cached file is portrait artwork. The cache only knows an image's kind, and an
/// episode's Primary is a landscape still, so the file itself has to say.
fn is_portrait(path: &Path) -> bool {
    use std::io::Read;
    let mut head = Vec::with_capacity(64 * 1024);
    let read = fs::File::open(path).and_then(|f| f.take(64 * 1024).read_to_end(&mut head));
    read.is_ok() && image_size(&head).is_some_and(|(w, h)| w > 0 && h * 5 >= w * 6)
}

/// A random pick of posters already cached for the given servers, one per item, different each
/// time. A cached file is served without a request, so these show on the sign-in screen with
/// nobody signed in.
fn cached_posters_in(cache: &Path, servers: &[String], limit: usize) -> Vec<CachedPoster> {
    // Each item's largest cached Primary, from file names alone: "{item}-{kind}-{tag}-{width}".
    let mut by_item: HashMap<(String, String), (String, Key, PathBuf)> = HashMap::new();
    for server in servers {
        let Some(folder) = server_folder(cache, server) else { continue };
        let Ok(entries) = fs::read_dir(folder) else { continue };
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else { continue };
            let Some(key) = Key::parse(&format!("/{}", name.replacen('-', "/", 3))) else { continue };
            if key.kind != "Primary" || key.width < 240 {
                continue;
            }
            let id = (server.clone(), key.item.clone());
            if by_item.get(&id).is_none_or(|(_, kept, _)| kept.width < key.width) {
                by_item.insert(id, (server.clone(), key, entry.path()));
            }
        }
    }
    let mut candidates: Vec<(String, Key, PathBuf)> = by_item.into_values().collect();
    shuffle(&mut candidates);
    // Checked in the shuffled order, so only as many files are opened as it takes to find enough.
    candidates
        .into_iter()
        .filter(|(_, _, path)| is_portrait(path))
        .take(limit)
        .map(|(server_id, key, _)| CachedPoster {
            server_id,
            image: Image { item_id: key.item, kind: key.kind, tag: key.tag },
            width: key.width,
        })
        .collect()
}

/// Fisher-Yates, drawing on the random v4 UUIDs Bloom already makes: no generator to seed.
fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = (uuid::Uuid::new_v4().as_u128() % (i as u128 + 1)) as usize;
        items.swap(i, j);
    }
}

#[tauri::command]
pub async fn cached_posters(jf: tauri::State<'_, Jellyfin>) -> Result<Vec<CachedPoster>, Error> {
    // Only servers still on the list: a removed server's artwork is gone, and a server nobody
    // added again shouldn't reappear here.
    let servers: Vec<String> = crate::jellyfin::servers_with(&jf).into_iter().map(|entry| entry.server.id).collect();
    // Reading the cache folders is disk work; keep it off the thread the window runs on.
    let cache = jf.cache.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || cached_posters_in(&cache, &servers, 16)).await.unwrap_or_default())
}

fn status(e: &Error) -> u16 {
    match e {
        Error::SignedOut => 401,
        Error::Unreachable(_) => 502,
        Error::Status(code) => *code,
        _ => 500,
    }
}

fn mime(b: &[u8]) -> &'static str {
    match b {
        [0xFF, 0xD8, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [b'G', b'I', b'F', ..] => "image/gif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{cached_posters_in, decode_tile, image_size, prune_cache_to, thumbnail, trickplay_key, trickplay_thumb_key, Key, ThumbKey};

    #[test]
    fn thumbnail_paths() {
        assert_eq!(
            trickplay_thumb_key("abc/src1/320/10x10/4/37"),
            Some(ThumbKey { item: "abc", source: "src1", width: 320, columns: 10, rows: 10, tile: 4, n: 37 })
        );
        for bad in ["abc/src1/320/10x10/4/100", "abc/src1/320/0x10/4/0", "abc/src1/320/99x99/4/0", "../src1/320/10x10/4/0", "abc/src1/320/10/4/0"] {
            assert!(trickplay_thumb_key(bad).is_none(), "{bad} was accepted");
        }
    }

    /// A real tile from the server, cut up: `BLOOM_TILE=<cached trickplay file> cargo test real_tile -- --ignored`.
    #[test]
    #[ignore]
    fn a_real_tile_is_cut_into_its_thumbnails() {
        let path = std::env::var("BLOOM_TILE").expect("BLOOM_TILE names a cached tile");
        let tile = std::fs::read(&path).unwrap();
        let started = std::time::Instant::now();
        let image = decode_tile(&tile).expect("the tile didn't decode");
        let first = thumbnail(&image, 10, 10, 37).unwrap();
        println!("the first thumbnail answered in {:?} (decode and one encode)", started.elapsed());
        let rest = std::time::Instant::now();
        let bytes: usize = (0..100).map(|n| thumbnail(&image, 10, 10, n).unwrap().len()).sum();
        println!("all 100 in {:?} more, {} KB in all", rest.elapsed(), bytes / 1024);
        let decoded = image::load_from_memory(&first).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (320, 180));
    }

    #[test]
    fn a_tile_is_cut_into_its_thumbnails() {
        // Four columns and two rows of 16 by 9 thumbnails, each a different shade.
        let tile = image::RgbImage::from_fn(64, 18, |x, y| {
            let n = (y / 9) * 4 + x / 16;
            image::Rgb([(n * 30) as u8, 0, 0])
        });
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95).encode_image(&tile).unwrap();
        let decoded = decode_tile(&jpeg).unwrap();
        assert!(thumbnail(&decoded, 4, 2, 8).is_none(), "there are only eight");
        let sixth = image::load_from_memory(&thumbnail(&decoded, 4, 2, 5).unwrap()).unwrap().to_rgb8();
        assert_eq!(sixth.dimensions(), (16, 9));
        let red = sixth.get_pixel(8, 4)[0];
        assert!((140..=160).contains(&red), "the sixth thumbnail came from the wrong place ({red})");
    }
    use std::time::{Duration, SystemTime};

    #[test]
    fn the_cache_keeps_what_was_used_last() {
        let cache = std::env::temp_dir().join(format!("bloom-prune-test-{}", uuid::Uuid::new_v4().simple()));
        let folder = cache.join("images").join("srv1");
        std::fs::create_dir_all(&folder).unwrap();
        let day = Duration::from_secs(24 * 60 * 60);
        let now = SystemTime::now();
        // (name, size, last used this long ago)
        let files = [
            ("old-Primary-t1-360", 400, 10 * day),
            ("mid-Primary-t2-360", 400, 5 * day),
            ("new-Primary-t3-360", 400, day),
            ("trickplay-a-320-0", 10, 30 * day),
            ("trickplay-b-320-0", 10, day),
        ];
        for (name, size, age) in files {
            let path = folder.join(name);
            std::fs::write(&path, vec![0u8; size]).unwrap();
            std::fs::File::options().append(true).open(&path).unwrap().set_modified(now - age).unwrap();
        }
        prune_cache_to(&cache, 1000, 900, 14 * day);
        let mut left: Vec<String> =
            std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        let _ = std::fs::remove_dir_all(&cache);
        assert_eq!(left, ["mid-Primary-t2-360", "new-Primary-t3-360", "trickplay-b-320-0"]);
    }

    #[test]
    fn trickplay_paths() {
        assert_eq!(trickplay_key("abc/src1/320/4"), Some(("abc", "src1", 320, 4)));
        for bad in ["abc/src1/320", "../src1/320/4", "abc/src1/wide/4", "abc/src1/320/4/x", "abc/s%2F/320/4"] {
            assert!(trickplay_key(bad).is_none(), "{bad} was accepted");
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes
    }

    #[test]
    fn image_sizes_from_headers() {
        assert_eq!(image_size(&png(240, 360)), Some((240, 360)));
        // A JPEG with an APP0 segment before its baseline frame header.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x02, 0x1C, 0x01, 0x68, 0x03,
        ];
        assert_eq!(image_size(&jpeg), Some((360, 540)));
        assert_eq!(image_size(b"not an image"), None);
    }

    #[test]
    fn cached_posters_are_portrait_one_per_item_from_listed_servers() {
        let cache = std::env::temp_dir().join(format!("bloom-cache-test-{}", uuid::Uuid::new_v4().simple()));
        let listed = cache.join("images").join("srv1");
        let removed = cache.join("images").join("srv2");
        std::fs::create_dir_all(&listed).unwrap();
        std::fs::create_dir_all(&removed).unwrap();
        let files: [(&str, Vec<u8>); 7] = [
            ("aaa-Primary-t1-360", png(360, 540)),
            ("aaa-Primary-t1-720", png(720, 1080)),
            ("bbb-Backdrop-t2-1280", png(1280, 720)),
            ("ccc-Primary-t3-160", png(160, 240)),
            ("eee-Primary-t5-480", png(480, 270)),
            (".tmpfile", png(360, 540)),
            ("ddd-Primary-t4-480", png(480, 720)),
        ];
        for (name, bytes) in files {
            std::fs::write(listed.join(name), bytes).unwrap();
        }
        std::fs::write(removed.join("fff-Primary-t6-480"), png(480, 720)).unwrap();
        std::fs::write(cache.join("images").join("ggg-Primary-t7-480"), png(480, 720)).unwrap();

        let posters = cached_posters_in(&cache, &["srv1".to_string(), "../x".to_string()], 10);
        let _ = std::fs::remove_dir_all(&cache);
        let mut items: Vec<&str> = posters.iter().map(|p| p.image.item_id.as_str()).collect();
        items.sort();
        assert_eq!(items, ["aaa", "ddd"], "only portrait posters, once each, from servers on the list");
        assert!(posters.iter().all(|p| p.server_id == "srv1"));
        assert_eq!(posters.iter().find(|p| p.image.item_id == "aaa").map(|p| p.width), Some(720), "not the largest copy");
    }

    #[test]
    fn paths() {
        let k = Key::parse("/0123456789abcdef0123456789abcdef/Primary/fedcba9876543210fedcba9876543210/300").unwrap();
        assert_eq!((k.kind, k.width), ("Primary", 360));
        assert!(k.server.is_none());
        let cached = Key::parse("/srv1/a/Primary/b/360").unwrap();
        assert_eq!(cached.server.as_deref(), Some("srv1"));
        assert!(Key::parse("/../a/Primary/b/360").is_none());
        assert_eq!(Key::parse("/a/Thumb/b/5000").unwrap().width, 1920);
        for bad in ["/a/Banner/b/300", "/a/Primary/b", "/a/Primary/b/300/x", "/../Primary/b/300", "/a/Primary/b/wide", "/a%2F/Primary/b/1"] {
            assert!(Key::parse(bad).is_none(), "{bad} was accepted");
        }
    }
}
