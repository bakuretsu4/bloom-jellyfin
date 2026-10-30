//! Wording shared by the pages: language names, lengths, sizes.

/// A language's name from its ISO 639 code (two or three letters), or the code itself.
pub fn language(code: &str) -> String {
    let name = match code.to_ascii_lowercase().as_str() {
        "en" | "eng" => "English",
        "ja" | "jpn" => "Japanese",
        "es" | "spa" => "Spanish",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "it" | "ita" => "Italian",
        "pt" | "por" => "Portuguese",
        "ru" | "rus" => "Russian",
        "zh" | "chi" | "zho" => "Chinese",
        "ko" | "kor" => "Korean",
        "ar" | "ara" => "Arabic",
        "hi" | "hin" => "Hindi",
        "nl" | "dut" | "nld" => "Dutch",
        "sv" | "swe" => "Swedish",
        "pl" | "pol" => "Polish",
        "tr" | "tur" => "Turkish",
        "id" | "ind" => "Indonesian",
        "th" | "tha" => "Thai",
        "vi" | "vie" => "Vietnamese",
        "no" | "nor" | "nob" => "Norwegian",
        "da" | "dan" => "Danish",
        "fi" | "fin" => "Finnish",
        "cs" | "cze" | "ces" => "Czech",
        "hu" | "hun" => "Hungarian",
        "el" | "gre" | "ell" => "Greek",
        "he" | "heb" => "Hebrew",
        "uk" | "ukr" => "Ukrainian",
        "ro" | "rum" | "ron" => "Romanian",
        "ms" | "may" | "msa" => "Malay",
        "fil" | "tl" | "tgl" => "Filipino",
        "und" => "Unknown",
        _ => return code.to_uppercase(),
    };
    name.to_string()
}

/// "Japanese, English", or the first `max` and a count past that ("Japanese, English +3").
pub fn language_list(codes: &[String], max: usize) -> String {
    let names: Vec<String> = codes.iter().map(|c| language(c)).collect();
    if names.len() > max {
        format!("{} +{}", names[..max].join(", "), names.len() - max)
    } else {
        names.join(", ")
    }
}

/// "1h 52m", "2h", "45m".
pub fn runtime(minutes: u32) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// "2.9 GB", "340 MB".
pub fn size(bytes: i64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", (bytes as f64 / 1e6).round())
    }
}

/// "7.1 Mb/s".
pub fn bitrate(bits: i64) -> String {
    format!("{:.1} Mb/s", bits as f64 / 1e6)
}

/// "N episode" / "N episodes".
pub fn count(n: u64, unit: &str) -> String {
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// "Ends at 9:42 PM": the local time `minutes` from now, in the viewer's own 12 or 24 hour style.
pub fn ends_at(minutes: u32) -> String {
    let when = std::time::SystemTime::now() + std::time::Duration::from_secs(u64::from(minutes) * 60);
    let secs = when.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as libc::time_t).unwrap_or(0);
    format!("Ends at {}", local_clock(secs))
}

/// A time of day in the local zone: "9:42 PM", or "21:42" where the locale has no AM or PM.
pub fn local_clock(secs: libc::time_t) -> String {
    // SAFETY: `localtime_r` and `strftime` write only into the structs and buffer given them.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&secs, &mut tm).is_null() {
            return String::new();
        }
        let format = |f: &[u8]| {
            let mut buf = [0 as libc::c_char; 32];
            let n = libc::strftime(buf.as_mut_ptr(), buf.len(), f.as_ptr().cast(), &tm);
            String::from_utf8_lossy(&buf[..n].iter().map(|c| *c as u8).collect::<Vec<u8>>()).into_owned()
        };
        if format(b"%p\0").is_empty() {
            format(b"%H:%M\0")
        } else {
            format(b"%-I:%M %p\0")
        }
    }
}

/// Seconds as `m:ss`, or `h:mm:ss` from an hour up.
pub fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 { seconds as u64 } else { 0 };
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// The first letter, upper case, for an avatar.
pub fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_read_as_the_original_writes_them() {
        assert_eq!(runtime(45), "45m");
        assert_eq!(runtime(112), "1h 52m");
        assert_eq!(runtime(120), "2h");
    }

    #[test]
    fn languages_are_named_and_long_lists_are_counted() {
        let codes: Vec<String> = ["jpn", "eng", "spa", "fre"].iter().map(|s| s.to_string()).collect();
        assert_eq!(language_list(&codes, 8), "Japanese, English, Spanish, French");
        assert_eq!(language_list(&codes, 2), "Japanese, English +2");
        assert_eq!(language("xx"), "XX");
        assert_eq!(language_list(&[], 8), "");
    }

    #[test]
    fn sizes_and_rates() {
        assert_eq!(size(2_900_000_000), "2.9 GB");
        assert_eq!(size(340_400_000), "340 MB");
        assert_eq!(bitrate(7_130_000), "7.1 Mb/s");
        assert_eq!(count(1, "episode"), "1 episode");
        assert_eq!(count(2, "episode"), "2 episodes");
    }
}
