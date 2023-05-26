{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    naersk.url = "github:nix-community/naersk/master";
    naersk.inputs.nixpkgs.follows = "nixpkgs";

    utils.url = "github:numtide/flake-utils";

    espeak-ng.url = "github:espeak-ng/espeak-ng";
    espeak-ng.flake = false;
  };

  outputs = inputs@{ self, nixpkgs, utils, naersk, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        stdenv = pkgs.stdenv;
        lib = pkgs.lib;

        naersk-lib = pkgs.callPackage naersk { };

        # Remove pcaudio support to avoid pulling in a lot of extra dependencies.
        # Use trunk, as there's no release containing the fix for https://github.com/espeak-ng/espeak-ng/issues/1271.
        espeak-ng = (pkgs.espeak-ng.override { pcaudiolibSupport = false; })
          .overrideAttrs(_: {
            src = inputs.espeak-ng;
            patches = [
              (pkgs.substituteAll { src = ./espeak.patch; mbrola = pkgs.mbrola; })
            ];
          });
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          name = "cc-speaks";
          src = ./.;

          # Magic to get bindgen to work.
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          preConfigure = ''
            export BINDGEN_EXTRA_CLANG_ARGS="$(< ${stdenv.cc}/nix-support/libc-crt1-cflags) \
              $(< ${stdenv.cc}/nix-support/libc-cflags) \
              $(< ${stdenv.cc}/nix-support/cc-cflags) \
              $(< ${stdenv.cc}/nix-support/libcxx-cxxflags) \
              ${lib.optionalString stdenv.cc.isClang "-idirafter ${stdenv.cc.cc}/lib/clang/${lib.getVersion stdenv.cc.cc}/include"} \
              ${lib.optionalString stdenv.cc.isGNU "-isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc} -isystem ${stdenv.cc.cc}/include/c++/${lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config} -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${lib.getVersion stdenv.cc.cc}/include"} \
              $NIX_CFLAGS_COMPILE"
          '';

          nativeBuildInputs = [
            pkgs.llvm
            pkgs.pkg-config
            pkgs.protobuf
          ];

          buildInputs = [
            pkgs.glibc
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
