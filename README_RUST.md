# Rust port

The Rust codebase has been ported by LLM from [jo-m](https://jo-m.ch/)'s Go implementation. jo-m has since retired his project. While Go is not a bad fit for Onlytrains, personally i feel much more at home in Rust and i wish to improve upon it further.

## Installation

Using Nix: `nix develop` provides everything including the cross toolchains.
Using Docker: `make docker_build` (Go) or `cargo build` inside the Docker image built from this repo's `Dockerfile`.

### Native build requirements (Rust)

To build the Rust code directly on your OS you need:

| Tool | Purpose | Arch | Debian/Ubuntu | Fedora |
|------|---------|------|---------------|--------|
| Rust + cargo | toolchain | `rustup` from <https://rustup.rs> | — | — |
| clang + libclang | bindgen (v4l2-sys, SQLite) | `clang` | `clang libclang-dev` | `clang clang-devel` |
| Linux kernel headers | `<linux/videodev2.h>` | `linux-headers` | `linux-libc-dev` | `kernel-headers` |
| musl-gcc | x86\_64-musl static builds | `musl-tools` | `musl-tools` | `musl-gcc` |
| ffmpeg | runtime (video decoding) | `ffmpeg` | `ffmpeg` | `ffmpeg` |

After installing the above, register the musl target and build:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build                                          # native debug
cargo build --target x86_64-unknown-linux-musl       # static x86_64
```

For `aarch64-unknown-linux-musl` static builds, install [`cross`](https://github.com/cross-rs/cross) and run:

```bash
cross build --release --target aarch64-unknown-linux-musl -p trainbot
```
