use crate::clock::Clock;
use crate::profiler::{Node, Profiler, ROOT};

/// The function that consumed the most time on its own, the answer to
/// "where did the program's life actually go."
pub struct Epitaph {
    pub name: String,
    pub self_nanos: u64,
    pub percent_of_total: f64,
}

impl Epitaph {
    /// A one-line, human-readable summary of the epitaph.
    pub fn message(&self) -> String {
        format!(
            "Your program spent {:.1}% of its life in {}().",
            self.percent_of_total, self.name
        )
    }
}

impl<C: Clock> Profiler<C> {
    /// The node with the greatest self time, excluding the sentinel
    /// root. `None` if nothing has been recorded yet.
    pub fn epitaph(&self) -> Option<Epitaph> {
        let root_total = self.root_total_nanos();
        let nodes = self.nodes();
        nodes
            .iter()
            .enumerate()
            .skip(1) // skip the sentinel root
            .max_by_key(|(_, node)| node.self_nanos)
            .map(|(_, node)| {
                let percent_of_total = if root_total > 0 {
                    node.self_nanos as f64 / root_total as f64 * 100.0
                } else {
                    0.0
                };
                Epitaph {
                    name: node.name.clone(),
                    self_nanos: node.self_nanos,
                    percent_of_total,
                }
            })
    }

    /// An indented tree of calls, total(ms), self(ms), and percent of
    /// the root total, heaviest child first at every level.
    ///
    /// The traversal is iterative with an explicit work stack, so it
    /// uses O(1) host stack no matter how deep the profiled call tree
    /// is. A recursive walk would put one host frame per tree level.
    pub fn text_report(&self) -> String {
        let root_total = self.root_total_nanos();
        let nodes = self.nodes();
        let mut out = String::new();
        out.push_str(&format!(
            "{:<32}{:>8}{:>12}{:>12}{:>9}\n",
            "name", "calls", "total(ms)", "self(ms)", "pct"
        ));

        // Stack of (node index, depth). Children are pushed heaviest-last
        // so the heaviest pops first, matching a heaviest-first DFS.
        let mut roots = nodes[ROOT].children.clone();
        sort_by_total(nodes, &mut roots);
        let mut stack: Vec<(usize, usize)> = roots.iter().rev().map(|&i| (i, 0)).collect();

        while let Some((idx, depth)) = stack.pop() {
            let node = &nodes[idx];
            let indent = "  ".repeat(depth);
            let label = format!("{indent}{}", node.name);
            let pct = if root_total > 0 {
                node.total_nanos as f64 / root_total as f64 * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                "{:<32}{:>8}{:>12.3}{:>12.3}{:>8.1}%\n",
                label,
                node.calls,
                node.total_nanos as f64 / 1_000_000.0,
                node.self_nanos as f64 / 1_000_000.0,
                pct
            ));

            let mut children = node.children.clone();
            sort_by_total(nodes, &mut children);
            for &child in children.iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out
    }

    /// The call tree as a JSON string: `{ name, calls, total_nanos,
    /// self_nanos, children }`, rooted at a synthetic top-level object
    /// whose `total_nanos` is the sum of every top-level span, since
    /// the sentinel root is never entered directly.
    ///
    /// Serialized iteratively with a work stack of "emit this node" and
    /// "emit this literal" items, so nesting depth costs no host stack.
    pub fn to_json(&self) -> String {
        let nodes = self.nodes();
        let root_total = self.root_total_nanos();
        let mut out = String::new();

        enum Work<'a> {
            Node(usize),
            Lit(&'a str),
        }
        let mut stack: Vec<Work> = vec![Work::Node(ROOT)];

        while let Some(item) = stack.pop() {
            match item {
                Work::Lit(s) => out.push_str(s),
                Work::Node(idx) => {
                    let node = &nodes[idx];
                    let total_nanos = if idx == ROOT { root_total } else { node.total_nanos };
                    out.push_str("{\"name\":");
                    write_json_string(&node.name, &mut out);
                    out.push_str(&format!(
                        ",\"calls\":{},\"total_nanos\":{},\"self_nanos\":{},\"children\":[",
                        node.calls, total_nanos, node.self_nanos
                    ));
                    // Close this object after its children.
                    stack.push(Work::Lit("]}"));
                    // Push children in reverse, interleaving commas, so
                    // they emit in arena order as `c0,c1,c2`.
                    let kids = &node.children;
                    for (rev_i, &child) in kids.iter().enumerate().rev() {
                        stack.push(Work::Node(child));
                        if rev_i > 0 {
                            stack.push(Work::Lit(","));
                        }
                    }
                }
            }
        }
        out
    }

    /// The profile in Brendan Gregg's folded-stack format: one line per
    /// function of the form `top;child;grandchild self_nanos`, where the
    /// number is that frame's self time. Pipe it straight into
    /// `flamegraph.pl` to get an SVG. Functions with zero self time are
    /// omitted, as folded format expects.
    pub fn to_flamegraph(&self) -> String {
        let nodes = self.nodes();
        let mut out = String::new();
        let mut stack: Vec<(usize, String)> = Vec::new();
        for &child in nodes[ROOT].children.iter().rev() {
            stack.push((child, nodes[child].name.clone()));
        }
        while let Some((idx, path)) = stack.pop() {
            let node = &nodes[idx];
            if node.self_nanos > 0 {
                out.push_str(&format!("{path} {}\n", node.self_nanos));
            }
            for &child in node.children.iter().rev() {
                stack.push((child, format!("{path};{}", nodes[child].name)));
            }
        }
        out
    }
}

fn sort_by_total(nodes: &[Node], indices: &mut [usize]) {
    indices.sort_by(|&a, &b| nodes[b].total_nanos.cmp(&nodes[a].total_nanos));
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
