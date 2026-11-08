{
  description = "Downloads Stack: macOS-style fan view of your Downloads folder for the COSMIC panel";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        cosmic-applet-downloads-stack = pkgs.callPackage ./package.nix { };
        default = cosmic-applet-downloads-stack;
      });

      # pkgs.cosmic-applet-downloads-stack, built against *your* nixpkgs.
      overlays.default = final: _prev: {
        cosmic-applet-downloads-stack = final.callPackage ./package.nix { };
      };

      # Adds the applet to the system when `programs.cosmic-applet-downloads-stack.enable = true`.
      nixosModules.default =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        {
          options.programs.cosmic-applet-downloads-stack.enable = lib.mkEnableOption "the Downloads Stack applet for the COSMIC panel";

          config = lib.mkIf config.programs.cosmic-applet-downloads-stack.enable {
            environment.systemPackages = [ (pkgs.callPackage ./package.nix { }) ];
          };
        };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
        };
      });
    };
}
