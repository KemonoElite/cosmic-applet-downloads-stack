// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One item shown in the fan.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// Icon names to try in order (specific mime icon first, generic last).
    pub icon_names: Vec<String>,
}

/// Finds the Downloads folder. Inside a Flatpak the sandbox only sees it when it has
/// `--filesystem=xdg-download`, and `user-dirs.dirs` may be missing, hence the fallbacks.
pub fn downloads_dir() -> Option<PathBuf> {
    [
        std::env::var_os("XDG_DOWNLOAD_DIR").map(PathBuf::from),
        dirs::download_dir(),
        dirs::home_dir().map(|home| home.join("Downloads")),
    ]
    .into_iter()
    .flatten()
    .find(|path| path.is_dir())
}

/// Browsers write these while a download is still in progress.
fn is_partial(name: &str) -> bool {
    const PARTIAL: [&str; 5] = [".part", ".crdownload", ".download", ".opdownload", ".tmp"];
    let lower = name.to_lowercase();
    PARTIAL.iter().any(|ext| lower.ends_with(ext))
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

/// The `limit` most recently modified items in `dir`, newest first.
pub fn recent(dir: &Path, limit: usize) -> Vec<Entry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut items: Vec<(SystemTime, Entry)> = read_dir
        .filter_map(Result::ok)
        .filter_map(|dirent| {
            let name = dirent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || is_partial(&name) {
                return None;
            }
            let path = dirent.path();
            // `fs::metadata` follows symlinks and skips broken ones.
            let meta = fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?;
            let icon_names = icon_names(&path, meta.is_dir());
            Some((
                modified,
                Entry {
                    path,
                    name,
                    icon_names,
                },
            ))
        })
        .collect();

    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.truncate(limit);
    items.into_iter().map(|(_, entry)| entry).collect()
}

/// Shortens long names in the middle so the extension stays visible.
pub fn shorten(name: &str, max_chars: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max_chars {
        return name.to_owned();
    }
    let keep = max_chars.saturating_sub(1);
    let tail = keep / 3;
    let head = keep - tail;
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}
