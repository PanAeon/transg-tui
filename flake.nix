{
  description = "Rust dev env";

  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
      in {
        devShell = with pkgs;
          mkShell {
            name = "rust-env";
            nativeBuildInputs = [ wrapGAppsHook ];
            buildInputs = [
              (rust-bin.stable.latest.default.override {
                extensions = [ "rust-src" ];
              })
              rustfmt
              clippy
              rust-analyzer
              pkg-config
              cargo-generate
              openssl
              rust-bindgen
              curl
              libtorrent-rasterbar # needed for libtorrent-sys
              pkgs.boost.dev
            ];
            LIBCLANG_PATH = pkgs.lib.makeLibraryPath
              [ pkgs.llvmPackages_latest.libclang.lib ];
            PKG_CONFIG_PATH =
              "${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libxml2.dev}/lib/pkgconfig";
            BINDGEN_EXTRA_CLANG_ARGS =
              # Includes with normal include path
              (builtins.map (a: ''-I"${a}/include"'') [
                pkgs.glibc.dev
                pkgs.libtorrent-rasterbar.dev
              ])
              # Includes with special directory paths
              ++ [
                ''
                  -I"${pkgs.llvmPackages_latest.libclang.lib}/lib/clang/${pkgs.llvmPackages_latest.libclang.version}/include"''
                "-I ${pkgs.llvmPackages_latest.clang.libc_dev}/include"
              ];
            #            RUST_SRC_PATH = "${pkgs.rust.packages.nightly.rustPlatform.rustLibSrc}";
            shellHook = "exec fish";
          };
      });
}
