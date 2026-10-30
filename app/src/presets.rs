//! Ready-made themes: popular open-source colour schemes, as the CSS the theme editor takes (the
//! files in `docs/themes`, which can also be pasted into the Svelte app). Choosing one adds it to
//! the person's own themes and uses it, so it can then be edited or deleted like any other.

use crate::themes;
use bloom_core::settings::{CustomTheme, ThemeBase};

/// Their ids start like this, so choosing another preset can take the last unedited one away.
pub const ID_PREFIX: &str = "preset-";

/// `(file name without .css, the CSS)`, by the theme's name.
const FILES: [(&str, &str); 30] = [
    ("ayu-dark", include_str!("../../docs/themes/ayu-dark.css")),
    ("ayu-mirage", include_str!("../../docs/themes/ayu-mirage.css")),
    ("catppuccin-frappe", include_str!("../../docs/themes/catppuccin-frappe.css")),
    ("catppuccin-latte", include_str!("../../docs/themes/catppuccin-latte.css")),
    ("catppuccin-macchiato", include_str!("../../docs/themes/catppuccin-macchiato.css")),
    ("catppuccin-mocha", include_str!("../../docs/themes/catppuccin-mocha.css")),
    ("dracula", include_str!("../../docs/themes/dracula.css")),
    ("everforest-dark", include_str!("../../docs/themes/everforest-dark.css")),
    ("everforest-light", include_str!("../../docs/themes/everforest-light.css")),
    ("github-dark", include_str!("../../docs/themes/github-dark.css")),
    ("github-light", include_str!("../../docs/themes/github-light.css")),
    ("gruvbox-dark", include_str!("../../docs/themes/gruvbox-dark.css")),
    ("gruvbox-light", include_str!("../../docs/themes/gruvbox-light.css")),
    ("horizon", include_str!("../../docs/themes/horizon.css")),
    ("kanagawa", include_str!("../../docs/themes/kanagawa.css")),
    ("palenight", include_str!("../../docs/themes/palenight.css")),
    ("night-owl", include_str!("../../docs/themes/night-owl.css")),
    ("nightfox", include_str!("../../docs/themes/nightfox.css")),
    ("nord", include_str!("../../docs/themes/nord.css")),
    ("one-dark", include_str!("../../docs/themes/one-dark.css")),
    ("one-light", include_str!("../../docs/themes/one-light.css")),
    ("oxocarbon", include_str!("../../docs/themes/oxocarbon.css")),
    ("poimandres", include_str!("../../docs/themes/poimandres.css")),
    ("rose-pine", include_str!("../../docs/themes/rose-pine.css")),
    ("rose-pine-dawn", include_str!("../../docs/themes/rose-pine-dawn.css")),
    ("rose-pine-moon", include_str!("../../docs/themes/rose-pine-moon.css")),
    ("solarized-dark", include_str!("../../docs/themes/solarized-dark.css")),
    ("solarized-light", include_str!("../../docs/themes/solarized-light.css")),
    ("tokyo-night", include_str!("../../docs/themes/tokyo-night.css")),
    ("tomorrow-night", include_str!("../../docs/themes/tomorrow-night.css")),
];

pub struct Preset {
    pub id: String,
    pub theme: CustomTheme,
}

/// The theme a preset file describes.
fn parse(slug: &str, css: &str) -> Preset {
    // "/* Nord for Bloom." names it.
    let name = css.lines().next().and_then(|l| l.strip_prefix("/* ")).and_then(|l| l.split_once(" for Bloom")).map_or(slug, |(n, _)| n).to_string();
    let blank = CustomTheme { id: format!("{ID_PREFIX}{slug}"), name, base: ThemeBase::Dark, ..CustomTheme::default() };
    let (theme, _) = themes::from_css(css, &blank);
    Preset { id: theme.id.clone(), theme }
}

/// Every preset, in the order they are listed.
pub fn all() -> Vec<Preset> {
    FILES.iter().map(|(slug, css)| parse(slug, css)).collect()
}

pub fn find(id: &str) -> Option<Preset> {
    all().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_named_distinct_and_passes_its_contrast_checks() {
        let presets = all();
        assert!(presets.len() >= 30, "{} presets", presets.len());
        let mut ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), presets.len(), "two presets share an id");
        for p in &presets {
            assert!(themes::usable(&p.theme), "{} fails a contrast check", p.theme.name);
            assert!(!p.theme.name.is_empty() && !p.theme.name.contains("for Bloom"), "{:?}", p.theme.name);
        }
    }

    #[test]
    fn light_and_dark_presets_both_exist() {
        let presets = all();
        assert!(presets.iter().any(|p| p.theme.base == ThemeBase::Light));
        assert!(presets.iter().any(|p| p.theme.base == ThemeBase::Dark));
    }

    #[test]
    fn a_preset_is_found_by_its_id() {
        assert_eq!(find("preset-nord").map(|p| p.theme.name), Some("Nord".to_string()));
        assert!(find("preset-nope").is_none());
    }
}
