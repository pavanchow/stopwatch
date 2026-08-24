use std::cell::Cell;
use std::time::Instant;

/// A source of monotonically increasing nanosecond timestamps.
///
/// The profiler never reads the wall clock directly. Every timing
/// decision goes through this trait, which is what makes the timing
/// logic testable: swap in a `MockClock` and drive time by hand instead
/// of racing the operating system's scheduler.
pub trait Clock {
    fn now_nanos(&self) -> u64;
}

/// A clock backed by `std::time::Instant`, for real measurements.
///
/// The instant is captured once, at construction, and every reading is
/// the elapsed time since that moment. This keeps `now_nanos` monotonic
/// even across leap seconds or system clock adjustments, since
/// `Instant` is guaranteed monotonic on supported platforms.
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_nanos(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
}

/// A clock a test can set and advance by hand.
///
/// Starts at zero. Interior mutability (`Cell`) lets `now_nanos` take
/// `&self` like the trait requires, while `advance` and `set` take
/// `&self` too, so a `MockClock` can be shared behind a shared
/// reference and still be driven forward from test code.
#[derive(Default)]
pub struct MockClock {
    nanos: Cell<u64>,
}

impl MockClock {
    pub fn new() -> Self {
        Self {
            nanos: Cell::new(0),
        }
    }

    pub fn advance(&self, nanos: u64) {
        self.nanos.set(self.nanos.get().saturating_add(nanos));
    }

    pub fn set(&self, nanos: u64) {
        self.nanos.set(nanos);
    }
}

impl Clock for MockClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.get()
    }
}
