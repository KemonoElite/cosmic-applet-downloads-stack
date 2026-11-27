// SPDX-License-Identifier: GPL-3.0-only

//! Reading the Downloads folder. Nothing here touches the UI, so it runs on a blocking thread.

use crate::{chromium, thumbnails};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// Suffixes browsers add while a download is still being written.
/// `.part`: Firefox; `.crdownload`: Chromium-based; `.download`: Epiphany/Safari; `.opdownload`: Opera.
const PARTIAL: [&str; 4] = [".part", ".crdownload", ".download", ".opdownload"];
/// A download that hasn't grown for this long is shown as stalled.
const STALLED_AFTER: Duration = Duration::from_secs(30);

/// One item in the stack.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The file on disk; for a download in progress, the partial file.
    pub path: PathBuf,
    /// What to show: the file name, or for a download in progress the name it will have.
    pub name: String,
    /// Freedesktop icon names to try in order: the specific mime icon first, generic last.
    pub icon_names: Vec<String>,
    /// Set while the file is still downloading.
    pub progress: Option<Progress>,
    /// Set for a finished picture that can be previewed.
    pub picture: Option<thumbnails::Stamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    /// `None` when the size isn't known (Firefox, or a server that doesn't send one).
    pub total: Option<u64>,
    pub state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Active,
    /// Nothing written for a while.
    Stalled,
    /// The browser says the download failed or was paused.
    Interrupted,
}

impl Progress {
    /// 0.0 to 1.0, when the total is known.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total?;
        #[allow(clippy::cast_precision_loss)]
        Some((self.received as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }
}

/// The user's Downloads folder (`XDG_DOWNLOAD_DIR`), or `~/Downloads` when that isn't set.
pub fn dir() -> Option<PathBuf> {
    [
        dirs::download_dir(),
        dirs::home_dir().map(|home| home.join("Downloads")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_dir())
}

/// What's in the Downloads folder.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// The most recently modified finished files and folders, newest first.
    pub files: Vec<Entry>,
    /// Downloads still in progress, newest first.
    pub downloading: Vec<Entry>,
}

/// The `limit` most recently modified finished items in `dir`, and every download in progress.
/// Hidden files are skipped.
pub fn scan(dir: &Path, limit: usize) -> Scan {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Scan::default();
    };

    let now = SystemTime::now();
    let mut items: Vec<(SystemTime, u64, Entry)> = read_dir
        .filter_map(Result::ok)
        .filter_map(|dirent| {
            let file_name = dirent.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') || file_name.to_lowercase().ends_with(".tmp") {
                return None;
            }
            let path = dirent.path();
            // Follows symlinks, so broken ones are skipped.
            let meta = fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?;

            let (name, progress) = match strip_partial(&file_name) {
                Some(final_name) => {
                    let stalled = now
                        .duration_since(modified)
                        .is_ok_and(|idle| idle > STALLED_AFTER);
                    let progress = Progress {
                        received: meta.len(),
                        total: None,
                        state: if stalled {
                            State::Stalled
                        } else {
                            State::Active
                        },
                    };
                    (final_name.to_owned(), Some(progress))
                }
                None => (file_name, None),
            };

            let icon_names = icon_names(Path::new(&name), meta.is_dir());
            let picture = (progress.is_none() && meta.is_file() && thumbnails::supported(&path))
                .then_some(thumbnails::Stamp {
                    len: meta.len(),
                    modified,
                });
            Some((
                modified,
                meta.len(),
                Entry {
                    path,
                    name,
                    icon_names,
                    progress,
                    picture,
                },
            ))
        })
        .collect();

    // Firefox creates an empty file under the final name next to the `.part`; show only one.
    let downloading: HashSet<String> = items
        .iter()
        .filter(|(_, _, entry)| entry.progress.is_some())
        .map(|(_, _, entry)| entry.name.clone())
        .collect();
    items.retain(|(_, len, entry)| {
        entry.progress.is_some() || *len > 0 || !downloading.contains(&entry.name)
    });

    items.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    let (mut downloading, mut files): (Vec<Entry>, Vec<Entry>) = items
        .into_iter()
        .map(|(_, _, entry)| entry)
        .partition(|entry| entry.progress.is_some());
    files.truncate(limit);

    add_browser_totals(&mut downloading);
    Scan { files, downloading }
}

/// Fills in the expected size of Chromium downloads, and their real name ("Unconfirmed
/// 123456.crdownload" becomes the file's name).
fn add_browser_totals(entries: &mut [Entry]) {
    let has_crdownload = entries.iter().any(|entry| {
        entry.progress.is_some()
            && entry
                .path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("crdownload"))
    });
    if !has_crdownload {
        return;
    }

    let downloads = chromium::unfinished();
    for entry in entries.iter_mut() {
        let Some(download) = find_download(&downloads, &entry.path) else {
            continue;
        };
        let Some(progress) = &mut entry.progress else {
            continue;
        };
        progress.total = download.total;
        if download.interrupted {
            progress.state = State::Interrupted;
        }
        if let Some(name) = download.target_path.file_name() {
            entry.name = name.to_string_lossy().into_owned();
            entry.icon_names = icon_names(Path::new(&entry.name), false);
        }
    }
}

/// The browser's record for the partial file at `path`. The browser may have stored the real path
/// of a symlinked Downloads folder, so resolved paths are compared too, then the file name alone.
fn find_download<'a>(
    downloads: &'a HashMap<PathBuf, chromium::Download>,
    path: &Path,
) -> Option<&'a chromium::Download> {
    if let Some(download) = downloads.get(path) {
        return Some(download);
    }
    if let Ok(real) = fs::canonicalize(path)
        && let Some(download) = downloads.get(&real)
    {
        return Some(download);
    }
    let name = path.file_name()?;
    let mut by_name = downloads
        .values()
        .filter(|download| download.current_path.file_name() == Some(name));
    // Only when the name is unambiguous.
    match (by_name.next(), by_name.next()) {
        (Some(download), None) => Some(download),
        _ => None,
    }
}

/// The name a partial download will have once it's done, if `name` is one.
fn strip_partial(name: &str) -> Option<&str> {
    let lower = name.to_lowercase();
    PARTIAL.iter().find_map(|suffix| {
        lower
            .ends_with(suffix)
            .then(|| &name[..name.len() - suffix.len()])
            .filter(|stem| !stem.is_empty())
    })
}

fn icon_names(path: &Path, is_dir: bool) -> Vec<String> {
    if is_dir {
        return vec!["folder".into()];
    }
    let mut names = Vec::new();
    if let Some(mime) = mime_guess::from_path(path).first() {
        names.push(mime.essence_str().replace('/', "-"));
        names.push(format!("{}-x-generic", mime.type_()));
    }
    names.push("text-x-generic".into());
    names
}

/// "512 B", "12.3 kB", "4.5 MB", "1.2 GB": decimal units, like COSMIC Files.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut size = bytes as f64 / 1000.0;
    let mut unit = 0;
    while size >= 1000.0 && unit < UNITS.len() - 1 {
        size /= 1000.0;
        unit += 1;
    }
    if size >= 100.0 {
        format!("{size:.0} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_names_are_stripped() {
        assert_eq!(strip_partial("movie.mkv.part"), Some("movie.mkv"));
        assert_eq!(strip_partial("setup.EXE.crdownload"), Some("setup.EXE"));
        assert_eq!(
            strip_partial("Unconfirmed 12345.crdownload"),
            Some("Unconfirmed 12345")
        );
        assert_eq!(strip_partial(".part"), None);
        assert_eq!(strip_partial("notes.txt"), None);
    }

    #[test]
    fn records_are_found_through_symlinks_and_by_name() {
        let record = |current: &str| chromium::Download {
            current_path: PathBuf::from(current),
            target_path: PathBuf::from("/real/Downloads/a.iso"),
            total: Some(10),
            interrupted: false,
        };
        let mut downloads = HashMap::new();
        downloads.insert(
            PathBuf::from("/real/Downloads/a.iso.crdownload"),
            record("/real/Downloads/a.iso.crdownload"),
        );

        // Seen through a symlinked folder that doesn't resolve here: matched by name.
        let seen = Path::new("/home/me/Downloads/a.iso.crdownload");
        assert_eq!(find_download(&downloads, seen).unwrap().total, Some(10));

        // Two records with the same name: no guess.
        downloads.insert(
            PathBuf::from("/other/a.iso.crdownload"),
            record("/other/a.iso.crdownload"),
        );
        assert!(find_download(&downloads, seen).is_none());
    }

    #[test]
    fn mime_icon_comes_first() {
        let names = icon_names(Path::new("photo.png"), false);
        assert_eq!(names[0], "image-png");
        assert_eq!(names[1], "image-x-generic");
        assert_eq!(names.last().unwrap(), "text-x-generic");
        assert_eq!(icon_names(Path::new("stuff"), true), ["folder"]);
    }

    #[test]
    fn sizes_are_readable() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(12_345), "12.3 kB");
        assert_eq!(format_size(4_500_000), "4.5 MB");
        assert_eq!(format_size(250_000_000), "250 MB");
        assert_eq!(format_size(1_234_000_000), "1.2 GB");
    }

    #[test]
    fn firefox_placeholder_is_hidden() {
        let dir = std::env::temp_dir().join(format!("ds-recent-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("big.iso"), b"").unwrap();
        fs::write(dir.join("big.iso.part"), vec![0u8; 2048]).unwrap();
        fs::write(dir.join("done.txt"), b"hi").unwrap();

        let Scan { files, downloading } = scan(&dir, 10);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(downloading.len(), 1);
        let big = &downloading[0];
        assert_eq!(big.name, "big.iso");
        let progress = big.progress.unwrap();
        assert_eq!(progress.received, 2048);
        assert_eq!(progress.total, None);
        assert_eq!(progress.state, State::Active);
        assert_eq!(big.icon_names, icon_names(Path::new("big.iso"), false));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "done.txt");
        assert!(files[0].progress.is_none());
    }
}
