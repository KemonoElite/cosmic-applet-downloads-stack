// SPDX-License-Identifier: GPL-3.0-only

//! Download totals from Chromium-based browsers (Brave, Chrome, Chromium, Vivaldi, Edge, ...).
//!
//! A `.crdownload` file on its own doesn't say how big the download will be, but the browser keeps
//! each download's expected size in the `downloads` table of the profile's `History` database. The
//! browser holds that database locked, so it's copied to the runtime directory, read, and the copy
//! deleted. Only the paths, sizes and state of unfinished downloads are read, nothing else.
//!
//! The browser commits to the database every few seconds, so a copy is only made when the
//! database has changed since the last read.

use rusqlite::{Connection, OpenFlags};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::SystemTime,
};

/// Profile folders, relative to `~/.config` (native) or `~/.var/app/<id>/config` (Flatpak).
const BROWSERS: &[(&str, Option<&str>)] = &[
    ("BraveSoftware/Brave-Browser", Some("com.brave.Browser")),
    ("BraveSoftware/Brave-Browser-Beta", None),
    ("BraveSoftware/Brave-Browser-Nightly", None),
    ("google-chrome", Some("com.google.Chrome")),
    ("google-chrome-beta", None),
    ("google-chrome-unstable", None),
    ("chromium", Some("org.chromium.Chromium")),
    ("vivaldi", Some("com.vivaldi.Vivaldi")),
    ("microsoft-edge", Some("com.microsoft.Edge")),
    ("microsoft-edge-beta", None),
    ("opera", Some("com.opera.Opera")),
    ("thorium", None),
];

/// Chromium's `DownloadState` values in the history database.
const STATE_IN_PROGRESS: i64 = 0;
const STATE_INTERRUPTED: i64 = 4;

/// An unfinished download, as the browser recorded it.
#[derive(Debug, Clone)]
pub struct Download {
    /// The `.crdownload` file being written.
    pub current_path: PathBuf,
    /// Where the file goes once it's done.
    pub target_path: PathBuf,
    /// Expected size; `None` when the server didn't say.
    pub total: Option<u64>,
    pub interrupted: bool,
}

struct Cached {
    stamp: SystemTime,
    downloads: Vec<Download>,
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, Cached>>> = LazyLock::new(Default::default);

/// Unfinished downloads from every Chromium-based browser profile found, keyed by the path of
/// the `.crdownload` file.
pub fn unfinished() -> HashMap<PathBuf, Download> {
    let mut found = HashMap::new();
    let Ok(mut cache) = CACHE.lock() else {
        return found;
    };

    for history in history_databases() {
        let Some(stamp) = stamp(&history) else {
            continue;
        };

        let fresh = cache
            .get(&history)
            .is_some_and(|cached| cached.stamp == stamp);
        if !fresh {
            match read(&history) {
                Ok(downloads) => {
                    cache.insert(history.clone(), Cached { stamp, downloads });
                }
                Err(err) => {
                    tracing::debug!(path = %history.display(), "couldn't read downloads: {err}");
                    continue;
                }
            }
        }

        if let Some(cached) = cache.get(&history) {
            for download in &cached.downloads {
                found.insert(download.current_path.clone(), download.clone());
            }
        }
    }

    found
}

/// Every `History` database under the known browser folders: `<browser>/History` (Opera) and
/// `<browser>/<profile>/History` (everything else).
fn history_databases() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(config) = dirs::config_dir() {
        roots.extend(BROWSERS.iter().map(|(dir, _)| config.join(dir)));
    }
    if let Some(home) = dirs::home_dir() {
        roots.extend(BROWSERS.iter().filter_map(|(dir, flatpak)| {
            flatpak.map(|id| home.join(".var/app").join(id).join("config").join(dir))
        }));
    }

    let mut databases = Vec::new();
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        let direct = root.join("History");
        if direct.is_file() {
            databases.push(direct);
        }
        let Ok(profiles) = fs::read_dir(&root) else {
            continue;
        };
        for profile in profiles.filter_map(Result::ok) {
            let history = profile.path().join("History");
            if history.is_file() {
                databases.push(history);
            }
        }
    }
    databases
}

/// When the database or its write-ahead log last changed.
fn stamp(history: &Path) -> Option<SystemTime> {
    let modified = |path: &Path| fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let main = modified(history)?;
    let wal = modified(&sidecar(history, "-wal"));
    Some(wal.map_or(main, |wal| wal.max(main)))
}

fn read(history: &Path) -> Result<Vec<Download>, Box<dyn std::error::Error>> {
    let dir = scratch_dir()?;
    let copy = dir.join(format!("History-{}", path_key(history)));
    let copy_wal = sidecar(&copy, "-wal");

    fs::copy(history, &copy)?;
    // Recent commits may still be in the write-ahead log; SQLite replays it on open.
    let wal = sidecar(history, "-wal");
    if wal.is_file() {
        fs::copy(&wal, &copy_wal)?;
    }

    let result = query(&copy);

    for path in [copy.clone(), copy_wal, sidecar(&copy, "-shm")] {
        let _ = fs::remove_file(path);
    }
    result
}

fn query(db: &Path) -> Result<Vec<Download>, Box<dyn std::error::Error>> {
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut statement = conn.prepare(
        "SELECT current_path, target_path, total_bytes, state FROM downloads \
         WHERE state IN (?1, ?2) AND current_path != ''",
    )?;
    let rows = statement.query_map([STATE_IN_PROGRESS, STATE_INTERRUPTED], |row| {
        let total: i64 = row.get(2)?;
        let state: i64 = row.get(3)?;
        Ok(Download {
            current_path: PathBuf::from(row.get::<_, String>(0)?),
            target_path: PathBuf::from(row.get::<_, String>(1)?),
            total: u64::try_from(total).ok().filter(|&total| total > 0),
            interrupted: state == STATE_INTERRUPTED,
        })
    })?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// A private folder for the copies: `$XDG_RUNTIME_DIR` (memory-backed) when there is one.
fn scratch_dir() -> std::io::Result<PathBuf> {
    let base = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("cosmic-applet-downloads-stack");
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// A short, stable name for a copy, so two profiles never share one.
fn path_key(path: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a database with the columns this module reads, the way Chromium lays them out.
    #[test]
    fn reads_unfinished_downloads() {
        let dir = std::env::temp_dir().join(format!("ds-chromium-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("History");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE downloads (id INTEGER PRIMARY KEY, current_path TEXT, target_path TEXT, \
             received_bytes INTEGER, total_bytes INTEGER, state INTEGER);
             INSERT INTO downloads VALUES (1, '/d/a.iso.crdownload', '/d/a.iso', 5, 100, 0);
             INSERT INTO downloads VALUES (2, '/d/b.zip.crdownload', '/d/b.zip', 5, 0, 4);
             INSERT INTO downloads VALUES (3, '/d/c.txt', '/d/c.txt', 9, 9, 1);",
        )
        .unwrap();
        drop(conn);

        let downloads = query(&db).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(downloads.len(), 2);
        let a = downloads
            .iter()
            .find(|d| d.target_path.ends_with("a.iso"))
            .unwrap();
        assert_eq!(a.current_path, Path::new("/d/a.iso.crdownload"));
        assert_eq!(a.total, Some(100));
        assert!(!a.interrupted);
        let b = downloads
            .iter()
            .find(|d| d.target_path.ends_with("b.zip"))
            .unwrap();
        assert_eq!(b.total, None);
        assert!(b.interrupted);
    }

    /// The browser keeps the database open, locked, with recent writes still in the WAL.
    #[test]
    fn reads_a_locked_database_through_a_copy() {
        let dir = std::env::temp_dir().join(format!("ds-chromium-lock-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("History");

        let browser = Connection::open(&db).unwrap();
        browser
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; \
                 PRAGMA locking_mode=EXCLUSIVE;
                 CREATE TABLE downloads (id INTEGER PRIMARY KEY, current_path TEXT, \
                 target_path TEXT, received_bytes INTEGER, total_bytes INTEGER, state INTEGER);
                 INSERT INTO downloads VALUES (1, '/d/x.iso.crdownload', '/d/x.iso', 1, 500, 0);",
            )
            .unwrap();

        // Reading the original directly fails while the browser holds it...
        let direct = Connection::open(&db).and_then(|conn| {
            conn.busy_timeout(std::time::Duration::ZERO)?;
            conn.query_row("SELECT count(*) FROM downloads", [], |r| r.get::<_, i64>(0))
        });
        assert!(
            direct.is_err(),
            "expected the locked database to refuse readers"
        );

        // ...but the copy has everything, including what's only in the WAL so far.
        let downloads = read(&db).unwrap();
        drop(browser);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].total, Some(500));
    }
}
