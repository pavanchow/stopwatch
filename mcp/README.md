# Stopwatch MCP server

An [MCP](https://modelcontextprotocol.io) server that runs a built-in Stopwatch workload under the profiler and hands an agent back the analysis: the call tree, the epitaph, folded flamegraph stacks, or the raw JSON tree.

## Tool

- **run_workload** run `fib`, `sort`, `blur`, or `mixed` under the profiler. `format` is `report` (call tree plus the epitaph), `json` (the raw tree), or `flamegraph` (folded stacks for `flamegraph.pl`).

## A note on scope and safety

This server does not compile or run arbitrary code. That would make it a remote code execution service, which is not something to publish casually. To profile your own Rust deterministically, embed the crate directly, which keeps the profiled program inside your own trust boundary:

```rust
use stopwatch::{Profiler, SystemClock};

let mut p = Profiler::new(SystemClock::new());
p.try_span("work", |p| {
    p.span("step_a", |_| do_a())?;
    p.span("step_b", |_| do_b())?;
    Ok::<(), stopwatch::ProfilerError>(())
})?;
println!("{}", p.text_report());
println!("{}", p.epitaph().unwrap().message());
```

## Install

```
cargo install --path ..
npm install
```

## Configure your client

```
claude mcp add stopwatch -- node /absolute/path/to/stopwatch/mcp/index.js
```

If the `stopwatch` binary is not on `PATH`, set `STOPWATCH_BIN` to its full path.

By Pavan Nallamothu.
