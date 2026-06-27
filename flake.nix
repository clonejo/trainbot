{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
  } @ inputs: let
    lib = nixpkgs.lib;
    system = "x86_64-linux";
    pkgs = import inputs.nixpkgs {
      system = system;
      overlays = [rust-overlay.overlays.default];
    };
    rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    muslPkgs = import inputs.nixpkgs {
      system = system;
      crossSystem = {
        config = "x86_64-unknown-linux-musl";
        isStatic = true;
      };
    };
    aarch64MuslPkgs = import inputs.nixpkgs {
      system = system;
      crossSystem = {
        config = "aarch64-unknown-linux-musl";
        isStatic = true;
      };
    };
  in {
    formatter.${system} = nixpkgs.legacyPackages.${system}.alejandra;

    devShells.${system}.default = pkgs.mkShell rec {
      nativeBuildInputs = with pkgs; [];
      packages = with pkgs; [
        # Rust toolchain (stable + musl targets declared in rust-toolchain.toml)
        rustToolchain

        # C tooling for bundled C deps (rusqlite, v4l2-sys bindgen)
        clang
        llvm
        libclang

        # musl cross toolchains
        pkgsCross.musl64.buildPackages.gcc # x86_64-linux-musl-gcc
        pkgsCross.aarch64-multiplatform-musl.buildPackages.gcc # aarch64-linux-musl-gcc

        # videodev2.h for v4l2-sys bindgen (Phase 3)
        linux-headers-libre # provides <linux/videodev2.h>

        # Build tools
        gcc
        pkg-config
        gnumake
        curl
        go_1_26

        # Frontend
        nodejs_24
      ];

      buildInputs = with pkgs; [
        # Vulkan bare tools and dependencies
        glslang
        vulkan-headers
        vulkan-loader
        vulkan-validation-layers

        # More Vulkan tools
        vulkan-extension-layer
        vulkan-tools
        vulkan-tools-lunarg
        vulkan-volk
      ];

      LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
      VK_LAYER_PATH = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
      VULKAN_SDK = "${pkgs.vulkan-validation-layers}/share/vulkan/explicit_layer.d";
      XDG_DATA_DIRS = builtins.getEnv "XDG_DATA_DIRS";
      XDG_RUNTIME_DIR = "/run/user/1000";
      LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
      # Required so bindgen (v4l2-sys, rusqlite) finds <linux/videodev2.h>
      BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.linux-headers-libre}/include";
    };
  };
}
