// SPDX-License-Identifier: GPL-3.0-only

//! Makes the host's icon themes visible to libcosmic's icon lookup.
//!
//! Inside a Flatpak sandbox `XDG_DATA_DIRS` is just `/app/share:/usr/share`, so the icon
//! theme COSMIC is set to use (which lives on the host) is never found. Flatpak mounts the
//! host's `/usr/share/icons` at `/run/host/share/icons` and `~/.local/share/icons` at
//! `/run/host/user-share/icons`, and the manifest grants read access to NixOS's theme
//! folders. This adds whichever of those exist to `XDG_DATA_DIRS`, before libcosmic does
//! its first icon lookup. libcosmic itself already reads which theme is selected from
//! COSMIC's settings, so following theme changes needs nothing more.

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
}
