use crate::timer::driver::Entry;
use crate::timer::{Duration, Error, Instant};

use std::sync::Arc;
use std::task::{self, Poll};

/// Registration with a timer.
///
/// The association between a `Delay` instance and a timer is done lazily in
/// `poll`
#[derive(Debug)]
pub(crate) struct Registration {
    entry: Arc<Entry>,
}

impl Registration {
    pub(crate) fn new(deadline: Instant, duration: Duration) -> Registration {
        Registration {
            entry: Entry::new(deadline, duration),
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.entry.time_ref().deadline
    }

    pub(crate) fn reset(&mut self, deadline: Instant) {
        unsafe {
            self.entry.time_mut().deadline = deadline;
        }

        Entry::reset(&mut self.entry);
    }

    pub(crate) fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    pub(crate) fn poll_elapsed(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        self.entry.poll_elapsed(cx)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        Entry::cancel(&self.entry);
    }
}