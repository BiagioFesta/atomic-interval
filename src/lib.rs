//! A thread-safe tiny implementation of a *lock-free* interval/timer structure.
//!
//! ## Example
//! ```
//! let period = Duration::from_secs(1);
//! let atomic_interval = AtomicIntervalLight::new(PERIOD);
//!
//! let time_start = Instant::now();
//! let elapsed = loop {
//!     if atomic_interval.is_ticked() {
//!         break time_start.elapsed();
//!     }
//! };
//!
//! println!("Elapsed: {:?}", elapsed);
//!
//! ```
//!
//! ## Memory Ordering
//! Like other standard atomic types, [`AtomicInterval`] requires specifying how the memory
//! accesses have to be synchronized.
//!
//! For more information see the [nomicon](https://doc.rust-lang.org/nomicon/atomics.html).
//!
//! ## AtomicIntervalLight
//! [`AtomicIntervalLight`] is an [`AtomicInterval`]'s variant that does not guarantee
//! any memory synchronization.
use quanta::Clock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// It implements a timer. It allows checking a periodic interval.
///
/// This structure is meant to be shared across multiple threads and does not
/// require additional sync wrappers. Generally, it can be used with
/// [`Arc<AtomicInterval>`](https://doc.rust-lang.org/std/sync/struct.Arc.html).
///
/// If you want performance maximization (when you do *not* need memory ordering
/// synchronization), [`AtomicIntervalLight`] is a relaxed variant of this class.
pub struct AtomicInterval {
    inner: AtomicIntervalImpl,
}

impl AtomicInterval {
    /// Creates a new [`AtomicInterval`] with a fixed period interval.
    /// The first tick is not instantaneous at the creation of the interval. It means `period`
    /// amount of time has to elapsed for the first tick.
    pub fn new(period: Duration) -> Self {
        Self {
            inner: AtomicIntervalImpl::new(period),
        }
    }

    /// Checks whether the interval's tick expired.
    /// When it returns `true` then *at least* `period` amount of time has passed
    /// since the last tick.
    ///
    /// When a period is passed (i.e., this function return `true`) the internal timer
    /// is automatically reset for the next tick.
    ///
    /// It takes two Ordering arguments to describe the memory ordering.
    /// `success` describes the required ordering when the period elapsed and the timer
    /// has to be reset (*read-modify-write* operation).
    /// `failures` describes the required ordering when the period is not passed yet.
    ///
    /// Using [`Ordering::Acquire`] as success ordering makes the store part of this operation
    /// [`Ordering::Relaxed`], and using [`Ordering::Release`] makes the successful load
    /// [`Ordering::Relaxed`].
    /// The failure ordering can only be [`Ordering::SeqCst`], [`Ordering::Acquire`] or
    /// [`Ordering::Relaxed`] and must be equivalent to or weaker than the success ordering.
    ///
    /// It can be used in a concurrency context: only one thread can tick the timer per period.
    ///
    /// # Example
    /// ```
    /// let atomic_interval = AtomicInterval::new(Duration::from_secs(1));
    /// let time_start = Instant::now();
    ///
    /// let elapsed = loop {
    ///    if atomic_interval.is_ticked(Ordering::Relaxed, Ordering::Relaxed) {
    ///        break time_start.elapsed();
    ///    }
    /// };
    ///
    /// println!("Elapsed: {:?}", elapsed);
    /// // Elapsed: 999.842446ms
    /// ```
    pub fn is_ticked(&self, success: Ordering, failure: Ordering) -> bool {
        self.inner.is_ticked::<false>(success, failure).0
    }
}

/// A relaxed version of [`AtomicInterval`]: for more information check that.
///
/// All [`Ordering`] are implicit: [`Ordering::Relaxed`].
///
/// On some architecture this version is allowed to spuriously fail.
/// It means [`AtomicIntervalLight::is_ticked`] might return `false` even if
/// the `period` amount of time has passed.
/// It can result in more efficient code on some platforms.
pub struct AtomicIntervalLight {
    inner: AtomicIntervalImpl,
}

impl AtomicIntervalLight {
    /// Creates a new [`AtomicIntervalLight`] with a fixed period interval.
    pub fn new(period: Duration) -> Self {
        Self {
            inner: AtomicIntervalImpl::new(period),
        }
    }

    /// See [`AtomicInterval::is_ticked`].
    pub fn is_ticked(&self) -> bool {
        self.inner
            .is_ticked::<true>(Ordering::Relaxed, Ordering::Relaxed)
            .0
    }
}

struct AtomicIntervalImpl {
    period: Duration,
    clock: Clock,
    last_tick: AtomicU64,
}

impl AtomicIntervalImpl {
    fn new(period: Duration) -> Self {
        let clock = Clock::new();
        let last_tick = AtomicU64::new(clock.start());

        Self {
            period,
            clock,
            last_tick,
        }
    }

    #[inline(always)]
    fn is_ticked<const WEAK_CMP: bool>(
        &self,
        success: Ordering,
        failure: Ordering,
    ) -> (bool, Duration) {
        let current = self.last_tick.load(failure);
        let elapsed = self.clock.delta(current, self.clock.end());

        if self.period <= elapsed
            && ((!WEAK_CMP
                && self
                    .last_tick
                    .compare_exchange(current, self.clock.start(), success, failure)
                    .is_ok())
                || (WEAK_CMP
                    && self
                        .last_tick
                        .compare_exchange_weak(current, self.clock.start(), success, failure)
                        .is_ok()))
        {
            (true, elapsed)
        } else {
            (false, elapsed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utilities::stress_timer_mt;

    #[test_case::test_case(Ordering::SeqCst, Ordering::SeqCst ; "SeqCst")]
    #[test_case::test_case(Ordering::Relaxed, Ordering::Relaxed ; "Relaxed")]
    fn test_stress_timer_strong_mt(success: Ordering, failure: Ordering) {
        stress_timer_mt::<false>(10, success, failure);
    }

    #[test_case::test_case(Ordering::SeqCst, Ordering::SeqCst ; "SeqCst")]
    #[test_case::test_case(Ordering::Relaxed, Ordering::Relaxed ; "Relaxed")]
    fn test_stress_timer_weak_mt(success: Ordering, failure: Ordering) {
        stress_timer_mt::<true>(10, success, failure);
    }

    mod utilities {
        use super::*;
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::sync::Barrier;

        pub(super) fn stress_timer_mt<const WEAK_CMP: bool>(
            num_iter: usize,
            success: Ordering,
            failure: Ordering,
        ) {
            let num_threads = num_cpus::get();
            let num_samples = 1000;
            let period = Duration::from_millis(1);
            let atomic_interval = Arc::new(AtomicIntervalImpl::new(period));
            let barrier_start = Arc::new(Barrier::new(num_threads));

            for _ in 0..num_iter {
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
                                    let (ticked, elapsed) =
                                        atomic_interval.is_ticked::<WEAK_CMP>(success, failure);
                                    if ticked {
                                        break elapsed;
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

                let min = samples.iter().min().unwrap();
                let max = samples.iter().max().unwrap();

                if *min < period {
                    let anticip_error = 1_f64 - min.as_secs_f64() / period.as_secs_f64();
                    println!(
                        "Max anticipation error: {:.3}% with {:?}",
                        anticip_error * 100_f64,
                        min
                    );
                }

                if period < *max {
                    let delay_error = max.as_secs_f64() / period.as_secs_f64() - 1_f64;
                    println!("Max delay error: {:.3}% with {:?}", delay_error, max);
                }

                samples
                    .iter()
                    .for_each(|elapsed| assert!(*elapsed >= period));
            }
        }
    }
}
