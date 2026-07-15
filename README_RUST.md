# Rust port

The Rust codebase has been ported by LLM from [jo-m](https://jo-m.ch/)'s Go implementation. jo-m has since retired his project. While Go is not a bad fit for Onlytrains, personally i feel much more at home in Rust and i wish to improve upon it further.

## Installation

Runtime dependencies:

- ffmpeg

Using Nix: `nix develop` provides everything including the cross toolchains.
Using Docker: `make docker_build` (Go) or `cargo build` inside the Docker image built from this repo's `Dockerfile`.

### Native build requirements (Rust)

To build the Rust code directly on your OS you need:

| Tool | Purpose | Arch | Debian/Ubuntu | Fedora |
|------|---------|------|---------------|--------|
| Rust + cargo | toolchain | `rustup` from <https://rustup.rs> | — | — |
| clang + libclang | bindgen (v4l2-sys, SQLite) | `clang` | `clang libclang-dev` | `clang clang-devel` |
| Linux kernel headers | `<linux/videodev2.h>` | `linux-headers` | `linux-libc-dev` | `kernel-headers` |

```bash
cargo build --release
```

For `aarch64-unknown-linux-gnu` static builds:

```bash
rustup target add aarch64-unknown-linux-gnu
pacman -S aarch64-linux-gnu-gcc aarch64-linux-gnu-glibc
RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target aarch64-unknown-linux-gnu -p trainbot
```

## Changelog / Breaking changes

(sorted new to old)

- WIP GIFs are no longer generated, instead we generate H264-encoded MP4s. The `ffmpeg` executable is now a runtime dependency. The frontends will only display MP4s. Migrate your old `data/blobs` using [`migrate-gif-mp4.sh`](./migrate-gif-mp4.sh). You can delete the old GIFs.
- Migration to Rust. New build requirements. (S)FTP upload feature has been dropped, as i was not using it. I am open for contributions to reimplement it.

## Contributions

`make rust_check` must pass

## Performance testing

### Grafana / Prometheus metrics

- The Grafana dashboard tell you how well the BufSrc and AutoStitcher threads are coping with the load.

### Profiling

- install [samply](https://github.com/mstange/samply)
- `cargo build --release`
- `samply record target/release/trainbot -i internal/pkg/stitch/testdata/set0/day.mp4`
- on raspi: `samply record --save-only -p $(pgrep trainbot) --unstable-presymbolicate`. Copy `.json.gz` and `.json.syms.json` to your computer and `samply load ….json.gz`
