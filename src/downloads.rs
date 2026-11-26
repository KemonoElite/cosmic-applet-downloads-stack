// SPDX-License-Identifier: GPL-3.0-only

//! Reading the Downloads folder. Nothing here touches the UI, so it runs on a blocking thread.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// One item in the stack.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// Freedesktop icon names to try in order: the specific mime icon first, generic last.
    pub icon_names: Vec<String>,
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

/// The `limit` most recently modified items in `dir`, newest first.
///
/// Hidden files and downloads that are still in progress are skipped.
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
            // Follows symlinks, so broken ones are skipped.
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

    items.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    items.truncate(limit);
    items.into_iter().map(|(_, entry)| entry).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_downloads_are_hidden() {
        assert!(is_partial("movie.mkv.part"));
        assert!(is_partial("setup.EXE.crdownload"));
        assert!(!is_partial("notes.txt"));
    }

    #[test]
    fn mime_icon_comes_first() {
        let names = icon_names(Path::new("photo.png"), false);
        assert_eq!(names[0], "image-png");
        assert_eq!(names[1], "image-x-generic");
        assert_eq!(names.last().unwrap(), "text-x-generic");
        assert_eq!(icon_names(Path::new("stuff"), true), ["folder"]);
    }
}
