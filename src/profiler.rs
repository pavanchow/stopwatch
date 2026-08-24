use crate::clock::Clock;
use crate::error::ProfilerError;
use std::collections::HashMap;

/// Hard ceiling on the number of distinct nodes the arena will hold.
/// Reached only by pathological instrumentation (a name generated per
/// call, for example). Crossing it returns an error instead of letting
/// the arena grow without bound.
pub const MAX_NODES: usize = 10_000;

/// Hard ceiling on how deep the frame stack may go. Crossing it is
/// almost always a missing `exit()`, so it is treated as an error
/// rather than silently truncated.
pub const MAX_DEPTH: usize = 256;

/// Hard ceiling on a span name's length in bytes.
pub const MAX_NAME_LEN: usize = 128;

/// Index of the sentinel root node, always the first node in the arena.
pub const ROOT: usize = 0;

/// One entry in the call tree arena.
///
/// `total_nanos` is wall time from enter to exit, including whatever
/// happened in children. `self_nanos` is that total with every child's
/// time subtracted out, so it is what this function did on its own.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub calls: u64,
    pub total_nanos: u64,
    pub self_nanos: u64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Child name to arena index, so `find_or_create_child` is O(1)
    /// instead of a linear scan. Keeping the `children` Vec alongside
    /// preserves first-seen order for traversal, the map is only for
    /// lookup. Without this, a workload that generates a unique span
    /// name per call turns instrumentation into an O(n^2) anchor.
    child_index: HashMap<String, usize>,
}

impl Node {
    fn new(name: String, parent: Option<usize>) -> Self {
        Self {
            name,
            calls: 0,
            total_nanos: 0,
            self_nanos: 0,
            parent,
            children: Vec::new(),
            child_index: HashMap::new(),
        }
    }
}

/// One active call on the frame stack.
///
/// `children_elapsed` accumulates the elapsed time of every child span
/// that exits while this frame is on top, so that at exit the frame's
/// own self time is just `elapsed - children_elapsed`.
struct Frame {
    node_idx: usize,
    start_nanos: u64,
    children_elapsed: u64,
}

/// Records a call tree by timestamping `enter`/`exit` pairs against an
/// injected clock.
///
/// The tree lives in a flat arena (`Vec<Node>`) addressed by index,
/// with node 0 reserved as a sentinel root that every top-level span
/// hangs off of. A span entered again with the same name under the
/// same parent reuses the node it created the first time, so calling
/// the same function from the same call site twice aggregates into one
/// node rather than producing two.
///
/// Recursion into the same name nests: a function calling itself
/// creates a fresh child node one level deeper each time, because the
/// "same parent" for the inner call is the node the outer call is
/// currently occupying, not the name itself. Two independent recursive
/// branches at the same depth under the same parent frame do still
/// collapse into one node, since node identity is (parent, name), not
/// call-site history.
pub struct Profiler<C: Clock> {
    clock: C,
    nodes: Vec<Node>,
    stack: Vec<Frame>,
}

impl<C: Clock> Profiler<C> {
    pub fn new(clock: C) -> Self {
        Self {
            clock,
            nodes: vec![Node::new("root".to_string(), None)],
            stack: Vec::new(),
        }
    }

    /// Begin a span. Must be paired with a later `exit()`.
    pub fn enter(&mut self, name: &str) -> Result<(), ProfilerError> {
        if name.len() > MAX_NAME_LEN {
            return Err(ProfilerError::NameTooLong {
                limit: MAX_NAME_LEN,
                actual: name.len(),
            });
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err(ProfilerError::MaxDepthExceeded { limit: MAX_DEPTH });
        }

        let parent_idx = self.stack.last().map(|f| f.node_idx).unwrap_or(ROOT);
        let node_idx = self.find_or_create_child(parent_idx, name)?;
        let start_nanos = self.clock.now_nanos();

        self.stack.push(Frame {
            node_idx,
            start_nanos,
            children_elapsed: 0,
        });
        Ok(())
    }

    /// Close the most recently entered, still-open span.
    pub fn exit(&mut self) -> Result<(), ProfilerError> {
        let frame = self.stack.pop().ok_or(ProfilerError::ExitWithoutEnter)?;
        let now = self.clock.now_nanos();
        let elapsed = now.saturating_sub(frame.start_nanos);
        let self_time = elapsed.saturating_sub(frame.children_elapsed);

        let node = &mut self.nodes[frame.node_idx];
        node.total_nanos += elapsed;
        node.self_nanos += self_time;
        node.calls += 1;

        if let Some(parent_frame) = self.stack.last_mut() {
            parent_frame.children_elapsed += elapsed;
        }
        Ok(())
    }

    /// Run `f` inside a span named `name`: enter, call `f`, exit.
    ///
    /// `f` receives the profiler back so spans can be nested from
    /// inside the closure.
    pub fn span<F, R>(&mut self, name: &str, f: F) -> Result<R, ProfilerError>
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.enter(name)?;
        let result = f(self);
        self.exit()?;
        Ok(result)
    }

    /// Like `span`, but for a closure that itself returns a `Result`.
    /// It flattens the two error layers into one, so a caller writes a
    /// single `?` instead of the awkward `??` that a plain `span` around
    /// a fallible closure would force. Any `ProfilerError` from the
    /// enter/exit is converted into the closure's error type `E`.
    pub fn try_span<F, T, E>(&mut self, name: &str, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<ProfilerError>,
    {
        self.enter(name)?;
        let result = f(self);
        self.exit()?;
        result
    }

    fn find_or_create_child(
        &mut self,
        parent_idx: usize,
        name: &str,
    ) -> Result<usize, ProfilerError> {
        if let Some(&existing) = self.nodes[parent_idx].child_index.get(name) {
            return Ok(existing);
        }

        if self.nodes.len() >= MAX_NODES {
            return Err(ProfilerError::MaxNodesExceeded { limit: MAX_NODES });
        }

        let new_idx = self.nodes.len();
        self.nodes.push(Node::new(name.to_string(), Some(parent_idx)));
        self.nodes[parent_idx].children.push(new_idx);
        self.nodes[parent_idx]
            .child_index
            .insert(name.to_string(), new_idx);
        Ok(new_idx)
    }

    /// The clock backing this profiler. Tests use this to reach a
    /// `MockClock`'s `advance`/`set` methods, since `Profiler` owns
    /// the clock outright rather than borrowing it.
    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// All nodes in arena order. Index 0 is the sentinel root.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Number of spans still open (unmatched `enter()` calls).
    pub fn open_spans(&self) -> usize {
        self.stack.len()
    }

    /// Total time covered by the profile: the sum of every top-level
    /// span's total time, since the sentinel root itself is never
    /// entered directly.
    pub fn root_total_nanos(&self) -> u64 {
        self.nodes[ROOT]
            .children
            .iter()
            .map(|&idx| self.nodes[idx].total_nanos)
            .sum()
    }
}
