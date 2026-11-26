{
  description = "Downloads Stack: your most recent downloads, fanned out from the COSMIC panel";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nix-filter.url = "github:numtide/nix-filter";
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      nix-filter,
      crane,
      fenix,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) lib;

        craneLib = (crane.mkLib pkgs).overrideToolchain fenix.packages.${system}.stable.toolchain;

        pkgDef = {
          inherit (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; }) pname version;

          # Only what the build reads, so editing the README or this file doesn't rebuild it.
          src = nix-filter.lib.filter {
            root = ./.;
            include = [
              ./Cargo.toml
              ./Cargo.lock
              ./build.rs
              ./i18n.toml
              ./justfile
              ./data
              ./i18n
              ./src
            ];
          };

          strictDeps = true;
          nativeBuildInputs = with pkgs; [
            just
            pkg-config
            autoPatchelfHook
            makeBinaryWrapper
          ];
          buildInputs = with pkgs; [
            libxkbcommon
            libglvnd # For libEGL
            wayland
          ];
          # Loaded at runtime with dlopen, so autoPatchelfHook adds them to the RPATH.
          runtimeDependencies = with pkgs; [
            wayland
            libglvnd
            libxkbcommon
            vulkan-loader
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly pkgDef;

        # Built and installed through the justfile, the same way pop-os/cosmic-applets does it.
        cosmic-applet-downloads-stack = craneLib.buildPackage (
          pkgDef
          // {
            inherit cargoArtifacts;
            buildPhaseCargoCommand = "just build-release --locked";
            installPhaseCommand = "just prefix=$out install";
            doNotPostBuildInstallCargoBinaries = true;
            # The unit tests run in `checks` instead.
            doCheck = false;

            # Files and folders are opened with `xdg-open`. Appended, so the system's copy wins.
            postFixup = ''
              wrapProgram $out/bin/${pkgDef.pname} \
                --suffix PATH : ${lib.makeBinPath [ pkgs.xdg-utils ]}
            '';

            meta = {
              description = "Your most recent downloads, fanned out from the COSMIC panel";
              homepage = "https://github.com/KemonoElite/cosmic-applet-downloads-stack";
              license = lib.licenses.gpl3Only;
              mainProgram = pkgDef.pname;
              platforms = lib.platforms.linux;
            };
          }
        );
      in
      {
        checks = {
          inherit cosmic-applet-downloads-stack;

          clippy = craneLib.cargoClippy (
            pkgDef
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          tests = craneLib.cargoTest (pkgDef // { inherit cargoArtifacts; });
        };

        packages = {
          inherit cosmic-applet-downloads-stack;
          default = cosmic-applet-downloads-stack;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = cosmic-applet-downloads-stack;
        };

        devShells.default = craneLib.devShell {
          inputsFrom = [ cosmic-applet-downloads-stack ];
          packages = [ fenix.packages.${system}.stable.rust-analyzer ];
          LD_LIBRARY_PATH = lib.makeLibraryPath pkgDef.runtimeDependencies;
        };

      }
    )
    // {
      # pkgs.cosmic-applet-downloads-stack
      overlays.default = final: _prev: {
        cosmic-applet-downloads-stack = self.packages.${final.stdenv.hostPlatform.system}.default;
      };

      # programs.cosmic-applet-downloads-stack.enable = true;
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          cfg = config.programs.cosmic-applet-downloads-stack;
        in
        {
          options.programs.cosmic-applet-downloads-stack = {
            enable = lib.mkEnableOption "the Downloads Stack applet for the COSMIC panel";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              defaultText = lib.literalExpression "cosmic-applet-downloads-stack.packages.\${pkgs.stdenv.hostPlatform.system}.default";
              description = "The Downloads Stack package to use.";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [ cfg.package ];
          };
        };
    };

  nixConfig = {
    # Cache for the Rust toolchain in fenix
    extra-substituters = [ "https://nix-community.cachix.org" ];
    extra-trusted-public-keys = [
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
  };
}
