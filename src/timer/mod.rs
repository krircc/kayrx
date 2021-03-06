//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! * `Delay` is a future that does no work and completes at a specific `Instant`
//!   in time.
//!
//! * `Interval` is a stream yielding a value at a fixed period. It is
//!   initialized with a `Duration` and repeatedly yields each time the duration
//!   elapses.
//!
//! * `Timeout`: Wraps a future or stream, setting an upper bound to the amount
//!   of time it is allowed to execute. If the future or stream does not
//!   complete in time, then it is canceled and an error is returned.
//!
//! * `DelayQueue`: A queue where items are returned once the requested delay
//!   has expired.
//!
//! These types are sufficient for handling a large number of scenarios
//! involving time.
//!
//! These types must be used from within the context of the `Runtime`.
//!
//! # Examples
//!
//! Wait 100ms and print "Hello World!"
//!
//! ```
//! use kayrx::timer::delay_for;
//!
//! use std::time::Duration;
//!
//!#[kayrx::main]
//! async fn main() {
//!     delay_for(Duration::from_millis(100)).await;
//!     println!("100 ms have elapsed");
//! }
//! ```
//!
//! Require that an operation takes no more than 300ms. Note that this uses the
//! `timeout` function on the `FutureExt` trait. This trait is included in the
//! prelude.
//!
//! ```
//! use kayrx::timer::{timeout, Duration};
//!
//! async fn long_future() {
//!     // do work here
//! }
//!
//! # async fn dox() {
//! let res = timeout(Duration::from_secs(1), long_future()).await;
//!
//! if res.is_err() {
//!     println!("operation timed out");
//! }
//! # }
//! ```

pub mod delay_queue;

pub use std::time::Duration;
pub use clock::clock_util::{pause, resume};
#[doc(inline)]
pub use delay_queue::DelayQueue;
pub use delay::{delay_for, delay_until, Delay};
pub use error::Error;
pub use self::instant::Instant;
pub use interval::{interval, interval_at, Interval};
#[doc(inline)]
pub use timeout::{timeout, timeout_at, Elapsed, Timeout};
pub use throttle::{throttle, Throttle};

mod clock;
mod error;
mod delay;
mod instant;
mod interval;
mod throttle;
mod timeout;
mod wheel;

pub(crate) use self::clock::Clock;
pub(crate) mod driver;

// ===== Internal utils =====

enum Round {
    Up,
    Down,
}

/// Convert a `Duration` to milliseconds, rounding up and saturating at
/// `u64::MAX`.
///
/// The saturating is fine because `u64::MAX` milliseconds are still many
/// million years.
#[inline]
fn ms(duration: Duration, round: Round) -> u64 {
    const NANOS_PER_MILLI: u32 = 1_000_000;
    const MILLIS_PER_SEC: u64 = 1_000;

    // Round up.
    let millis = match round {
        Round::Up => (duration.subsec_nanos() + NANOS_PER_MILLI - 1) / NANOS_PER_MILLI,
        Round::Down => duration.subsec_millis(),
    };

    duration
        .as_secs()
        .saturating_mul(MILLIS_PER_SEC)
        .saturating_add(u64::from(millis))
}