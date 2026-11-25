//! Tick throughput for the 100-agent tower.
//!
//! ```text
//! cargo bench --bench tick_throughput
//! ```

use std::hint::black_box;
use std::time::{Duration, Instant};

use agent_tower_sim::sim::{initialize_tower, run_tick};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const SEED: u64 = 0xA11C_E000;

fn tick_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("tower_tick");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(5));

    group.throughput(Throughput::Elements(1));
    group.bench_function("steady_tick", |b| {
        b.iter_custom(|iters| {
            let mut state = initialize_tower(SEED);
            for _ in 0..100 {
                run_tick(&mut state);
            }
            let started = Instant::now();
            for _ in 0..iters {
                run_tick(black_box(&mut state));
            }
            black_box(state.tick);
            started.elapsed()
        });
    });

    const BATCH: u64 = 100;
    group.throughput(Throughput::Elements(BATCH));
    group.bench_function("hundred_ticks_after_warmup", |b| {
        b.iter_custom(|iters| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iters {
                let mut state = initialize_tower(SEED);
                for _ in 0..50 {
                    run_tick(&mut state);
                }
                let started = Instant::now();
                for _ in 0..BATCH {
                    run_tick(black_box(&mut state));
                }
                elapsed += started.elapsed();
                black_box(state.tick);
            }
            elapsed
        });
    });

    group.finish();
}

criterion_group!(benches, tick_throughput);
criterion_main!(benches);
