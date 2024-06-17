{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    utils.url = "github:numtide/flake-utils";

    espeak-ng.url = "github:espeak-ng/espeak-ng";
    espeak-ng.flake = false;
  };

  outputs = inputs@{ self, nixpkgs, utils, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        stdenv = pkgs.stdenv;
        lib = pkgs.lib;

        # Remove pcaudio support to avoid pulling in a lot of extra dependencies.
        # Use trunk, as there's no release containing the fix for https://github.com/espeak-ng/espeak-ng/issues/1271.
        espeak-ng = (pkgs.espeak-ng.override { pcaudiolibSupport = false; })
          .overrideAttrs(_: {
            src = inputs.espeak-ng;
            patches = [
              (pkgs.substituteAll { src = ./espeak.patch; mbrola = pkgs.mbrola; })
            ];
            # Overwrite main postInstall to avoid setting ALSA_PLUGIN_DIR and pulling in all of ALSA.
            postInstall = ''
              patchelf --set-rpath "$(patchelf --print-rpath $out/bin/espeak-ng)" $out/bin/speak-ng
            '';
          });
      in
      {
        defaultPackage = pkgs.rustPlatform.buildRustPackage {
          name = "cc-speaks";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [
            pkgs.protobuf
          ];

          buildInputs = [
            pkgs.rustPlatform.bindgenHook
            espeak-ng
          ];
        };

        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            pkgs.rustPackages.clippy
            espeak-ng
          ];
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        };
      });
}
