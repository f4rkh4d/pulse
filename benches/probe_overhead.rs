//! microbench for the pure bookkeeping side of probing: rolling success
//! rate + status history update. this is NOT measuring http latency — that
//! dwarfs any overhead here. the point is to show that pulse's own
//! per-probe cost is negligible compared to the network cost.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use pulse::probe::{ProbeResult, ProbeState};

fn record_many(stats: &mut ProbeState, n: usize) {
    for i in 0..n {
        let ok = i % 4 != 0;
        let r = ProbeResult {
            idx: 0,
            status: Some(if ok { 200 } else { 500 }),
            latency_ms: ((i % 50) as u128) + 1,
            ok,
        };
        stats.record(&r);
    }
}

fn bench_probe_bookkeeping(c: &mut Criterion) {
    c.bench_function("probe_state_record_60", |b| {
        b.iter(|| {
            let mut s = ProbeState::default();
            record_many(&mut s, black_box(60));
            black_box(s.success_rate());
        });
    });

    c.bench_function("probe_state_record_1000", |b| {
        b.iter(|| {
            let mut s = ProbeState::default();
            record_many(&mut s, black_box(1000));
            black_box(s.success_rate());
        });
    });
}

criterion_group!(benches, bench_probe_bookkeeping);
criterion_main!(benches);
