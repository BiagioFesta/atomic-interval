use atomic_interval::AtomicIntervalLight;
use std::mem::replace;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn main() {
    let num_threads = num_cpus::get();
    let period = Duration::from_secs(1);
    let atomic_interval = Arc::new(AtomicIntervalLight::new(period));
    let last_time = Arc::new(Mutex::new(Instant::now()));

    println!("Number of threads: {}", num_threads);
    println!("Period: {:?}", period);

    (0..num_threads)
        .map(|id_thread| {
            let atomic_interval = atomic_interval.clone();
            let last_time = last_time.clone();

            thread::spawn(move || {
                loop {
                    if atomic_interval.is_ticked() {
                        break;
                    }

                    thread::sleep(Duration::from_millis(1));
                }

                let elapsed =
                    replace(last_time.lock().unwrap().deref_mut(), Instant::now()).elapsed();

                println!(
                    " > Thread '{}' completed! (elapsed since last tick: {:.2?})",
                    id_thread, elapsed
                );
            })
        })
        .for_each(|join_handle| join_handle.join().unwrap());
}
