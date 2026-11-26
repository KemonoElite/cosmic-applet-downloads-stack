// SPDX-License-Identifier: GPL-3.0-only

//! Makes icon themes from the NixOS profiles visible.
//!
//! Icons are looked up under `$XDG_DATA_DIRS/icons`, and that variable is read once, at the first
//! lookup. NixOS keeps icon themes in the system and per-user profiles, and the panel doesn't
//! always pass all of them on to applets. A theme installed through `users.users.<name>.packages`
//! or Home Manager is then invisible, and the applet falls back to the COSMIC theme instead of
//! the one picked in COSMIC Settings.

use std::path::{Path, PathBuf};

/// Adds the NixOS profile `share` folders that have icons to `XDG_DATA_DIRS`.
///
/// Call it first thing, before any other thread starts and before the first icon lookup. Folders
/// that are missing, have no `icons` folder or are already listed are skipped, and on other
/// systems nothing changes.
pub fn add_nixos_profile_dirs() {
    if !Path::new("/run/current-system/sw").exists() {
        return;
    }

    let mut extra: Vec<PathBuf> = vec!["/run/current-system/sw/share".into()];
    if let Some(user) = std::env::var_os("USER") {
        extra.push(Path::new("/etc/profiles/per-user").join(user).join("share"));
    }
    if let Some(home) = dirs::home_dir() {
        extra.push(home.join(".nix-profile/share"));
        extra.push(home.join(".local/state/nix/profile/share"));
    }
    extra.push("/nix/var/nix/profiles/default/share".into());

    let mut data_dirs: Vec<PathBuf> = std::env::var_os("XDG_DATA_DIRS")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    if data_dirs.is_empty() {
        // The XDG defaults, so they stay searchable once the variable is set.
        data_dirs = vec!["/usr/local/share".into(), "/usr/share".into()];
    }

    let mut changed = false;
    for dir in extra {
        if dir.join("icons").is_dir() && !data_dirs.contains(&dir) {
            data_dirs.push(dir);
            changed = true;
        }
    }

    if changed && let Ok(joined) = std::env::join_paths(&data_dirs) {
        // SAFETY: runs at the very start of the applet, before any other thread exists.
        unsafe { std::env::set_var("XDG_DATA_DIRS", joined) };
    }
}
