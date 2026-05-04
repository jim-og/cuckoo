use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use cuckoo::core::{Store, Timer, TimerId};

// A comfortably large start time so short-wheel timer intervals stay in the
// future (tick rounds down to ~1_000_000 - 7 = 999_992).
const NOW: u64 = 1_000_000;

fn make_timer(start: u64, interval: u64) -> Timer {
    Timer::new(TimerId::new(), start, interval, None)
}

// Intervals that land each timer in a different storage tier.
//
// With NOW=1_000_000 and tick=999_992:
//   overdue     -> pop_time = 1 < tick
//   short_wheel -> pop_time = NOW + 500 ≈ 1_000_500 (within SHORT_WHEEL_PERIOD_MS = 1024ms of tick)
//   long_wheel  -> pop_time = NOW + 2_000 (within LONG_WHEEL_PERIOD_MS ≈ 4.2M ms of tick)
//   heap        -> pop_time = NOW + 4_300_000 (beyond long wheel)
const TIERS: &[(&str, u64, u64)] = &[
    ("overdue", 0, 1),
    ("short_wheel", NOW, 500),
    ("long_wheel", NOW, 2_000),
    ("heap", NOW, 4_300_000),
];

// ── insert ────────────────────────────────────────────────────────────────────

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_insert");

    let preload_sizes = [0usize, 1_000, 100_000];

    for &(tier, start, interval) in TIERS {
        for &size in &preload_sizes {
            group.bench_with_input(
                BenchmarkId::new(tier, size),
                &(start, interval, size),
                |b, &(start, interval, size)| {
                    b.iter_batched(
                        || {
                            let mut store = Store::new(NOW);
                            for _ in 0..size {
                                // Preload with timers spread across the target tier.
                                store.insert(make_timer(start, interval + 8));
                            }
                            store
                        },
                        |mut store| {
                            store.insert(make_timer(start, interval));
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }

    group.finish();
}

// ── pop ───────────────────────────────────────────────────────────────────────

fn bench_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_pop");

    let sizes = [100usize, 10_000];

    for &size in &sizes {
        // All timers in short wheel, all expiring at NOW+600.
        group.bench_with_input(
            BenchmarkId::new("short_wheel_expire", size),
            &size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut store = Store::new(NOW);
                        for _ in 0..size {
                            store.insert(make_timer(NOW, 500));
                        }
                        store
                    },
                    |mut store| store.pop(NOW + 600),
                    BatchSize::LargeInput,
                );
            },
        );

        // Timers in long wheel cascade into short wheel at pop.
        group.bench_with_input(
            BenchmarkId::new("long_wheel_cascade", size),
            &size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut store = Store::new(NOW);
                        for _ in 0..size {
                            store.insert(make_timer(NOW, 2_000));
                        }
                        store
                    },
                    |mut store| store.pop(NOW + 3_000),
                    BatchSize::LargeInput,
                );
            },
        );
    }

    // Pop with nothing expiring — overhead of the tick loop itself.
    group.bench_function("empty_tick", |b| {
        b.iter_batched(
            || {
                let mut store = Store::new(NOW);
                store.insert(make_timer(NOW, 60_000));
                store
            },
            |mut store| store.pop(NOW + 100),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

// ── remove ────────────────────────────────────────────────────────────────────

fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("store_remove");

    let sizes = [100usize, 10_000, 100_000];

    for &(tier, start, interval) in TIERS {
        for &size in &sizes {
            group.bench_with_input(
                BenchmarkId::new(tier, size),
                &(start, interval, size),
                |b, &(start, interval, size)| {
                    b.iter_batched(
                        || {
                            let mut store = Store::new(NOW);
                            let mut ids = Vec::with_capacity(size);
                            for _ in 0..size {
                                let id = TimerId::new();
                                store.insert(Timer::new(id.clone(), start, interval, None));
                                ids.push(id);
                            }
                            (store, ids)
                        },
                        |(mut store, ids)| {
                            store.remove(&ids[ids.len() / 2]);
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_insert, bench_pop, bench_remove);
criterion_main!(benches);
