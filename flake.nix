{
  description = "marcterm - A terminal emulator built with Freya and Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        rpathLibs = with pkgs; [
          libGL
          libxkbcommon
          wayland
          libx11
          libxcursor
          libxrandr
          libxi
          libxcb
          fontconfig
          freetype
          stdenv.cc.cc.lib
        ];

        marcterm = pkgs.rustPlatform.buildRustPackage {
          pname = "marcterm";
          version = "0.1.14";

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
            outputHashes = {
              "freya-0.4.0-rc.17" = "sha256-gNmO7Pq2gXqLJbJFIKNrxVQGV1O4KpYVfb7CY2eD3NA=";
            };
          };

          # Skia downloads sources during build, needs network access
          __noChroot = true;

          doCheck = false;
          auditable = false;
          dontStrip = true;

          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
            python3
            makeWrapper
            curl
            cacert
            git
          ];

          buildInputs = with pkgs; [
            fontconfig
            freetype
            libxkbcommon
            libGL
            wayland
            libx11
            libxcursor
            libxrandr
            libxi
            libxcb
            systemd
          ];

          # Required for Skia download during build
          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          GIT_SSL_CAINFO = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

          postInstall = ''
            install -Dm644 flatpak/io.marc.term.desktop \
              $out/share/applications/io.marc.term.desktop
            install -Dm644 icon.png \
              $out/share/icons/hicolor/128x128/apps/io.marc.term.png
            install -Dm644 flatpak/io.marc.term.metainfo.xml \
              $out/share/metainfo/io.marc.term.metainfo.xml
          '';

          postFixup = ''
            patchelf --set-rpath "${pkgs.lib.makeLibraryPath rpathLibs}" \
              $out/bin/marcterm
          '';

          meta = with pkgs.lib; {
            description = "Terminal emulator built with Freya and Rust";
            homepage = "https://github.com/marc2332/marcterm";
            license = licenses.mit;
            platforms = platforms.linux;
            mainProgram = "marcterm";
          };
        };
      in {
        packages.default = marcterm;
        packages.marcterm = marcterm;

        apps.default = {
          type = "program";
          program = "${marcterm}/bin/marcterm";
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ marcterm ];
          packages = with pkgs; [ rust-analyzer clippy rustfmt ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath rpathLibs;
        };
      }
    ) // {
      overlays.default = final: prev: {
        marcterm = self.packages.${prev.system}.marcterm;
      };
    };
}
