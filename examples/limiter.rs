use atomic_interval::AtomicIntervalLight;
use once_cell::sync::OnceCell;
use std::thread;
use std::time::Duration;

const MAX_PERIOD_SAMPLING: Duration = Duration::from_secs(1);

fn push_sample(id_thread: usize, value: u8) {
    // Note AtomicInterval can be used without additional
    // sync wrapper (e.g., a `Mutex`) as it is atomic.
    static LIMITER: OnceCell<AtomicIntervalLight> = OnceCell::new();

    let limiter_init = || AtomicIntervalLight::new(MAX_PERIOD_SAMPLING);

    // Only one threads can push a sample for each PERIOD.
    // We limit the samples acquisition with a interval.
    if LIMITER.get_or_init(limiter_init).is_ticked() {
        println!("Thread '{}' pushed sample: '{}'", id_thread, value);
    }
}

fn main() {
    let num_threads = num_cpus::get();

    (0..num_threads)
        .map(|id_thread| {
            thread::spawn(move || loop {
                let sample = rand::random();

                // Multiple threads concurrently try to push a sample.
                push_sample(id_thread, sample);

                thread::sleep(Duration::from_millis(1));
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|join_handle| join_handle.join().unwrap());
}
