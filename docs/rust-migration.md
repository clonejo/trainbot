# trainbot → Rust: Migration Plan

A phased plan to reimplement [trainbot](https://github.com/clonejo/trainbot) in Rust as a **drop-in replacement**, optimized for easy deployment (a single statically-linked binary with **no runtime dynamic C libraries**).

Tick boxes as you go. `[ ]` = todo, `[x]` = done.

---

## Locked decisions

| Area | Decision |
| --- | --- |
| Strategy | Idiomatic Rust rewrite (not a transliteration), preserving external contracts |
| Goal | Drop-in replacement for the existing deployment + independent Vue frontend |
| GPU (`pkg/vk`) | **Dropped.** CPU path (`pmatch`/`avg`) is the default; re-add later via `wgpu`/`ash` behind a feature if wanted |
| Upload / FTP | **Dropped from the core binary.** Becomes a separate binary in a later phase |
| Database | `rusqlite` with the **`bundled`** feature (static SQLite); embed the exact `schema.sql` and run it on every open |
| Logging | `tracing` + `tracing-subscriber` (structured + spans per pipeline stage) |
| CLI | `clap`, with **multi-call (argv[0]) dispatch** + subcommands; bare invocation runs the detector |
| Concurrency | `std::thread` + `crossbeam-channel`; **no async** in the core |
| CPU kernels | Safe Rust + `rayon` (replacing the C + OpenMP kernels) |
| Video files | Shell out to `ffmpeg` (subprocess), as the Go code effectively does |
| USB camera | `v4l` crate, **default `v4l2` feature only** (raw ioctls, no `libv4l`) |
| Pi Camera v3 | Shell out to `rpicam-vid` (subprocess) |
| Images | `image` crate (jpeg/png/gif, all pure-Rust codecs) + pure-Rust resize |
| Metrics | `metrics` + `metrics-exporter-prometheus`, exact metric names/types/buckets on `:18963` |
| Blob filenames | Already implemented in Rust by the maintainer; plug in as-is |
| Build target | **musl static** (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`) |

### The "no runtime dynamic C" guarantee

This holds **only** if these invariants stay true. Enforced in CI (see the static-binary gate in Phase 0):

- [ ] `rusqlite` is built with `bundled` (never the system-library path → would need `libsqlite3.so`)
- [ ] The `v4l` crate uses its default `v4l2` feature and **never** `libv4l` (which links `libv4l1`/`libv4l2`/`libv4lconvert`)
- [ ] Video/camera stay as **subprocesses** (`ffmpeg`, `rpicam-vid`); no `ffmpeg-next` / `libcamera-rs` linkage

`ffmpeg` and `rpicam-vid` remain **runtime executable** dependencies (the user must have them installed), but they are not linked into the binary — exactly as in the Go version.

Build-time (host) C tooling is still required and is **not** shipped: a C compiler (`cc`, to compile bundled SQLite) and `libclang` + the target's `videodev2.h` (for the bindgen step in `v4l2-sys`). These belong in the build environment / Nix flake, not the runtime. *Optional hardening:* swap `v4l`'s bindgen-based sys crate for a pure-const V4L2 binding to drop the `libclang` build-time dependency entirely.

---

## Target module layout (Cargo workspace)

A workspace keeps the pure kernels independently testable/benchmarkable and gives the future uploader its own binary without disturbing the core.

```
trainbot/                      # workspace root (Cargo.toml, Nix flake, CI)
├── crates/
│   ├── cv/                    # PURE, no I/O — easiest to validate first
│   │   ├── pmatch             # patch matching (was pkg/pmatch + c.c), rayon
│   │   ├── avg                # brightness avg / avg-dev (was pkg/avg + c.c), rayon
│   │   └── ransac             # RANSAC (was pkg/ransac)
│   ├── imutil/                # load/save jpg|png|gif, crop, resize, mask (was pkg/imutil)
│   ├── vid/                   # FrameSource trait: ffmpeg / v4l2 / rpicam-vid (was pkg/vid)
│   ├── store/                 # datastore paths + rusqlite (schema.sql, queries) (was internal/pkg/{db,upload-datastore})
│   └── core/                  # pipeline: sequence → fitDx → stitch → gif; metrics; config; orchestration
├── bin/
│   ├── trainbot/              # the multi-call binary (detect/confighelper/cleanup)
│   └── upload/                # LATER (Phase 8): separate uploader binary
└── tests/
    └── conformance/           # Go-vs-Rust drop-in harness (see below)
```

`pkg/vk` and the FTP half of `internal/pkg/upload` are intentionally absent.

---

## Drop-in contracts (the spec the rewrite must satisfy)

External surfaces a live deployment or the independent frontend depends on. Each is verified in the phase that builds it; tick when confirmed equivalent to Go.

- [ ] **SQLite schema** — embed `schema.sql` verbatim, execute on **every** DB open (idempotent; carries v1→v2). Live table `trains_v2`; `temperatures` in use. Do not redesign.
- [ ] **Data layout** — `--data-dir`/`DATA_DIR` (default `data`); `data/db.sqlite3`; `data/blobs/<name>`; thumbnails `<name>.thumb.jpg`. Blob filenames = maintainer's existing Rust code.
- [ ] **CLI flags + env vars** — full `go-arg` surface, names + defaults verbatim.
- [ ] **Prometheus metrics** — exact names, types, histogram buckets on `:18963`; Grafana JSON is the spec.
- [ ] **Binaries / invocation** — `trainbot`, `confighelper`, `cleanup` invocable by those names; bare `trainbot` takes no subcommand.
- [ ] **Detector ↔ uploader contract** — boundary is `trains_v2.uploaded` / `.cleaned_up`; detector writes `uploaded=false`.

---

## The conformance harness (built in Phase 0, used throughout)

The Go binary is the **reference oracle**. Fixtures: `internal/pkg/stitch/testdata/set0/{day,night,rain,snow}.mp4` with expected results in `auto_set0_test.go`:

- `day` → 86 frames, speed 21.53, accel 0.27
- `night` → 83 frames, speed 22.7, accel −0.5
- `rain` → 82 frames, speed 17.9, accel 0
- `snow` → 56 frames, speed 20.5, accel −0.75
- plus reference stitched `*.jpg` images.

Harness runs the same inputs through both binaries and diffs:

- [ ] `trains_v2` rows (within numeric tolerance)
- [ ] `sqlite3 .schema` output (must be identical)
- [ ] stitched images (perceptual/SSIM similarity vs reference jpgs)
- [ ] `/metrics` output (series names + bucket boundaries)
- [ ] final acceptance: Rust binary against a **real production `data/` dir**, frontend renders unchanged

---

## Phases

Validate the cheap, pure things first, then I/O, then the pipeline, then CLI/deploy, then cut over. Upload last.

### Phase 0 — Scaffolding, build target, and gates
**Work**
- [x] Cargo workspace + module skeleton above *(cv crates only so far; bin/ and tests/ added as later phases need them)*
- [ ] Extend the repo's Nix flake: Rust toolchain + musl cross + `cc` + `libclang` + target `videodev2.h`
- [ ] CI matrix: static binaries for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`
- [ ] Stand up the conformance harness skeleton (can run the Go reference binary)
- [ ] **Static-binary gate:** CI asserts no dynamic deps (`ldd` → "not a dynamic executable" / no `NEEDED`)

**Exit criteria**
- [ ] Empty binaries build static on both arches; gate green; harness can invoke the Go oracle

### Phase 1 — Pure CV kernels (`cv` crate)
**Work**
- [x] Port `pmatch` C kernel → safe Rust + `rayon` (replace OpenMP)
- [x] Replace `#pragma omp critical` max-update with a deterministic parallel reduction over `(cos2, x, y)`, **matching Go's tie-break** (first max under strict `>`, scan `y` outer then `x` inner → prefer lowest `(y, x)`)
- [x] Port `avg` C kernel → safe Rust + `rayon`
- [x] Port `ransac`

**Verify**
- [x] `pmatch`/`avg` unit tests: numeric equivalence against Go test vectors (tight tolerances)
- [x] `ransac` unit tests: convergence to same parameter values within Go tolerances (not bit-identical — RNG/optimizer differ; see impl notes)
- [x] `score_rgba_cos` cross-check: verify float output matches Go's `ScoreRGBACosSlow` at a known non-perfect offset (PNG saved from Go's decoder; 3 offsets checked at 1e-12 tolerance)
- [ ] `criterion` benchmarks vs the OpenMP C (no perf regression)

**Exit criteria**
- [x] Kernels match Go outputs; zero I/O in this crate
- [ ] No perf regression (blocked on criterion benchmarks above)

**Implementation notes**

*pmatch* — `compute_cos2` is a direct translation of the C kernel (integer accumulation of `dot`, `absI2`, `absP2`). Rayon parallelizes over rows; the x-scan within each row is sequential so the strict `>` update naturally picks the lowest x on a tie. The inter-row reduction also enforces lowest `(y, x)` on equal cos².

*avg* — Two rayon passes using `par_chunks(4)` over the raw byte buffer (no padding in `image::RgbaImage`). Integer-truncating division for the per-channel mean matches Go/C exactly.

*ransac* — Uses Levenberg-Marquardt with numerical Jacobian (nalgebra SVD solve). `rand::SmallRng` seeded from `MetaParams::seed`. The RNG stream differs from Go's `math/rand`, but the algorithm still converges to the correct parameters within the Go test tolerances.

*avg test data* — Go's `image/jpeg` and Rust's `zune-jpeg` decode the same JPEG to different pixel values (different DCT rounding). Solution: saved `pkg/avg/testdata/{high,mid,low}.png` using Go's decoder; Rust tests load those PNGs so both operate on bit-identical pixel data. Tolerances are tight (1e-5) since the arithmetic is integer-exact.

*pmatch cross-check* — Same decoder-divergence issue applies to `score_rgba_cos`. Saved `pkg/pmatch/testdata/bird.png` from Go's decoder; `test_score_rgba_cos_known_offset` loads this PNG and asserts 3 known offsets match Go's `ScoreRGBACosSlow` values within 1e-12. Criterion benchmarks wired in `benches/bench.rs` (`score_rgba_cos`, `search_rgba`); run `cargo bench -p pmatch` to compare throughput against the C+OpenMP baseline.

### Phase 2 — Image utils, datastore, DB (`imutil`, `store`)
**Work**
- [ ] `imutil`: jpg/png/gif load+save, crop, resize, mask via the `image` crate
- [ ] `store` datastore: path logic (`GetDBPath`, `GetBlobPath`, `.thumb.jpg`) matching Go's unit-tested strings
- [ ] Plug in the maintainer's blob-filename code
- [ ] `store` db: `rusqlite` (`bundled`), embed `schema.sql`, run on every open
- [ ] Implement the insert/query paths the detector and cleanup need

**Verify**
- [ ] Open a real `db.sqlite3`, run embedded schema, diff `.schema` Go-vs-Rust (empty)
- [ ] Round-trip `trains_v2` rows
- [ ] Confirm `start_ts` formatting matches Go (UNIQUE; subsecond/UTC/DST)
- [ ] Frontend still reads the resulting file

**Exit criteria**
- [ ] Schema diff empty; datastore path tests match Go; static gate still green

### Phase 3 — Video/camera sources (`vid` crate)
**Work**
- [ ] Define a `FrameSource` trait
- [ ] file → `ffmpeg` subprocess (honor pixel format / size)
- [ ] USB cam → `v4l` (default `v4l2` feature, raw ioctls), honoring `--camera-format-fourcc/-w/-h`
- [ ] `picam3` → `rpicam-vid` subprocess, honoring `--rotate-180`

**Verify**
- [ ] Decode the set0 mp4s; compare frame counts/dimensions to Go
- [ ] Re-run the static gate (proves the `v4l` default feature added no link)

**Exit criteria**
- [ ] All three sources produce frames matching Go; gate green

### Phase 4 — Stitch pipeline (`core`)
**Work**
- [ ] Threaded pipeline: source queue → `findOffset` (discard|record) → sequence → `fitDx` → stitch → image, using `std::thread` + `crossbeam-channel`
- [ ] `tracing` spans per stage
- [ ] GIF creation

**Verify (strongest drop-in check)**
- [ ] Run all four set0 videos; assert exact `auto_set0_test.go` numbers (frames / speed / accel)
- [ ] Image similarity vs `testdata/set0/*.jpg`

**Exit criteria**
- [ ] All four scenarios reproduce Go within tolerance and visually match

### Phase 5 — Metrics, logging, temperature (`core`)
**Work**
- [ ] `tracing-subscriber` pretty + JSON wired to `LogConfig` flags (`--log-pretty` etc.)
- [ ] `metrics` + `metrics-exporter-prometheus` on `:18963`
- [ ] Reproduce exact metric names, types, and **histogram bucket boundaries** (cross-check `grafana/Onlytrains-dashboard.json`)
- [ ] `temperatures`: periodic Pi thermal-zone read → insert

**Verify**
- [ ] Scrape `/metrics` from both binaries; diff series names + buckets
- [ ] Load the existing Grafana dashboard against the Rust binary

**Exit criteria**
- [ ] Metric series and buckets identical; dashboard renders

*Note:* `metrics-exporter-prometheus` pulls `hyper`/`tokio` transitively (pure Rust, so the static-C guarantee is unaffected). To avoid `tokio` entirely, swap in a minimal sync HTTP responder for `/metrics`.

### Phase 6 — CLI, multi-call binary, full config surface (`bin/trainbot`)
**Work**
- [ ] `clap` config mirroring the entire `go-arg` flag/env list and defaults verbatim
- [ ] Multi-call dispatch via `argv[0]`; symlink `trainbot`/`confighelper`/`cleanup` (+ arch-suffixed names) to one binary
- [ ] Also accept `trainbot <subcommand>`; **bare `trainbot --input ...` defaults to the detector**
- [ ] Update the Makefile to create the symlinks at install/deploy time
- [ ] Implement `confighelper` (interactive crop-rectangle web UI on `--listen-addr`)
- [ ] Implement `cleanup` (local blob cleanup)
- [ ] **Accept-and-ignore** `--enable-upload`/`ENABLE_UPLOAD` + `UPLOAD_*` vars this phase

**Verify**
- [ ] Diff `--help`
- [ ] Run the unmodified `trainbot.service` unit and Makefile `deploy_*` targets against the Rust binary
- [ ] Flag/env parity test

**Exit criteria**
- [ ] Identical invocation behavior; existing deploy tooling works unchanged

### Phase 7 — Cutover & drop-in acceptance
**Work**
- [ ] Full conformance run on a real `data/` dir
- [ ] Deploy to a Pi 4 alongside the existing frontend; confirm end-to-end
- [ ] Tag a release whose artifact names/arches match existing CI outputs

**Exit criteria**
- [ ] Static gate green on both arches
- [ ] Frontend renders production data identically
- [ ] Binary swaps in with no config changes (except the documented upload delta)

### Phase 8 — Separate uploader binary (`bin/upload`) — *later*
**Work**
- [ ] New binary operating on the same `data/` dir + DB, flipping `trains_v2.uploaded` / `.cleaned_up`
- [ ] Configurable **shell-command** upload hook (`--upload-command`), and/or SFTP/rsync
- [ ] Re-map `ENABLE_UPLOAD` semantics or add explicit flags; document the FTP-user migration
- [ ] Port remote orphan cleanup (`CleanupOrphanedRemoteBlobs`) if needed

**Exit criteria**
- [ ] Uploader marks rows correctly; cleanup acts on uploaded rows; FTP migration documented

---

## Cross-cutting risks to watch

- **Floating-point divergence.** Rust vs Go math can differ in the last bits; keep tolerances, and match the `pmatch` argmax tie-break to Go (Phase 1).
- **Histogram bucket parity** is exact-match-sensitive (Phase 5).
- **`start_ts` formatting** must match Go (UNIQUE; subsecond/UTC/DST) (Phase 2).
- **musl + bundled SQLite** compiles cleanly, but pin SQLite threading/build flags; schema re-runs on every open.
- **bindgen under musl cross** needs target `videodev2.h` + host `libclang` (Phase 0); the pure-const V4L2 option removes this.
- **The static-binary CI gate is the guardrail** for the whole no-runtime-C goal — keep it mandatory.

---

## Milestone checklist (one line per phase)

- [ ] **Phase 0** — Workspace + musl static build + conformance harness + no-dynamic-C gate
- [ ] **Phase 1** — Pure CV kernels (pmatch/avg/ransac) on rayon, validated against Go vectors *(score_rgba_cos cross-check vector + criterion benchmarks pending)*
- [ ] **Phase 2** — imutil + datastore + rusqlite(bundled) with the embedded idempotent schema
- [ ] **Phase 3** — FrameSource: ffmpeg / v4l2(raw) / rpicam-vid, all subprocess or syscall
- [ ] **Phase 4** — Threaded stitch pipeline, validated against the set0 expected numbers
- [ ] **Phase 5** — tracing + Prometheus (exact metrics) + temperature
- [ ] **Phase 6** — clap + multi-call binary (detect/confighelper/cleanup), full flag/env parity
- [ ] **Phase 7** — Cutover + drop-in acceptance on a real Pi + frontend
- [ ] **Phase 8** — (Later) Separate uploader binary; re-add upload as a shell hook / SFTP
