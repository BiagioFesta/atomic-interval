//! A thread-safe tiny implementation of a *lock-free* interval/timer structure.
//!
//! ## Example
//! ```
//! use atomic_interval::AtomicIntervalLight;
//! use std::time::Duration;
//! use std::time::Instant;
//!
//! let period = Duration::from_secs(1);
//! let atomic_interval = AtomicIntervalLight::new(period);
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
#![warn(missing_docs)]

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
    ///
    /// The first tick is not instantaneous at the creation of the interval. It means `period`
    /// amount of time has to elapsed for the first tick.
    pub fn new(period: Duration) -> Self {
        Self {
            inner: AtomicIntervalImpl::new(period),
        }
    }

    /// The period set for this interval.
    pub fn period(&self) -> Duration {
        self.inner.period()
    }

    /// Changes the period of this interval.
    pub fn set_period(&mut self, period: Duration) {
        self.inner.set_period(period)
    }

    /// Checks whether the interval's tick expired.
    ///
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
    /// use atomic_interval::AtomicInterval;
    /// use std::sync::atomic::Ordering;
    /// use std::time::Duration;
    /// use std::time::Instant;
    ///
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

    /// The period set for this interval.
    pub fn period(&self) -> Duration {
        self.inner.period()
    }

    /// Changes the period of this interval.
    pub fn set_period(&mut self, period: Duration) {
        self.inner.set_period(period)
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
    fn set_period(&mut self, period: Duration) {
        self.period = period
    }

    #[inline(always)]
    fn period(&self) -> Duration {
        self.period
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

    #[test]
    fn test_ticks() {
        utilities::test_ticks_impl::<false>();
        utilities::test_ticks_impl::<true>();
    }

    mod utilities {
        use super::*;

        pub(super) fn test_ticks_impl<const WEAK_CMP: bool>() {
            const NUM_TICKS: usize = 10;
            const ERROR_TOLERANCE: f64 = 0.03; // 3%

            let period = Duration::from_millis(10);
            let atomic_interval = AtomicIntervalImpl::new(period);

            for _ in 0..NUM_TICKS {
                let elapsed = wait_for_atomic_interval::<WEAK_CMP>(&atomic_interval);
                assert!(period <= elapsed);

                let error = elapsed.as_secs_f64() / period.as_secs_f64() - 1_f64;
                assert!(
                    error <= ERROR_TOLERANCE,
                    "Delay error {:.1}% (max: {:.1}%)",
                    error * 100_f64,
                    ERROR_TOLERANCE * 100_f64
                );
            }
        }

        fn wait_for_atomic_interval<const WEAK_CMP: bool>(
            atomic_interval: &AtomicIntervalImpl,
        ) -> Duration {
            loop {
                let (ticked, elapsed) =
                    atomic_interval.is_ticked::<WEAK_CMP>(Ordering::Relaxed, Ordering::Relaxed);
                if ticked {
                    break elapsed;
                }
            }
        }
    }
}
