//! The environment for programs Bloom starts from the system: yt-dlp, the desktop's browser
//! opener, fc-list.
//!
//! Inside the AppImage, its startup points a dozen variables at the AppImage's own files:
//! LD_LIBRARY_PATH at its libraries, PYTHONHOME at its (absent) Python, PATH, XDG_DATA_DIRS,
//! GTK_PATH and more. A system program started with them breaks: on Arch, yt-dlp's Python died at
//! once ("No module named 'encodings'") and kde-open refused the bundled OpenSSL, so a trailer
//! neither played nor opened in the browser. Programs started here get those entries taken back
//! out, as the desktop would have run them. Outside an AppImage nothing changes.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

/// Variables Bloom's own AppImage hook sets for its WebKit (packaging/apprun-bloom.sh), which
/// another program shouldn't inherit.
const HOOK_ONLY: [&str; 2] = ["WEBKIT_FORCE_DMABUF_RENDERER", "WEBKIT_DMABUF_RENDERER_FORCE_SHM"];

/// What to change for a program started from the system: a variable set to a new value, or
/// removed with None.
pub(crate) fn changes() -> Vec<(OsString, Option<OsString>)> {
    changes_for(std::env::var_os("APPDIR").as_deref(), std::env::vars_os())
}

/// A `std::process::Command` for a system program, with the environment it should have.
pub(crate) fn command(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    for (name, value) in changes() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

/// The same, for tokio.
pub(crate) fn async_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    for (name, value) in changes() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    command
}

/// PATH as a system program sees it, for finding one.
pub(crate) fn path() -> Option<OsString> {
    let changed = changes().into_iter().find(|(name, _)| name == "PATH");
    match changed {
        Some((_, value)) => value,
        None => std::env::var_os("PATH"),
    }
}

fn inside(entry: &[u8], appdir: &[u8]) -> bool {
    entry.starts_with(appdir) && matches!(entry.get(appdir.len()), None | Some(b'/'))
}

fn changes_for(appdir: Option<&OsStr>, vars: impl Iterator<Item = (OsString, OsString)>) -> Vec<(OsString, Option<OsString>)> {
    let Some(appdir) = appdir.filter(|a| Path::new(a).is_absolute() && a.len() > 1) else { return Vec::new() };
    let appdir = appdir.as_bytes().strip_suffix(b"/").unwrap_or(appdir.as_bytes());
    let mut out = Vec::new();
    for (name, value) in vars {
        let key = name.to_string_lossy();
        if key == "APPDIR" || key == "APPIMAGE" || key == "ARGV0" || key == "OWD" || HOOK_ONLY.contains(&key.as_ref()) {
            out.push((name, None));
            continue;
        }
        let bytes = value.as_bytes();
        if !bytes.windows(appdir.len()).any(|w| w == appdir) {
            continue;
        }
        // A list of paths keeps its entries outside the AppImage; a single path inside it goes.
        let entries: Vec<&[u8]> = bytes.split(|&b| b == b':').filter(|entry| !entry.is_empty()).collect();
        let kept: Vec<&[u8]> = entries.iter().copied().filter(|entry| !inside(entry, appdir)).collect();
        // Only mentioning a path that starts the same (another mount) isn't a change.
        if kept.len() == entries.len() {
            continue;
        }
        if kept.is_empty() {
            out.push((name, None));
        } else {
            out.push((name, Some(OsString::from_vec(kept.join(&b':')))));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(list: &[(&str, &str)]) -> impl Iterator<Item = (OsString, OsString)> {
        list.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect::<Vec<_>>().into_iter()
    }

    fn get<'a>(changes: &'a [(OsString, Option<OsString>)], name: &str) -> Option<&'a Option<OsString>> {
        changes.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    #[test]
    fn inside_an_appimage_its_own_entries_are_taken_out() {
        let appdir = OsStr::new("/tmp/.mount_bloom.ninGMe");
        let env = [
            // As the AppImage's startup set them on an Arch laptop.
            ("PATH", "/tmp/.mount_bloom.ninGMe/usr/bin/:/tmp/.mount_bloom.ninGMe/usr/sbin/:/usr/local/bin:/usr/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_bloom.ninGMe/usr/lib/:/tmp/.mount_bloom.ninGMe//usr/lib/x86_64-linux-gnu/"),
            ("PYTHONHOME", "/tmp/.mount_bloom.ninGMe/usr/"),
            ("XDG_DATA_DIRS", "/tmp/.mount_bloom.ninGMe/usr/share/:/usr/share:/var/lib/flatpak/exports/share"),
            ("GTK_PATH", "/tmp/.mount_bloom.ninGMe//usr/lib/x86_64-linux-gnu/gtk-3.0:/usr/lib/gtk-3.0"),
            ("APPDIR", "/tmp/.mount_bloom.ninGMe"),
            ("APPIMAGE", "/home/me/AppImages/bloom.appimage"),
            ("WEBKIT_FORCE_DMABUF_RENDERER", "1"),
            ("HOME", "/home/me"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            // A different mount that only shares the start of the name isn't the AppImage's.
            ("OTHER", "/tmp/.mount_bloom.ninGMeX/lib"),
        ];
        let changes = changes_for(Some(appdir), vars(&env));
        assert_eq!(get(&changes, "PATH"), Some(&Some(OsString::from("/usr/local/bin:/usr/bin"))));
        assert_eq!(get(&changes, "LD_LIBRARY_PATH"), Some(&None), "the AppImage's libraries leaked");
        assert_eq!(get(&changes, "PYTHONHOME"), Some(&None), "Python would look for itself in the AppImage");
        assert_eq!(get(&changes, "XDG_DATA_DIRS"), Some(&Some(OsString::from("/usr/share:/var/lib/flatpak/exports/share"))));
        assert_eq!(get(&changes, "GTK_PATH"), Some(&Some(OsString::from("/usr/lib/gtk-3.0"))));
        for gone in ["APPDIR", "APPIMAGE", "WEBKIT_FORCE_DMABUF_RENDERER"] {
            assert_eq!(get(&changes, gone), Some(&None), "{gone} was passed on");
        }
        assert_eq!(get(&changes, "HOME"), None, "an unrelated variable was touched");
        assert_eq!(get(&changes, "WAYLAND_DISPLAY"), None);
        assert_eq!(get(&changes, "OTHER"), None, "a look-alike path was taken for the AppImage's");
    }

    /// The fix in a real AppImage's environment: save a running AppImage Bloom's with
    /// `cp /proc/$(pgrep -x bloom)/environ /tmp/bloom.environ`, then
    /// `BLOOM_ENVIRON=/tmp/bloom.environ cargo test real_appimage_environment -- --ignored`.
    /// Needs yt-dlp and python3 installed.
    #[test]
    #[ignore = "needs a saved AppImage environment"]
    fn real_appimage_environment() {
        let file = std::env::var("BLOOM_ENVIRON").expect("BLOOM_ENVIRON names a saved environ file");
        let saved: Vec<(OsString, OsString)> = std::fs::read(file)
            .unwrap()
            .split(|&b| b == 0)
            .filter_map(|kv| {
                let at = kv.iter().position(|&b| b == b'=')?;
                Some((OsString::from_vec(kv[..at].to_vec()), OsString::from_vec(kv[at + 1..].to_vec())))
            })
            .collect();
        let appdir = saved.iter().find(|(k, _)| k == "APPDIR").map(|(_, v)| v.clone()).expect("not an AppImage's environment");
        let changes = changes_for(Some(&appdir), saved.clone().into_iter());
        let mut env: std::collections::BTreeMap<OsString, OsString> = saved.into_iter().collect();
        for (name, value) in changes {
            match value {
                Some(value) => env.insert(name, value),
                None => env.remove(&name),
            };
        }
        let run = |program: &str, args: &[&str]| {
            let output = std::process::Command::new(program).args(args).env_clear().envs(&env).output().unwrap();
            let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            println!("{program} {args:?}: {} {}", output.status, text.trim());
            (output.status.success(), text)
        };
        assert!(run("yt-dlp", &["--version"]).0, "yt-dlp still fails");
        assert!(run("python3", &["-c", "import ssl; print(ssl.OPENSSL_VERSION)"]).0, "Python still fails");
        if std::process::Command::new("kde-open").arg("--version").output().is_ok() {
            let (_, text) = run("kde-open", &["--version"]);
            assert!(!text.contains("not found") && !text.contains("symbol lookup error"), "kde-open still loads the AppImage's libraries");
        }
    }

    #[test]
    fn outside_an_appimage_nothing_changes() {
        let env = [("PATH", "/usr/bin"), ("LD_LIBRARY_PATH", "/opt/lib"), ("WEBKIT_FORCE_DMABUF_RENDERER", "1")];
        assert!(changes_for(None, vars(&env)).is_empty());
        assert!(changes_for(Some(OsStr::new("relative")), vars(&env)).is_empty());
        assert!(changes_for(Some(OsStr::new("/")), vars(&env)).is_empty());
    }
}
