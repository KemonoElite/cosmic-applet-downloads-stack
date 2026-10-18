// SPDX-License-Identifier: GPL-3.0-only

//! Makes the applet use the same icon theme as the rest of the system.
//!
//! Two things are needed, and neither happens by itself inside a Flatpak:
//!
//! 1. The theme's files must be on the icon search path. Inside the sandbox
//!    `XDG_DATA_DIRS` is just `/app/share:/usr/share`, so the host's themes are never
//!    found. Flatpak mounts the host's `/usr/share/icons` at `/run/host/share/icons` and
//!    `~/.local/share/icons` at `/run/host/user-share/icons`, and the manifest grants read
//!    access to NixOS's theme folders. [`expose_host_icon_themes`] adds whichever of those
//!    exist to `XDG_DATA_DIRS`.
//! 2. The theme *name* must be applied. libcosmic's applet startup begins on "Cosmic" and
//!    only switches if its settings watcher later delivers an update.
//!    [`apply_system_icon_theme`] reads COSMIC's setting directly and applies it up front.
//!
//! Both must run before anything looks up an icon, and before any threads start.

use std::env;
use std::path::PathBuf;

pub fn expose_host_icon_themes() {
    let mut dirs: Vec<PathBuf> = match env::var_os("XDG_DATA_DIRS") {
        Some(value) => env::split_paths(&value).collect(),
        None => vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")],
    };

    let mut candidates: Vec<PathBuf> = vec![
        // Mounted by Flatpak itself when the host has these folders.
        PathBuf::from("/run/host/share"),
        PathBuf::from("/run/host/user-share"),
        // NixOS system profile (the manifest grants read access).
        PathBuf::from("/run/current-system/sw/share"),
    ];
    if let Some(home) = env::var_os("HOME") {
        // home-manager / `nix profile` installs.
        candidates.push(PathBuf::from(home).join(".nix-profile/share"));
    }

    let mut changed = false;
    for candidate in candidates {
        if candidate.join("icons").is_dir() && !dirs.contains(&candidate) {
            dirs.push(candidate);
            changed = true;
        }
    }

    if changed {
        if let Ok(joined) = env::join_paths(&dirs) {
            // SAFETY: called first thing in `main`, before any other thread exists.
            unsafe { env::set_var("XDG_DATA_DIRS", joined) };
        }
    }
    eprintln!(
        "downloads-stack: XDG_DATA_DIRS = {:?}",
        env::var_os("XDG_DATA_DIRS")
    );
}

/// Reads the icon theme COSMIC is set to use and makes it this applet's icon theme.
pub fn apply_system_icon_theme() {
    let config_home = if env::var_os("FLATPAK_ID").is_some() {
        // In a Flatpak, XDG_CONFIG_HOME is the app's private folder. The host's config
        // (readable through `--filesystem=xdg-config/cosmic`) is at HOST_XDG_CONFIG_HOME
        // or ~/.config.
        env::var_os("HOST_XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
    } else {
        dirs::config_dir()
    };

    let Some(config_home) = config_home else {
        return;
    };
    let path = config_home.join("cosmic/com.system76.CosmicTk/v1/icon_theme");

    // The file holds a quoted string, e.g. "WhiteSur".
    let theme = std::fs::read_to_string(&path)
        .map(|contents| contents.trim().trim_matches('"').trim().to_owned())
        .unwrap_or_default();

    eprintln!("downloads-stack: icon theme from {path:?} = {theme:?}");
    if !theme.is_empty() {
        cosmic::icon_theme::set_default(theme);
    }
}
