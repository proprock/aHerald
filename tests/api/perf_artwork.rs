//! Warm-file-cache artwork baseline, independent of resident IPC, GUI and monitor DPI.
use aherald::{artwork::prepare_at, config::Config, events::Event};
use std::{hint::black_box, path::Path, time::Instant};

const KINDS: [&str; 4] = [
    "turn_complete",
    "permission_required",
    "input_required",
    "error",
];
const WARMUP_ROUNDS: usize = 3;
const MEASURED_ROUNDS: usize = 25;
const MEAN_BUDGET_MS: f64 = 30.0;

fn prepare(config: &Config, event: &Event, root: &Path) -> f64 {
    let start = Instant::now();
    let animation = prepare_at(config, event, root, 1.0)
        .expect("fixture must decode and prepare")
        .expect("fixture must produce artwork, not text fallback");
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    // Keep missing, truncated or accidentally resized fixtures from appearing faster.
    assert_eq!(animation.frames.len(), 8);
    assert_eq!(animation.durations_ms, vec![83; 8]);
    assert!(animation.looping);
    for frame in &animation.frames {
        assert_eq!((frame.width, frame.height), (64, 64));
        assert_eq!(frame.bgra.len(), 64 * 64 * 4);
    }
    black_box(animation);
    elapsed
}

#[test]
fn warmed_artwork_preparation_stays_inside_budget() {
    let wall = Instant::now();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perf_artwork");
    let config = Config::parse(
        "[notifications]\nvisual='builtin'\nscale=1.0\ninterpolation='nearest'",
        &root,
    )
    .unwrap();
    let events = KINDS.map(|kind| Event::demo(kind).unwrap());
    // Warm the filesystem/allocator paths, but never retain a prepared-animation cache.
    for _ in 0..WARMUP_ROUNDS {
        for event in &events {
            prepare(&config, event, &root);
        }
    }
    let mut samples: [Vec<f64>; 4] = std::array::from_fn(|_| Vec::new());
    // Interleave kinds so one kind does not always see the early or late machine load.
    for round in 0..MEASURED_ROUNDS {
        for offset in 0..events.len() {
            let index = (round + offset) % events.len();
            samples[index].push(prepare(&config, &events[index], &root));
        }
    }
    println!("artwork: scale=1 nearest, {WARMUP_ROUNDS} warmups + {MEASURED_ROUNDS} samples/kind");
    println!("kind                 n   mean_ms median_ms    sd_ms   min_ms   p95_ms   max_ms");
    let mut failures = Vec::new();
    for (kind, mut values) in KINDS.into_iter().zip(samples) {
        values.sort_by(f64::total_cmp);
        let n = values.len();
        let mean = values.iter().sum::<f64>() / n as f64;
        let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
        let median = values[n / 2];
        let p95 = values[(n * 95).div_ceil(100) - 1];
        println!(
            "{kind:<20} {n:>2} {mean:>9.2} {median:>9.2} {sd:>8.2} {:>8.2} {p95:>8.2} {:>8.2}",
            values[0],
            values[n - 1]
        );
        if mean > MEAN_BUDGET_MS {
            failures.push(format!("{kind}: {mean:.2} ms > {MEAN_BUDGET_MS:.2} ms"));
        }
    }
    println!(
        "wall time including warmup: {:.2} s",
        wall.elapsed().as_secs_f64()
    );
    assert!(
        failures.is_empty(),
        "artwork mean budget exceeded: {}",
        failures.join("; ")
    );
}
