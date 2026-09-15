//! Trailers: the web trailers (almost always YouTube) a server lists for a film or show.
//!
//! With yt-dlp installed they play in Bloom's own player: yt-dlp finds the video and audio
//! streams behind the page, and mpv plays those like any other stream. mpv's own yt-dlp hook
//! stays off (see player.rs), so nothing of the user's mpv setup is involved. Without yt-dlp, or
//! when it can't find the streams, the trailer opens in the desktop's browser.
//!
//! Only https addresses the server listed are ever used: the page names a trailer by its
//! number, never by its address.

use crate::jellyfin::{Error, Jellyfin};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tauri::State;

/// yt-dlp on the PATH, if it's installed.
pub(crate) fn yt_dlp() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join("yt-dlp")).find(|candidate| candidate.is_file())
}

/// The streams behind a trailer's page.
#[derive(Debug, PartialEq)]
pub(crate) struct Found {
    pub(crate) video: String,
    /// A separate audio stream, when the best video carries none.
    pub(crate) audio: Option<String>,
    /// Headers the streams must be asked for with ("Name: value").
    pub(crate) headers: Vec<String>,
    pub(crate) duration: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FormatDto {
    url: String,
    http_headers: HashMap<String, String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct InfoDto {
    url: Option<String>,
    http_headers: HashMap<String, String>,
    requested_formats: Option<Vec<FormatDto>>,
    duration: Option<f64>,
}

fn header_lines(headers: &HashMap<String, String>) -> Vec<String> {
    let mut lines: Vec<String> = headers
        .iter()
        .filter(|(name, value)| !name.eq_ignore_ascii_case("cookie") && !name.contains(['\r', '\n']) && !value.contains(['\r', '\n']))
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    lines.sort();
    lines
}

/// yt-dlp's --dump-json: a video and an audio format, or one format with both.
fn parse(json: &[u8]) -> Option<Found> {
    let info: InfoDto = serde_json::from_slice(json).ok()?;
    match info.requested_formats.filter(|formats| !formats.is_empty()) {
        Some(formats) => {
            let video = formats.first().filter(|f| !f.url.is_empty())?;
            Some(Found {
                video: video.url.clone(),
                audio: formats.get(1).map(|audio| audio.url.clone()).filter(|url| !url.is_empty()),
                headers: header_lines(&video.http_headers),
                duration: info.duration,
            })
        }
        None => Some(Found {
            video: info.url.filter(|url| !url.is_empty())?,
            audio: None,
            headers: header_lines(&info.http_headers),
            duration: info.duration,
        }),
    }
}

/// How long yt-dlp may take in all before the trailer opens in the browser instead.
const RESOLVE_LIMIT: Duration = Duration::from_secs(30);

/// Asks yt-dlp for the best video up to 1080p (AV1 left out, which not every GPU decodes) with
/// the best audio. None without yt-dlp, or when it can't find them in time. yt-dlp is killed
/// when it runs out of time or nothing waits for it any more.
pub(crate) async fn resolve(page: &str) -> Option<Found> {
    let program = yt_dlp()?;
    let mut command = tokio::process::Command::new(program);
    command
        .args(["--dump-json", "--no-playlist", "--no-warnings", "--socket-timeout", "10"])
        .args(["--retries", "1", "--extractor-retries", "1"])
        .args(["-f", "bv*[height<=1080][vcodec!^=av01]+ba/b[height<=1080]/b"])
        // The address after "--", so it can never be read as an option.
        .args(["--", page])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(RESOLVE_LIMIT, command.output()).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&output.stdout)
}

/// Opens an https address in the desktop's browser.
pub(crate) fn open_in_browser(url: &str) -> Result<(), Error> {
    if !url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https") {
        return Err(Error::Status(400));
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";
    let mut child = std::process::Command::new(program)
        .arg(url)
        .spawn()
        .map_err(|e| Error::Player(format!("couldn't open the browser ({e})")))?;
    // Reaped once it exits, so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Whether trailers can play in Bloom's player on this computer.
#[tauri::command]
pub fn trailers_in_app() -> bool {
    yt_dlp().is_some()
}

#[tauri::command]
pub async fn open_trailer(jf: State<'_, Jellyfin>, item_id: String, index: usize) -> Result<(), Error> {
    let (_, url) = crate::library::trailer_with(&jf, &item_id, index).await?;
    open_in_browser(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_video_and_audio() {
        let json = br#"{"duration": 142.0, "requested_formats": [
            {"url": "https://v.example/video", "http_headers": {"User-Agent": "Mozilla/5.0", "Cookie": "secret"}},
            {"url": "https://v.example/audio", "http_headers": {}}]}"#;
        let found = parse(json).unwrap();
        assert_eq!(found.video, "https://v.example/video");
        assert_eq!(found.audio.as_deref(), Some("https://v.example/audio"));
        assert_eq!(found.headers, ["User-Agent: Mozilla/5.0"], "a cookie was passed on");
        assert_eq!(found.duration, Some(142.0));
    }

    #[test]
    fn one_format_with_both() {
        let found = parse(br#"{"url": "https://v.example/both", "http_headers": {"Accept": "*/*"}}"#).unwrap();
        assert_eq!((found.video.as_str(), found.audio), ("https://v.example/both", None));
        assert!(parse(br#"{"title": "no streams"}"#).is_none());
        assert!(parse(b"not json").is_none());
    }

    #[test]
    fn only_https_opens() {
        assert!(open_in_browser("file:///etc/passwd").is_err());
        assert!(open_in_browser("javascript:alert(1)").is_err());
    }
}
