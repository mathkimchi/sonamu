{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    nixgl.url = "github:nix-community/nixGL";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      nixgl,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          # NOTE: to self: might need to delete flake.lock or run `nix flake update nixgl` when you add an overlay
          overlays = [ nixgl.overlay ];
        };
      in
      with pkgs;
      rec {
        # `nix run github:mathkimchi/sonamu`
        packages.default = rustPlatform.buildRustPackage {
          pname = "sonamu";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true; # smithay from git
          };
          cargoBuildFlags = [
            "-p"
            "sonamu_sde"
          ];
          doCheck = false;

          nativeBuildInputs = [
            pkg-config
            autoPatchelfHook
          ];
          buildInputs = [
            libxkbcommon
            fontconfig
            udev
            seatd
            libgbm
            libinput
            pixman
            wayland
            vulkan-loader
            libX11
            libXcursor
            libXi
            libXrandr
          ];
          # Loaded at runtime (dlopen), so added to the binaries' runpath
          runtimeDependencies = [
            libGL
            vulkan-loader
            libxkbcommon
            wayland
          ];

          meta.mainProgram = "sonamu_sde";
        };

        devShell = mkShell rec {
          buildInputs = [
            # Fix: linking with `cc` failed
            # rustc # inconsistencies from only downloading rustc here while rustup, cargo, etc come from system
            gcc

            libxkbcommon
            libGL

            # for smithay client toolkit
            pkg-config
            fontconfig

            # for smithay (compositor)
            udev
            seatd
            libgbm
            libinput
            pixman

            # WINIT_UNIX_BACKEND=wayland
            wayland

            # # for servo
            # clang
            # libclang
            # cmake
            # # make
            # glibc.dev
            # llvmPackages.stdenv
            # # # llvmPackages_22.libc
            # # llvmPackages_22.libllvm
            # # llvmPackages_22.libc-full
            # # llvmPackages_22.clang-unwrapped
            # llvmPackages_20.libc
            # llvmPackages_20.libllvm
            # llvmPackages_20.libc-full
            # llvmPackages_20.clang-unwrapped
            # glib
            # # fontsan dependencies:
            # opentype-sanitizer # ots
            # lz4
            # brotli
            # woff2

            # # something python fails without this
            # python3Packages.pyyaml

            # technically this was for the glyphon demo, but I might need it later anyways
            # https://github.com/iced-rs/iced/issues/2385
            libX11
            libXcursor
            libXrandr
            libXi
            libxcb
            # libxkbcommon
            vulkan-loader
            pkgs.nixgl.nixGLIntel

            # for helix
            wgsl-analyzer

            # needed for cargo-update
            openssl
          ];
          LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
          # LIBCLANG_PATH = "${}";

          # From https://github.com/iced-rs/iced/issues/2385
          shellHook = ''
            export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}";
          '';
        };
      }
    );
}
# {
#   inputs = {
#     # flake-utils.url = "github:numtide/flake-utils";
#     nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
#     nixgl.url = "github:nix-community/nixGL";
#   };

#   outputs =
#     {
#       self,
#       nixpkgs,
#       # flake-utils,
#       nixgl,
#       ...
#     }:
#     let
#       pkgs = import nixpkgs {
#         inherit system;
#         # NOTE: to self: might need to delete flake.lock or run `nix flake update nixgl` when you add an overlay
#         overlays = [ nixgl.overlay ];
#       };
#     in
#     {
#       devShells.${system}.default = pkgs.mkShell {
#         packages = with pkgs; [
#           libxkbcommon
#           libGL

#           # for smithay client toolkit
#           pkg-config
#           fontconfig

#           # WINIT_UNIX_BACKEND=wayland
#           wayland

#           # for servo
#           clang
#           libclang
#           cmake
#           # make
#           glibc.dev
#           llvmPackages.stdenv
#           # # llvmPackages_22.libc
#           # llvmPackages_22.libllvm
#           # llvmPackages_22.libc-full
#           # llvmPackages_22.clang-unwrapped
#           llvmPackages_20.libc
#           llvmPackages_20.libllvm
#           llvmPackages_20.libc-full
#           llvmPackages_20.clang-unwrapped
#           glib
#           # fontsan dependencies:
#           opentype-sanitizer # ots
#           lz4
#           brotli
#           woff2

#           # # something python fails without this
#           # python3Packages.pyyaml

#           # technically this was for the glyphon demo, but I might need it later anyways
#           # https://github.com/iced-rs/iced/issues/2385
#           libX11
#           libXcursor
#           libXrandr
#           libXi
#           libxcb
#           # libxkbcommon
#           vulkan-loader
#           nixgl.nixGLIntel
#         ];
#         LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
#         # LIBCLANG_PATH = "${}";

#         # From https://github.com/iced-rs/iced/issues/2385
#         shellHook = ''
#           export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}";
#         '';
#       };
#     };
# }
