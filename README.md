# Downloads Stack: a COSMIC applet

A panel button that opens a macOS Dock-style "fan" of your most recent downloads.
Click a file to open it, or "Open in Files" to open the whole folder.

- Newest file sits closest to the panel (bottom panel: newest at the bottom, like the Dock)
- Hides dotfiles and in-progress downloads (`.part`, `.crdownload`, ...)
- Re-scans the folder every time you open it

## Host it as a Flatpak repo (GitHub Pages)

1. Put your GitHub username in the project:

       sh scripts/set-owner.sh YOUR_GITHUB_USERNAME [repo-name]

   (default repo name is `cosmic-applet-downloads-stack`)

2. Create that repo on GitHub and push this folder to the `main` branch.
3. In the repo: **Settings → Actions → General → Workflow permissions → Read and write**.
4. Let the **Flatpak repo** workflow finish (Actions tab, about 15-30 min the first time).
5. **Settings → Pages → Source: Deploy from a branch → `gh-pages` / `(root)`**.

Every push to `main` rebuilds and republishes the repo.

### Add the repo and install

Replace `USER` and `REPO`; USER is lowercase.

    flatpak remote-add --user --if-not-exists --no-gpg-verify downloads-stack https://USER.github.io/REPO/repo/
    flatpak install --user downloads-stack io.github.USER.DownloadsStack

Updates: `flatpak update --user`.

Then add **Downloads Stack** under COSMIC Settings → Desktop → Panel → Configure panel applets.
Log out and back in if it doesn't appear, so the panel picks up the new desktop entry.

### Optional: sign the repo (no `--no-gpg-verify`)

    sh scripts/make-signing-key.sh

Save the printed key as a repo secret named `FLATPAK_GPG_PRIVATE_KEY`, re-run the workflow, then:

    flatpak remote-add --user --if-not-exists downloads-stack https://USER.github.io/REPO/downloads-stack.flatpakrepo

## Test locally first

    flatpak install --user flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
    just flatpak

Needs `flatpak`, `flatpak-builder` and `just`. libcosmic needs a recent Rust (1.93+); the
`rust-stable` extension tracks the latest stable.

## Icons

The panel button and the file-type icons come from **your system icon theme** (the one set in
COSMIC Settings). The applet follows it live, so switching themes updates the icons without a restart.

- **Flatpak:** the sandbox can't see the host's `/usr`, but Flatpak exposes host icon themes at
  `/run/host/share/icons` and `/run/host/user-share/icons`. The applet adds both to `XDG_DATA_DIRS` on
  startup (`icons::init` in `src/icons.rs`), so no extra permission is needed.
- **Built in:** a small set of SVGs in `resources/icons/` is compiled into the binary. They are used
  only when the theme has none of the icons the applet asks for, so an icon is never blank.
  File names are freedesktop icon names; edit an SVG and rebuild to change it.

## Sandbox permissions

`--socket=wayland`, `--device=dri` (rendering), `--filesystem=xdg-download` (read your downloads),
`xdg-config/cosmic:ro` + `xdg-config/user-dirs.dirs:ro` (theme and folder location),
and the COSMIC settings daemon on D-Bus. Files open through the desktop portal.

## Troubleshooting

- **Wrong colors**: change `xdg-config/cosmic:ro` to `:rw` in the manifest (Minimon uses `rw`).
- **Icons don't match the desktop (Flatpak)**: the sandbox only reaches themes installed in the host's
  `/usr/share/icons` or `~/.local/share/icons`. A theme installed elsewhere, or symlinked out of those
  folders, is invisible to it and the built-in icons are shown instead.
- **"Downloads folder not found"**: check `flatpak info --show-permissions io.github.USER.DownloadsStack`.
- **Logs**: `journalctl -f` while clicking the applet.
- Building for Flathub itself needs offline sources (`flatpak-cargo-generator` + `Cargo.lock`)
  instead of `--share=network` in the manifest.

## Native build (no Flatpak)

    just build-release
    sudo just install

## Tweak

In `src/app.rs`: `MAX_ITEMS` (how many files), `MAX_NAME_CHARS`, `ARC_DEPTH` (curve of the fan).
In `src/icons.rs`: the list of built-in icons.

## Flatpak

f=~/.local/share/flatpak/exports/share/applications/io.github.yourname.DownloadsStack.desktop
cp -L "$f" /tmp/ds.desktop
printf '\nX-CosmicApplet=true\nX-CosmicHoverPopup=Auto\n' >> /tmp/ds.desktop
rm "$f" && cp /tmp/ds.desktop "$f"
