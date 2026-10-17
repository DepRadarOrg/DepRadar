//! Criterion benchmarks for the typosquat signals. Run with `cargo bench`.

use criterion::{criterion_group, criterion_main, Criterion};
use deprader::typosquat::dataset::load_popular_packages;
use deprader::typosquat::distance::{levenshtein_distance, nearest_match};
use deprader::typosquat::scanner::score_package;
use std::hint::black_box;

/// `(label, candidate)`: an exact popular name, a one-keystroke typo, and
/// "react" with a Cyrillic 'а' (U+0430).
const INPUTS: [(&str, &str); 3] = [
    ("clean", "react"),
    ("typo", "expresss"),
    ("homoglyph", "re\u{0430}ct"),
];

fn bench_levenshtein(c: &mut Criterion) {
    let mut group = c.benchmark_group("levenshtein_distance");
    for (label, input) in INPUTS {
        group.bench_function(label, |b| {
            b.iter(|| levenshtein_distance(black_box(input), black_box("express")))
        });
    }
    group.finish();
}

fn bench_nearest_match(c: &mut Criterion) {
    let dataset = load_popular_packages();
    let mut group = c.benchmark_group("nearest_match");
    for (label, input) in INPUTS {
        group.bench_function(label, |b| {
            b.iter(|| nearest_match(black_box(input), black_box(&dataset)))
        });
    }
    group.finish();
}

fn bench_score_package(c: &mut Criterion) {
    let mut group = c.benchmark_group("score_package");
    for (label, input) in INPUTS {
        group.bench_function(label, |b| b.iter(|| score_package(black_box(input))));
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_levenshtein,
    bench_nearest_match,
    bench_score_package
);
criterion_main!(benches);
