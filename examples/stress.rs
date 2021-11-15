use atomic_interval::AtomicInterval;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Barrier;
use std::time::Duration;
use std::time::Instant;

fn main() {
    let num_threads = num_cpus::get();
    let num_samples = 1000;
    let period = Duration::from_millis(1);
    let atomic_interval = Arc::new(AtomicInterval::new(period));
    let barrier_start = Arc::new(Barrier::new(num_threads));

    let mut index_iteration = 0;
    loop {
        println!("Iteration: {}", index_iteration);
        index_iteration += 1;

        let (samples_sender, samples_receiver) = mpsc::channel();

        #[allow(clippy::needless_collect)]
        let threads = (0..num_threads)
            .map(|_| {
                let atomic_interval = atomic_interval.clone();
                let samples_sender = samples_sender.clone();
                let barrier_start = barrier_start.clone();

                std::thread::spawn(move || {
                    barrier_start.wait();

                    loop {
                        let sample = loop {
                            if atomic_interval.is_ticked(Ordering::Relaxed, Ordering::Relaxed) {
                                break Instant::now();
                            }
                        };

                        if samples_sender.send(sample).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        let mut samples = Vec::with_capacity(num_samples);
        while samples.len() < num_samples {
            samples.push(samples_receiver.recv().unwrap());
        }

        drop(samples_receiver);
        threads
            .into_iter()
            .for_each(|join_handle| join_handle.join().unwrap());

        samples.sort_unstable();

        let diffs = samples
            .into_iter()
            .as_slice()
            .windows(2)
            .map(|adj| {
                let first = adj[0];
                let second = adj[1];

                second.duration_since(first)
            })
            .collect::<Vec<_>>();

        let min = diffs.iter().min().unwrap();
        let max = diffs.iter().max().unwrap();

        if *min < period {
            let anticip_error = 1_f64 - min.as_secs_f64() / period.as_secs_f64();
            println!(
                "  Max anticipation error: {:.3}% with {:?}",
                anticip_error * 100_f64,
                min
            );
        }

        if period < *max {
            let delay_error = max.as_secs_f64() / period.as_secs_f64() - 1_f64;
            println!(
                "  Max delay error: {:.3}% with {:?}",
                delay_error * 100_f64,
                max
            );
        }
    }
}
