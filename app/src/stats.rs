//! The playback readout (key I): what the player is doing, read from mpv once a second while it is
//! shown. Codec, size and rate, the decoder in use, frames dropped, bit rates, the audio, how far
//! ahead the cache is, and how far sound and picture are apart.

use crate::mpv::Engine;
use crate::{App, Player, StatRow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use std::sync::Arc;
use std::time::Duration;

/// "4.2 Mb/s" from mpv's bits per second.
fn bit_rate(bits: &str) -> Option<String> {
    let bits: f64 = bits.trim().parse().ok().filter(|b| *b > 0.0)?;
    Some(if bits >= 1e6 { format!("{:.1} Mb/s", bits / 1e6) } else { format!("{:.0} kb/s", bits / 1e3) })
}

/// A number mpv gave, rounded to `places`.
fn number(text: &str, places: usize) -> Option<String> {
    text.trim().parse::<f64>().ok().map(|n| format!("{n:.places$}"))
}

/// The rows to show, from a way of reading mpv's properties.
fn rows(get: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |label: &str, value: Option<String>| {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            out.push((label.to_string(), value));
        }
    };
    let (w, h) = (get("video-params/w"), get("video-params/h"));
    let size = match (w, h) {
        (Some(w), Some(h)) => Some(format!("{w}\u{d7}{h}")),
        _ => None,
    };
    let codec = get("video-format").map(|c| c.to_uppercase());
    let rate = get("container-fps").and_then(|f| number(&f, 3)).map(|f| format!("{f} fps"));
    add("Video", Some([codec, size, rate].into_iter().flatten().collect::<Vec<_>>().join("  ")));
    add("Decoder", Some(match get("hwdec-current").as_deref() {
        None | Some("") | Some("no") => "software".to_string(),
        Some(d) => format!("hardware ({d})"),
    }));
    add("Showing", get("estimated-vf-fps").and_then(|f| number(&f, 1)).map(|f| format!("{f} fps")));
    let (dropped_dec, dropped_out) = (get("decoder-frame-drop-count"), get("frame-drop-count"));
    if dropped_dec.is_some() || dropped_out.is_some() {
        add("Dropped", Some(format!("{} by the decoder, {} at the output", dropped_dec.unwrap_or_else(|| "0".into()), dropped_out.unwrap_or_else(|| "0".into()))));
    }
    add("Video rate", get("video-bitrate").and_then(|b| bit_rate(&b)));
    let channels = get("audio-params/channel-count").map(|c| format!("{c} ch"));
    let hz = get("audio-params/samplerate").and_then(|s| s.parse::<f64>().ok()).map(|s| format!("{:.1} kHz", s / 1000.0));
    let audio = [get("audio-codec-name").map(|c| c.to_uppercase()), channels, hz].into_iter().flatten().collect::<Vec<_>>().join("  ");
    add("Audio", Some(audio));
    add("Audio rate", get("audio-bitrate").and_then(|b| bit_rate(&b)));
    add("Cached", get("demuxer-cache-duration").and_then(|s| number(&s, 1)).map(|s| format!("{s} s ahead")));
    add("A/V sync", get("avsync").and_then(|s| number(&s, 3)).map(|s| format!("{s} s")));
    out
}

/// Starts reading the engine for the readout, while it is shown. UI thread.
pub fn install(app: &App, engine: Arc<Engine>) {
    thread_local! {
        static TICK: slint::Timer = slint::Timer::default();
    }
    let weak: Weak<App> = app.as_weak();
    TICK.with(|t| {
        t.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(app) = weak.upgrade() else { return };
            let player = app.global::<Player>();
            if !player.get_stats_visible() {
                return;
            }
            let shown: Vec<StatRow> = rows(|name| engine.property(name)).into_iter().map(|(label, value)| StatRow { label: SharedString::from(label), value: SharedString::from(value) }).collect();
            player.set_stats(ModelRc::new(VecModel::from(shown)));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn reader(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn bit_rates_read_in_megabits_or_kilobits() {
        assert_eq!(bit_rate("4200000").as_deref(), Some("4.2 Mb/s"));
        assert_eq!(bit_rate("192000").as_deref(), Some("192 kb/s"));
        assert_eq!(bit_rate("0"), None);
        assert_eq!(bit_rate("n/a"), None);
    }

    #[test]
    fn the_readout_names_what_mpv_reports_and_skips_what_it_does_not() {
        let r = rows(reader(&[
            ("video-params/w", "1920"),
            ("video-params/h", "1080"),
            ("video-format", "hevc"),
            ("container-fps", "23.976"),
            ("hwdec-current", "nvdec"),
            ("decoder-frame-drop-count", "2"),
            ("audio-codec-name", "aac"),
            ("audio-params/channel-count", "2"),
            ("audio-params/samplerate", "48000"),
        ]));
        let get = |label: &str| r.iter().find(|(l, _)| l == label).map(|(_, v)| v.as_str());
        assert_eq!(get("Video"), Some("HEVC  1920\u{d7}1080  23.976 fps"));
        assert_eq!(get("Decoder"), Some("hardware (nvdec)"));
        assert_eq!(get("Dropped"), Some("2 by the decoder, 0 at the output"));
        assert_eq!(get("Audio"), Some("AAC  2 ch  48.0 kHz"));
        // Nothing reported for these, so no row.
        assert_eq!(get("Video rate"), None);
        assert_eq!(get("A/V sync"), None);
    }

    #[test]
    fn no_hardware_decoder_reads_as_software() {
        let r = rows(reader(&[("hwdec-current", "no")]));
        assert!(r.iter().any(|(l, v)| l == "Decoder" && v == "software"));
    }
}
