# Design

## The Clock abstraction

Everything in Stopwatch that needs a timestamp goes through one trait:

```rust
pub trait Clock {
    fn now_nanos(&self) -> u64;
}
```

`SystemClock` captures a `std::time::Instant` at construction and returns elapsed nanoseconds since that moment on every call. `MockClock` holds a `Cell<u64>` starting at zero, with `advance(nanos)` and `set(nanos)` to move it by hand.

This is the reason the timing logic can be tested at all. Wall-clock timing tests are inherently approximate: sleep for 10ms and assert the measurement is "close to" 10ms, then watch the assertion flake on a loaded CI box. With the clock injected, a test drives `MockClock` through an exact sequence of timestamps and asserts an exact nanosecond result. There is no tolerance window because there is no noise source. The profiler's internal arithmetic (subtraction of start from now, subtraction of children's elapsed from a span's own elapsed) is the thing under test, and it is deterministic given deterministic input.

## The arena call tree

The call tree lives in one `Vec<Node>`, addressed by index rather than built out of `Rc<RefCell<...>>` or raw pointers. Node 0 is a sentinel root that every top-level span attaches to; it is never entered directly, so its own `calls`/`total_nanos`/`self_nanos` stay at zero, and reports treat it as a container rather than a measured function.

```rust
pub struct Node {
    pub name: String,
    pub calls: u64,
    pub total_nanos: u64,
    pub self_nanos: u64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}
```

A span's identity is `(parent index, name)`. `enter()` looks for an existing child of the current frame's node with a matching name and reuses it if found; otherwise it allocates a new node. This is what makes repeated spans aggregate: the same function called twice from the same call site hits the same node both times, and its `calls`, `total_nanos`, and `self_nanos` accumulate rather than producing duplicate rows.

Two resource bounds keep the arena finite: `MAX_NODES` caps how many distinct nodes the arena will ever hold, and `MAX_DEPTH` caps how deep the frame stack may go. `MAX_NAME_LEN` caps a span name's length. Crossing any of them returns a `ProfilerError` variant. Nothing in the profiler panics on bad or excessive input; a program that runs away with instrumentation gets a typed error back from `enter()`, not a crashed profiler.

## Self time vs. total time, and the frame stack

The frame stack is a `Vec<Frame>` separate from the arena:

```rust
struct Frame {
    node_idx: usize,
    start_nanos: u64,
    children_elapsed: u64,
}
```

`enter(name)` finds or creates the node, records `now_nanos()` as `start_nanos`, and pushes a frame with `children_elapsed` at zero. `exit()` pops the top frame and does the accounting:

```
elapsed        = now - start_nanos
self_time      = elapsed - children_elapsed
node.total_nanos += elapsed
node.self_nanos  += self_time
node.calls       += 1
```

Then, if a parent frame remains on the stack, `elapsed` is added to that parent frame's `children_elapsed`. This is the whole trick: every child's elapsed time gets folded into its parent's `children_elapsed` accumulator exactly once, right when the child exits, so by the time the parent itself exits, subtracting `children_elapsed` from the parent's own `elapsed` leaves exactly the time the parent spent outside of any child call. Because this happens per frame instance rather than per node, aggregating repeated or recursive calls into one node never double-counts: each individual enter/exit pair contributes its own elapsed and self time once, and the node just sums those contributions across every call it represents.

`span(name, f)` is `enter` + `f(self)` + `exit`, wrapped so the caller cannot forget the matching `exit()`.

## Recursion

Node identity is `(parent index, name)`, and "parent" means the node currently occupying the top of the frame stack, not a fixed call site. When a span recurses into itself, the inner call's parent is the node the outer call is presently sitting in, so the inner call gets its own child node one level deeper rather than colliding with the outer one. A three-level recursive `fib` therefore produces three distinct `fib` nodes, one per depth, each with its own `calls`/`total_nanos`/`self_nanos`. Two independent recursive branches at the same depth under the same parent frame (for example `fib(n-1)` and `fib(n-2)` both called directly from the same invocation) do collapse into the same child node, since the lookup only depends on parent and name, not on which branch made the call. This is consistent with how repeated spans aggregate in general: same parent, same name, same node.

Because self time is computed and added per stack frame instance, not per node, this collapsing never breaks the invariant that summing every node's `self_nanos` reproduces the total time spent. Every nanosecond between the outermost enter and outermost exit belongs to the self time of exactly one active frame at the moment it elapses; recursion changes which node that frame's contribution lands on, not how many nanoseconds get counted.

## The report

`text_report()` walks the tree depth-first starting from the root's children, printing one line per node: name (indented by depth), calls, total time in milliseconds, self time in milliseconds, and percent of the root's total time. At each level, children are sorted so the one with the largest total time prints first, so the heaviest branch of the tree is always the one you see first.

Percent of total is computed against `root_total_nanos()`, which is the sum of every top-level (direct child of the sentinel root) node's `total_nanos`. Since top-level spans do not overlap in time, this sum is the same as the wall time the whole profiled run took.

## The epitaph

`epitaph()` scans every node except the sentinel root and returns the one with the largest `self_nanos`, alongside what fraction of the root's total time that represents:

```rust
pub struct Epitaph {
    pub name: String,
    pub self_nanos: u64,
    pub percent_of_total: f64,
}
```

`message()` turns that into one line: `"Your program spent 61.2% of its life in partition()."` The phrasing is deliberate. A profiler's job is ultimately to answer "where did the time go," and self time, not total time, is the honest answer to that question: total time credits a function for work its children did, self time does not.

## The JSON shape

`to_json()` hand-writes JSON (no serde) as a single nested object rooted at the sentinel root, whose `total_nanos` is overridden to `root_total_nanos()` so the top of the tree carries a meaningful total even though the sentinel itself is never entered. Every node, including the root, serializes to exactly this shape:

```json
{
  "name": "...",
  "calls": 0,
  "total_nanos": 0,
  "self_nanos": 0,
  "children": []
}
```

`children` is an array of objects with the same shape, recursively, one entry per child node in arena order. String values are escaped for quotes, backslashes, and control characters. This shape is the contract a separate visualization consumes, so it stays exactly as written here.

By Pavan Nallamothu (pavanchow)

## Performance and robustness notes

- **O(1) child lookup.** Each node keeps a `child_index` map from child name to arena index alongside the ordered `children` vector, so `find_or_create_child` is a hash lookup rather than a linear scan. Without it, instrumentation that generates a unique span name per call would degrade to O(n squared).
- **Iterative reports.** `text_report`, `to_json`, and `to_flamegraph` walk the tree with an explicit work stack, not recursion, so a very deep call tree costs O(1) host stack instead of one stack frame per level.
- **`try_span`.** For a closure that returns a `Result`, `try_span` flattens the profiler's error into the closure's error type, so a caller writes a single `?` instead of the `??` a plain `span` around a fallible closure would force.
- **Folded flamegraph export.** `to_flamegraph` emits Brendan Gregg's folded-stack format, one line per function as `top;child self_nanos`, ready to pipe into `flamegraph.pl`.
