use crate::clock::Clock;
use crate::profiler::{Profiler, ROOT};

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
    pub fn text_report(&self) -> String {
        let root_total = self.root_total_nanos();
        let nodes = self.nodes();
        let mut out = String::new();
        out.push_str(&format!(
            "{:<32}{:>8}{:>12}{:>12}{:>9}\n",
            "name", "calls", "total(ms)", "self(ms)", "pct"
        ));

        let mut children: Vec<usize> = nodes[ROOT].children.clone();
        sort_by_total(nodes, &mut children);
        for idx in children {
            write_text_node(nodes, idx, 0, root_total, &mut out);
        }
        out
    }

    /// The call tree as a JSON string: `{ name, calls, total_nanos,
    /// self_nanos, children }`, rooted at a synthetic top-level object
    /// whose `total_nanos` is the sum of every top-level span, since
    /// the sentinel root is never entered directly.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_json_node(self.nodes(), ROOT, self.root_total_nanos(), &mut out);
        out
    }
}

fn sort_by_total(nodes: &[crate::profiler::Node], indices: &mut [usize]) {
    indices.sort_by(|&a, &b| nodes[b].total_nanos.cmp(&nodes[a].total_nanos));
}

fn write_text_node(
    nodes: &[crate::profiler::Node],
    idx: usize,
    depth: usize,
    root_total: u64,
    out: &mut String,
) {
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
    for child_idx in children {
        write_text_node(nodes, child_idx, depth + 1, root_total, out);
    }
}

fn write_json_node(
    nodes: &[crate::profiler::Node],
    idx: usize,
    root_total_override: u64,
    out: &mut String,
) {
    let node = &nodes[idx];
    let total_nanos = if idx == ROOT {
        root_total_override
    } else {
        node.total_nanos
    };

    out.push('{');
    out.push_str("\"name\":");
    write_json_string(&node.name, out);
    out.push_str(&format!(
        ",\"calls\":{},\"total_nanos\":{},\"self_nanos\":{},\"children\":[",
        node.calls, total_nanos, node.self_nanos
    ));
    for (i, &child_idx) in node.children.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_json_node(nodes, child_idx, root_total_override, out);
    }
    out.push_str("]}");
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
