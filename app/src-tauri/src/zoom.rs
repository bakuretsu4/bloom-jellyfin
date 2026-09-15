//! Interface zoom, stepped like a browser's (Ctrl + / Ctrl - / Ctrl 0, and Ctrl + wheel).
//!
//! Rust applies the zoom to the webview and remembers it in the settings (settings.rs), so the
//! saved level is in place before the page first paints. Only the webview scales: mpv's layer
//! underneath is untouched, so video keeps its full resolution.

use crate::settings::SettingsStore;
use std::sync::Mutex;
use tauri::{State, WebviewWindow};

const STEPS: [f64; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

pub struct Zoom {
    level: Mutex<f64>,
}

impl Zoom {
    /// Starts at the saved level, snapped to a step.
    pub fn new(saved: f64) -> Self {
        Self { level: Mutex::new(STEPS[nearest(saved)]) }
    }

    pub fn level(&self) -> f64 {
        *self.level.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn nearest(zoom: f64) -> usize {
    STEPS
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - zoom).abs().total_cmp(&(b.1 - zoom).abs()))
        .map_or(5, |(i, _)| i)
}

#[tauri::command]
pub fn zoom_level(zoom: State<'_, Zoom>) -> f64 {
    zoom.level()
}

/// `direction` 1 zooms in a step, -1 out a step, 0 resets to 100%. Returns the new level.
#[tauri::command]
pub fn zoom_step(
    window: WebviewWindow,
    zoom: State<'_, Zoom>,
    store: State<'_, SettingsStore>,
    direction: i32,
) -> Result<f64, String> {
    let mut level = zoom.level.lock().unwrap_or_else(|e| e.into_inner());
    let i = nearest(*level);
    let next = match direction.signum() {
        1 => STEPS[(i + 1).min(STEPS.len() - 1)],
        -1 => STEPS[i.saturating_sub(1)],
        _ => 1.0,
    };
    window.set_zoom(next).map_err(|e| e.to_string())?;
    *level = next;
    store.update(|settings| settings.zoom = next);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::{nearest, STEPS};

    #[test]
    fn snaps_to_steps() {
        assert_eq!(STEPS[nearest(1.0)], 1.0);
        assert_eq!(STEPS[nearest(1.13)], 1.1);
        assert_eq!(STEPS[nearest(9.0)], 2.0);
        assert_eq!(STEPS[nearest(f64::NAN)], STEPS[nearest(f64::NAN)]);
    }
}
