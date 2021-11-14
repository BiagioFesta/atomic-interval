# atomic-interval
A very tiny library implementing an *lock-free* atomic timer.

## Examples

```rust
use atomic_interval::AtomicIntervalLight;
use std::time::Duration;
use std::time::Instant;
use std::thread;

const PERIOD: Duration = Duration::from_secs(1);

fn main() {
    let atomic_interval = AtomicIntervalLight::new(PERIOD);
    let time_start = Instant::now();

    let elapsed = loop {
        if atomic_interval.is_ticked() {
            break time_start.elapsed();
        }

        thread::sleep(Duration::from_millis(1));
    };

    println!("Elapsed: {:.2?}", elapsed);  // Elapsed: 1.00s
}
```

```
cargo run --example simple
```
