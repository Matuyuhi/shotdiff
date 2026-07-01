use criterion::{black_box, criterion_group, criterion_main, Criterion};
use image::{Rgba, RgbaImage};

const PINK: Rgba<u8> = Rgba([0xFF, 0x2D, 0x87, 0xFF]);

#[inline]
fn pixel_delta(a: &[u8; 4], b: &[u8; 4]) -> u8 {
    let mut d = 0u8;
    for i in 0..4 {
        d = d.max(a[i].abs_diff(b[i]));
    }
    d
}

fn build_diff_float(a: &RgbaImage, b: &RgbaImage, threshold: u8) -> (RgbaImage, u64) {
    let (w, h) = a.dimensions();
    let mut out = RgbaImage::new(w, h);
    let mut changed = 0u64;
    for ((pa, pb), out_px) in a.pixels().zip(b.pixels()).zip(out.pixels_mut()) {
        if pixel_delta(&pa.0, &pb.0) > threshold {
            changed += 1;
            *out_px = PINK;
        } else {
            let [r, g, bl, _] = pb.0;
            let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * bl as f32) as u32;
            let v = (128 + luma / 2).min(255) as u8;
            *out_px = Rgba([v, v, v, 0xFF]);
        }
    }
    (out, changed)
}

fn build_diff_int(a: &RgbaImage, b: &RgbaImage, threshold: u8) -> (RgbaImage, u64) {
    let (w, h) = a.dimensions();
    let mut out = RgbaImage::new(w, h);
    let mut changed = 0u64;
    for ((pa, pb), out_px) in a.pixels().zip(b.pixels()).zip(out.pixels_mut()) {
        if pixel_delta(&pa.0, &pb.0) > threshold {
            changed += 1;
            *out_px = PINK;
        } else {
            let [r, g, bl, _] = pb.0;
            let luma = (299 * r as u32 + 587 * g as u32 + 114 * bl as u32) / 1000;
            let v = (128 + luma / 2).min(255) as u8;
            *out_px = Rgba([v, v, v, 0xFF]);
        }
    }
    (out, changed)
}

fn bench_build_diff(c: &mut Criterion) {
    let w = 1920;
    let h = 1080;
    let a = RgbaImage::from_pixel(w, h, Rgba([100, 150, 200, 255]));
    let b = RgbaImage::from_pixel(w, h, Rgba([100, 150, 200, 255]));

    let mut group = c.benchmark_group("build_diff");
    group.bench_function("float", |b_bench| {
        b_bench.iter(|| build_diff_float(black_box(&a), black_box(&b), black_box(16)))
    });
    group.bench_function("int", |b_bench| {
        b_bench.iter(|| build_diff_int(black_box(&a), black_box(&b), black_box(16)))
    });
    group.finish();
}

criterion_group!(benches, bench_build_diff);
criterion_main!(benches);
