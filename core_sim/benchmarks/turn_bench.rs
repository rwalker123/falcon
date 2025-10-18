use core_sim::{build_headless_app, run_turn, SimulationConfig};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

fn bench_turn(c: &mut Criterion) {
    let mut group = c.benchmark_group("turn");

    for size in [8u32, 16, 32, 48, 64] {
        group.bench_with_input(BenchmarkId::new("grid", size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut app = build_headless_app();
                    app.world.resource_mut::<SimulationConfig>().grid_size = (size, size).into();
                    app
                },
                |mut app| {
                    run_turn(&mut app);
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(turn_benches, bench_turn);
criterion_main!(turn_benches);
