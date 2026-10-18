// SPDX-License-Identifier: GPL-3.0-only

//! Icon lookup.
//!
//! 1. **System icon theme.** The one picked in COSMIC Settings. libcosmic reads it from the
//!    `com.system76.CosmicTk` config at startup and follows changes while the applet runs.
//! 2. **Built-in icons.** A few SVGs compiled into the binary (`resources/icons/`). They are used
//!    only when the theme has none of the names we ask for, for example inside a Flatpak
//!    sandbox that can't see the host's theme.
//!
//! libcosmic's own `icon::from_name(..).handle()` silently returns an *empty* image on a miss
//! (its built-in bundle is disabled on Linux), so [`resolve`] looks the file up itself.

use cosmic::widget::icon;
use std::path::{Path, PathBuf};

const GENERIC: &[u8] = include_bytes!("../resources/icons/text-x-generic.svg");

/// Icons embedded in the binary, by freedesktop icon name.
/// Names ending in `-symbolic` are tinted to the theme's text colour by libcosmic.
const BUILT_IN: &[(&str, &[u8])] = &[
    (
        "folder-download-symbolic",
        include_bytes!("../resources/icons/folder-download-symbolic.svg"),
    ),
    (
        "folder-open-symbolic",
        include_bytes!("../resources/icons/folder-open-symbolic.svg"),
    ),
    ("folder", include_bytes!("../resources/icons/folder.svg")),
    ("text-x-generic", GENERIC),
    (
        "image-x-generic",
        include_bytes!("../resources/icons/image-x-generic.svg"),
    ),
    (
        "audio-x-generic",
        include_bytes!("../resources/icons/audio-x-generic.svg"),
    ),
    (
        "video-x-generic",
        include_bytes!("../resources/icons/video-x-generic.svg"),
    ),
    (
        "application-x-generic",
        include_bytes!("../resources/icons/application-x-generic.svg"),
    ),
];

/// Call first thing in `main`, before any other thread starts and before the first icon lookup.
///
/// Flatpak shows the host's icon themes inside the sandbox at `/run/host/share/icons` (system)
/// and `/run/host/user-share/icons` (per-user). The icon lookup only searches
/// `$XDG_DATA_DIRS/icons`, so make sure both are listed. Outside a Flatpak this does nothing.
pub fn init() {
    const HOST_DATA_DIRS: [&str; 2] = ["/run/host/share", "/run/host/user-share"];

    if !Path::new("/.flatpak-info").exists() {
        return;
    }

    let mut dirs: Vec<PathBuf> = std::env::var_os("XDG_DATA_DIRS")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    if dirs.is_empty() {
        // Same defaults as the XDG spec, so the runtime's own icons stay searchable.
        dirs = vec!["/usr/local/share".into(), "/usr/share".into()];
    }

    let mut changed = false;
    for dir in HOST_DATA_DIRS.map(PathBuf::from) {
        if dir.join("icons").is_dir() && !dirs.contains(&dir) {
            dirs.push(dir);
            changed = true;
        }
    }

    if changed && let Ok(joined) = std::env::join_paths(&dirs) {
        // SAFETY: called at the very start of `main`, so no other thread can be reading the
        // environment yet.
        unsafe { std::env::set_var("XDG_DATA_DIRS", joined) };
    }
}

/// Finds an icon for the first of `names` that exists. Put the most specific name first.
///
/// The whole list is tried against the system theme before the built-in set, so the system
/// theme always wins when it has *any* of the names.
pub fn resolve<S: AsRef<str>>(names: &[S], size: u16) -> icon::Handle {
    if let Some((first, rest)) = names.split_first() {
        let found = icon::from_name(first.as_ref().to_owned())
            .fallback(Some(icon::IconFallback::Names(
                rest.iter()
                    .map(|name| name.as_ref().to_owned().into())
                    .collect(),
            )))
            .size(size)
            .path();

        if let Some(path) = found {
            // Takes `symbolic` from the file that was actually found, not the name we asked for.
            return icon::from_path(path);
        }
    }

    names
        .iter()
        .find_map(|name| built_in(name.as_ref()))
        .unwrap_or_else(|| icon::from_svg_bytes(GENERIC))
}

fn built_in(name: &str) -> Option<icon::Handle> {
    BUILT_IN
        .iter()
        .find(|(built_in_name, _)| *built_in_name == name)
        .map(|(built_in_name, svg)| {
            icon::from_svg_bytes(*svg).symbolic(built_in_name.ends_with("-symbolic"))
        })
}
