use criterion::{criterion_group, criterion_main, Criterion};
use pmatch::{score_rgba_cos, search_rgba};

const X0: u32 = 65;
const Y0: u32 = 35;
const W: u32 = 30;
const H: u32 = 20;

fn load_bird() -> image::RgbaImage {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../pkg/pmatch/testdata/bird.jpg");
    image::open(path).unwrap().to_rgba8()
}

fn bench_score(c: &mut Criterion) {
    let img = load_bird();
    let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();
    c.bench_function("score_rgba_cos", |b| {
        b.iter(|| score_rgba_cos(&img, &pat, X0 + 1, Y0))
    });
}

fn bench_search(c: &mut Criterion) {
    let img = load_bird();
    let pat = image::imageops::crop_imm(&img, X0, Y0, W, H).to_image();
    c.bench_function("search_rgba", |b| b.iter(|| search_rgba(&img, &pat)));
}

criterion_group!(benches, bench_score, bench_search);
criterion_main!(benches);
