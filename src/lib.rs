//! A small profiler that records a call tree with total time, self
//! time, and call counts per function, and reports where the time
//! actually went.
//!
//! The clock is injected through the `Clock` trait, which is what
//! makes the timing logic deterministically testable: drive a
//! `MockClock` by hand in a test instead of racing the wall clock.

mod clock;
mod error;
mod profiler;
mod report;
pub mod workloads;

pub use clock::{Clock, MockClock, SystemClock};
pub use error::ProfilerError;
pub use profiler::{Node, Profiler, MAX_DEPTH, MAX_NAME_LEN, MAX_NODES, ROOT};
pub use report::Epitaph;
