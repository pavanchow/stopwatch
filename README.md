# Stopwatch

**A small, from-scratch profiler in Rust that tells you exactly where your program's time went.**

Stopwatch instruments a program with `enter`/`exit` spans (or a `span()` closure), builds a call tree as it runs, and reports total time, self time, and call counts for every function it saw. It is small enough to read start to finish in one sitting, and its timing logic is deterministically testable, because the clock is injected rather than read from the wall.

## A call tree with self time and total time

Every span becomes a node in an arena-backed call tree. Each node tracks:

- `calls`: how many times the span was entered
- `total_nanos`: wall time from enter to exit, including children
- `self_nanos`: that same span, minus whatever time its children accounted for

The distinction matters. A function can have a huge total time because it calls something slow, or a huge self time because it is genuinely doing the work itself. Stopwatch keeps both numbers so you don't have to guess which one you're looking at.

## Deterministic timing with an injected clock

The profiler never reads `std::time::Instant` directly. It goes through a `Clock` trait:

```rust
pub trait Clock {
    fn now_nanos(&self) -> u64;
}
```

`SystemClock` implements it for real measurements. `MockClock` implements it for tests: it starts at zero and only moves when a test calls `advance(nanos)` or `set(nanos)`. That means the test suite asserts exact nanosecond totals instead of hoping a sleep was long enough or a CI runner wasn't busy. See `DESIGN.md` for the accounting rules that make this possible.

## Recursion and repeated spans

A span entered again with the same name under the same parent reuses the node it created the first time, so calling the same function twice from the same call site aggregates into one row instead of two. A function that calls itself nests: each recursive call becomes a child node one level deeper, so a recursive `fib` shows up as a chain of `fib` nodes, one per depth, each with its own calls/total/self.

## The epitaph

Alongside the full tree, Stopwatch answers one question directly: which function ate the most time on its own.

```
Your program spent 61.2% of its life in partition().
```

## Workloads

Four built-in workloads exercise the profiler with real work, not synthetic sleeps:

- `fib`: naive recursive Fibonacci. Shows recursion depth and a call-count explosion.
- `sort`: a small bubble sort next to a larger quick sort (with its own `partition` span), so total and self time visibly diverge between the two.
- `blur`: a box blur over a synthetic grayscale buffer, split into `horizontal_pass` and `vertical_pass`.
- `mixed`: runs all three under one top-level span.

## Usage

```sh
cargo build --release

# Text report + epitaph
stopwatch run --workload mixed

# JSON call tree only
stopwatch run --workload sort --json
```

As a library:

```rust
use stopwatch::{Profiler, SystemClock};

let mut profiler = Profiler::new(SystemClock::new());
profiler.span("work", |p| {
    p.span("step_one", |_| { /* ... */ }).unwrap();
    p.span("step_two", |_| { /* ... */ }).unwrap();
}).unwrap();

println!("{}", profiler.text_report());
if let Some(epitaph) = profiler.epitaph() {
    println!("{}", epitaph.message());
}
```

## JSON export

`to_json()` produces a hand-written, nested JSON tree, no serde:

```json
{ "name": "...", "calls": 0, "total_nanos": 0, "self_nanos": 0, "children": [] }
```

This is the exact shape a separate visualization consumes, so it is kept stable.

## Testing

```sh
cargo test
```

Every timing assertion in the test suite is driven by `MockClock`, so the results are exact nanosecond counts, not timing-dependent guesses. See `DESIGN.md` for the full accounting model.

By Pavan Nallamothu (pavanchow)
