{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, utils, naersk }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        stdenv = pkgs.stdenv;
        lib = pkgs.lib;

        naersk-lib = pkgs.callPackage naersk { };
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
          ];

          buildInputs = [
            pkgs.espeak-ng
            pkgs.glibc
          ];
        };

        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.cargo
            pkgs.rustc
            pkgs.rustfmt
            pkgs.rustPackages.clippy
            pkgs.espeak-ng
          ];
          RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
        };
      });
}
