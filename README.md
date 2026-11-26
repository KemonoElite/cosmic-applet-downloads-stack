# Downloads Stack

A COSMIC panel applet that fans out your most recent downloads, like a Dock stack on macOS.
Click a file to open it, or **Open in Files** to open the whole folder.

- Newest file sits closest to the panel (bottom panel: newest at the bottom, like the Dock)
- Downloads in progress (`.part`, `.crdownload`, ...) show a progress bar under their name,
  updated every second while the stack is open
- Hides dotfiles
- Re-scans the folder every time it opens
- Opened apps get focus, through a Wayland activation token like the stock applets

It's laid out like a crate from [pop-os/cosmic-applets](https://github.com/pop-os/cosmic-applets):
`lib.rs` exposes `run()`, strings live in Fluent files under `i18n/`, the desktop entry template
and icon live in `data/`, `build.rs` writes the translated desktop entry with `xdgen`, and the
`justfile` installs everything.

## Install on NixOS

In your system flake:

```nix
{
  inputs.cosmic-applet-downloads-stack.url = "github:KemonoElite/cosmic-applet-downloads-stack";

  outputs = { nixpkgs, cosmic-applet-downloads-stack, ... }: {
    nixosConfigurations.<host> = nixpkgs.lib.nixosSystem {
      modules = [
        cosmic-applet-downloads-stack.nixosModules.default
        { programs.cosmic-applet-downloads-stack.enable = true; }
      ];
    };
  };
}
```

Then add **Downloads Stack** under COSMIC Settings → Desktop → Panel → Configure panel applets.
Log out and back in if it doesn't show up, so the panel picks up the new desktop entry.

Other ways in:

- `overlays.default` adds `pkgs.cosmic-applet-downloads-stack`
- `nix run github:KemonoElite/cosmic-applet-downloads-stack` (run it from a COSMIC session)
- `programs.cosmic-applet-downloads-stack.package` swaps in another build

The flake uses [crane](https://github.com/ipetkov/crane) with the stable Rust toolchain from
[fenix](https://github.com/nix-community/fenix), the same as cosmic-applets. It asks to use the
nix-community cache for that toolchain; accepting saves compiling Rust.

## Download progress

A partial file doesn't say how big the finished download will be, so where the total comes from
depends on the browser:

- **Brave, Chrome, Chromium, Vivaldi, Edge, Opera** (`.crdownload`): the browser records the
  expected size in its `History` database. The applet copies that database to
  `$XDG_RUNTIME_DIR/cosmic-applet-downloads-stack/`, reads only the unfinished downloads (path,
  expected size, state), and deletes the copy. The bar fills up and shows "4.5 MB of 12 MB". The
  browser saves to that database every few seconds, so the total can take a moment to appear.
- **Firefox** (`.part`) and others: the total isn't recorded anywhere the applet can read, so the
  bar animates and the size counts up.

A download that hasn't grown for 30 seconds is marked **Stalled**; one the browser reports as
failed or paused is marked **Interrupted**. Clicking a download in progress opens the Downloads
folder.

## Develop

```sh
nix develop        # toolchain, rust-analyzer, just, and the runtime libraries
just               # cargo build --release
just check         # clippy, pedantic
just run           # from a COSMIC session
nix flake check    # package, clippy and unit tests
```

## Translate

Copy `i18n/en/cosmic_applet_downloads_stack.ftl` to `i18n/<lang>/` and translate it. The same file
holds the desktop entry's name, comment and keywords, so `build.rs` picks those up too.

## Tweak

In `src/lib.rs`: `MAX_ITEMS` (how many files), `LABEL_MAX_WIDTH`, `ARC_DEPTH` (curve of the fan),
and the fan's spacing (`ROW_SPACING`, `PILL_PADDING`, ...).

## Icons

All icons come from your icon theme and follow it live. File icons use the freedesktop mime names
(`image-png`, then `image-x-generic`, then `text-x-generic`); if the theme has none of those, a
symbolic generic icon is used so a row is never blank.

On NixOS the panel doesn't always pass the profile folders that hold icon themes on to applets,
which hides themes installed with `users.users.<name>.packages` or Home Manager. The applet adds
those folders itself on startup (`src/icons.rs`), so the theme picked in COSMIC Settings is used.
