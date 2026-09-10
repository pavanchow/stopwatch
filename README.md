<img src="docs/logo.svg" alt="Stopwatch logo" width="96">

# Stopwatch: a call-tree profiler in Rust

Stopwatch is a call-tree profiler written from scratch in Rust that tells you exactly where your program's time went, breaking each function into self time and total time. It instruments a program with `enter`/`exit` spans (or a `span()` closure), builds a call tree as it runs, and reports total time, self time, and call counts for every function it saw. Because the clock is injected rather than read from the wall, its timing is deterministic, so you can embed it in a test suite and assert exact nanosecond totals.

**[Live demo](https://pavanchow.github.io/stopwatch/)** · MIT licensed · written in Rust

Built from scratch by [Pavan Nallamothu](https://pavanchow.github.io/) ([LinkedIn](https://www.linkedin.com/in/pavanchow/), [GitHub](https://github.com/pavanchow)).

## Self time vs total time, explained

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

`SystemClock` implements it for real measurements. `MockClock` implements it for tests: it starts at zero and only moves when a test calls `advance(nanos)` or `set(nanos)`. That makes Stopwatch a deterministic benchmarking tool where the test suite asserts exact nanosecond totals instead of hoping a sleep was long enough or a CI runner wasn't busy. See `DESIGN.md` for the accounting rules that make this possible.

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

# Folded stacks for flamegraph.pl
stopwatch run --workload sort --flamegraph
```

As a library. Use `span` for a closure that returns a plain value, and `try_span` for one that returns a `Result`, which flattens the error so you write one `?` instead of `??`:

```rust
use stopwatch::{Profiler, ProfilerError, SystemClock};

let mut profiler = Profiler::new(SystemClock::new());
profiler.try_span("work", |p| -> Result<(), ProfilerError> {
    p.span("step_one", |_| { /* ... */ })?;
    p.span("step_two", |_| { /* ... */ })?;
    Ok(())
})?;

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

## A lightweight flamegraph alternative

`to_flamegraph()` (and `stopwatch run --workload sort --flamegraph`) emits Brendan Gregg's folded-stack format, one line per function as `top;child;grandchild self_nanos`. Pipe it straight into `flamegraph.pl` to get an SVG, no perf and no OS-level tooling:

```sh
stopwatch run --workload sort --flamegraph | flamegraph.pl > sort.svg
```

## Stopwatch vs perf, flamegraph, and criterion

These are excellent tools solving adjacent problems. Stopwatch fills the gap between them: an embeddable, deterministic call tree.

| | Stopwatch | perf | flamegraph | criterion |
|---|---|---|---|---|
| What it is | embeddable call-tree profiler | OS-level sampling profiler | a visualizer for folded stacks | statistical microbenchmarks |
| Deterministic | yes (injected clock) | no | n/a | no, it is statistical |
| Platform | any, pure Rust | Linux | any (renders SVG) | any |
| Call tree with self vs total | yes | via post-processing | shows stacks, not the raw math | no |
| Embeds in a test suite | yes | no | no | yes, for timing only |
| Dependencies | one (`clap`), zero for the library | kernel support | Perl script | several crates |

Stopwatch even feeds flamegraph directly through its folded-stack export, so it complements that tool rather than replacing it.

## MCP server for agents

`mcp/` is a Model Context Protocol server exposing a `run_workload` tool that returns the report, the epitaph, or folded flamegraph stacks. It deliberately does not compile or run arbitrary code, profiling your own Rust is done by embedding the crate. See `mcp/README.md`.

## Testing

```sh
cargo test
```

Every timing assertion in the test suite is driven by `MockClock`, so the results are exact nanosecond counts, not timing-dependent guesses. See `DESIGN.md` for the full accounting model.

## License

MIT licensed. By Pavan Nallamothu (pavanchow).
