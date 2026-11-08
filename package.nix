{
  lib,
  rustPlatform,
  libcosmicAppHook,
  xdg-utils,
}:

let
  cargoToml = lib.importTOML ./Cargo.toml;
  appId = "io.github.yourname.DownloadsStack";
in
rustPlatform.buildRustPackage {
  pname = cargoToml.package.name;
  inherit (cargoToml.package) version;

  # Only the files the build needs, so editing flake.nix or the README doesn't trigger a rebuild.
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./resources
    ];
  };

  # libcosmic and friends are git dependencies. `allowBuiltinFetchGit` lets Nix fetch them
  # at the exact revisions pinned in Cargo.lock, so no output hashes are needed.
  #
  # If your Nix refuses that, use `cargoHash = lib.fakeHash;` instead, build once, and
  # copy the hash from the error message.
  cargoLock = {
    lockFile = ./Cargo.lock;
    allowBuiltinFetchGit = true;
  };

  # Provides pkg-config, wayland/xkbcommon/vulkan/EGL linking and runtime wrapping.
  nativeBuildInputs = [ libcosmicAppHook ];

  # `cargo test` would build libcosmic a second time.
  doCheck = false;

  # The repo's justfile installs to a fixed `target/release` path, which doesn't match Nix's
  # per-target directory. cargoInstallHook already installs the binary, so just add the rest.
  postInstall = ''
    install -Dm0644 resources/${appId}.desktop -t $out/share/applications
    install -Dm0644 resources/${appId}.svg -t $out/share/icons/hicolor/scalable/apps
    install -Dm0644 resources/${appId}.metainfo.xml -t $out/share/metainfo
  '';

  # The applet opens files and folders by running `xdg-open`.
  preFixup = ''
    libcosmicAppWrapperArgs+=(--prefix PATH : ${lib.makeBinPath [ xdg-utils ]})
  '';

  meta = {
    description = cargoToml.package.description;
    homepage = "https://github.com/KemonoElite/cosmic-applet-downloads-stack";
    license = lib.licenses.gpl3Only;
    mainProgram = cargoToml.package.name;
    platforms = lib.platforms.linux;
  };
}
