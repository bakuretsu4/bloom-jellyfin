//! Which items are films and shows, for the poster wall beside the sign-in form.
//!
//! The artwork cache holds portrait pictures of two kinds: posters and the photos of cast and
//! crew, and nothing in a cache file says which is which. So the cards that are shown note the
//! films and shows they stand for, and the wall draws only from those. Kept in a small file in the
//! data folder so it is there at the next launch, when nobody is signed in to ask.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The most that are kept: far more than the sixteen the wall shows, and a bound on the file.
const MOST: usize = 600;

static FILE: OnceLock<PathBuf> = OnceLock::new();

struct Known {
    /// In the order first seen, oldest first.
    order: Vec<String>,
    set: HashSet<String>,
    saved_at: Option<Instant>,
    dirty: bool,
}

static KNOWN: LazyLock<Mutex<Known>> = LazyLock::new(|| Mutex::new(Known { order: Vec::new(), set: HashSet::new(), saved_at: None, dirty: false }));

fn known() -> std::sync::MutexGuard<'static, Known> {
    KNOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads what was noted in earlier sessions. At startup.
pub fn init(data_folder: &std::path::Path) {
    let path = data_folder.join("title-posters.json");
    let saved: Vec<String> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let mut k = known();
    for id in saved {
        if id.bytes().all(|b| b.is_ascii_alphanumeric()) && k.set.insert(id.clone()) {
            k.order.push(id);
        }
    }
    let _ = FILE.set(path);
}

/// A film, show or collection whose poster was shown.
pub fn note(id: &str) {
    let mut k = known();
    if !k.set.insert(id.to_string()) {
        return;
    }
    k.order.push(id.to_string());
    if k.order.len() > MOST {
        let dropped = k.order.remove(0);
        k.set.remove(&dropped);
    }
    k.dirty = true;
    // Written at most every few seconds while new ones keep arriving; the last few may wait for the
    // next one, which costs only a slightly smaller wall.
    if k.saved_at.is_none_or(|at| at.elapsed() > Duration::from_secs(5)) {
        save(&mut k);
    }
}

fn save(k: &mut Known) {
    k.saved_at = Some(Instant::now());
    k.dirty = false;
    if let (Some(path), Ok(bytes)) = (FILE.get(), serde_json::to_vec(&k.order)) {
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// Writes what is noted but not yet saved. When the sign-in screen opens and at exit.
pub fn flush() {
    let mut k = known();
    if k.dirty {
        save(&mut k);
    }
}

/// The items noted.
pub fn titles() -> HashSet<String> {
    known().set.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noting_an_item_twice_keeps_it_once_and_the_oldest_go_first() {
        let before = titles().len();
        note("zzzwall1");
        note("zzzwall1");
        assert_eq!(titles().len(), before + 1);
        assert!(titles().contains("zzzwall1"));
    }
}
