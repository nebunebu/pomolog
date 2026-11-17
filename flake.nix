{
  description = "Pomolog: pomodoro timer, kanban, calendar, habits and analytics";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      runtimeLibs =
        pkgs: with pkgs; [
          wayland
          libxkbcommon
          vulkan-loader
          libGL
          alsa-lib
        ];

      mkPomolog =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "pomolog";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                base = baseNameOf path;
              in
              !(builtins.elem base [
                "target"
                "result"
                "flake.nix"
                "flake.lock"
                "nix"
              ]);
          };
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [
            pkgs.dbus
            pkgs.sqlite
          ]
          ++ runtimeLibs pkgs;

          postInstall = ''
            install -Dm644 assets/pomolog.desktop $out/share/applications/pomolog.desktop
            install -Dm644 assets/icon.svg $out/share/icons/hicolor/scalable/apps/pomolog.svg
            mkdir -p $out/share/pomolog
            cp -r assets/themes $out/share/pomolog/themes
          '';

          postFixup = ''
            patchelf --add-rpath ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)} $out/bin/pomolog
          '';

          meta = {
            description = "Pomodoro timer with kanban, calendar, habits and analytics";
            license = pkgs.lib.licenses.mit;
            mainProgram = "pomolog";
            platforms = pkgs.lib.platforms.linux;
          };
        };
    in
    {
      packages = forAllSystems (pkgs: rec {
        pomolog = mkPomolog pkgs;
        default = pomolog;
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/pomolog";
        };
      });

      overlays.default = final: _prev: { pomolog = mkPomolog final; };

      homeManagerModules.default = import ./nix/hm-module.nix self;

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            pkg-config
          ];
          buildInputs = [
            pkgs.dbus
            pkgs.sqlite
          ]
          ++ runtimeLibs pkgs;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };
      });

      checks = forAllSystems (pkgs: {
        package = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
