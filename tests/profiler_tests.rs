use stopwatch::{MockClock, ProfilerError, Profiler, MAX_DEPTH, MAX_NODES};

fn total_and_self_by_name(profiler: &Profiler<MockClock>, name: &str) -> Vec<(u64, u64, u64)> {
    profiler
        .nodes()
        .iter()
        .filter(|n| n.name == name)
        .map(|n| (n.calls, n.total_nanos, n.self_nanos))
        .collect()
}

#[test]
fn single_span_reports_exact_elapsed() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("work").unwrap();
    profiler.clock().advance(100);
    profiler.exit().unwrap();

    let rows = total_and_self_by_name(&profiler, "work");
    assert_eq!(rows, vec![(1, 100, 100)]);
}

#[test]
fn nested_span_splits_self_and_total() {
    // a spans 0..100, b spans 20..60 inside it.
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("a").unwrap();
    profiler.clock().advance(20); // now at 20
    profiler.enter("b").unwrap();
    profiler.clock().advance(40); // now at 60
    profiler.exit().unwrap(); // closes b
    profiler.clock().advance(40); // now at 100
    profiler.exit().unwrap(); // closes a

    let a = total_and_self_by_name(&profiler, "a");
    let b = total_and_self_by_name(&profiler, "b");
    assert_eq!(a, vec![(1, 100, 60)]);
    assert_eq!(b, vec![(1, 40, 40)]);
}

#[test]
fn sibling_children_both_excluded_from_parent_self() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("parent").unwrap();
    profiler.enter("child_a").unwrap();
    profiler.clock().advance(10);
    profiler.exit().unwrap(); // child_a: 0..10
    profiler.enter("child_b").unwrap();
    profiler.clock().advance(15);
    profiler.exit().unwrap(); // child_b: 10..25
    profiler.clock().advance(5); // parent's own time: 25..30
    profiler.exit().unwrap(); // parent: 0..30

    let parent = total_and_self_by_name(&profiler, "parent");
    let child_a = total_and_self_by_name(&profiler, "child_a");
    let child_b = total_and_self_by_name(&profiler, "child_b");
    assert_eq!(parent, vec![(1, 30, 5)]);
    assert_eq!(child_a, vec![(1, 10, 10)]);
    assert_eq!(child_b, vec![(1, 15, 15)]);
}

#[test]
fn repeated_span_name_aggregates() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    for _ in 0..3 {
        profiler.enter("work").unwrap();
        profiler.clock().advance(10);
        profiler.exit().unwrap();
    }

    let rows = total_and_self_by_name(&profiler, "work");
    assert_eq!(rows, vec![(3, 30, 30)]);
}

#[test]
fn recursion_nests_without_double_counting_self_time() {
    // fib(3)-shaped recursion: three nested levels of the same name.
    // Level 0: 0..90, level 1: 10..80, level 2: 30..60.
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("fib").unwrap(); // level 0 starts at 0
    profiler.clock().advance(10);
    profiler.enter("fib").unwrap(); // level 1 starts at 10
    profiler.clock().advance(20);
    profiler.enter("fib").unwrap(); // level 2 starts at 30
    profiler.clock().advance(30);
    profiler.exit().unwrap(); // level 2 closes at 60: total 30, self 30
    profiler.clock().advance(20);
    profiler.exit().unwrap(); // level 1 closes at 80: total 70, self 40
    profiler.clock().advance(10);
    profiler.exit().unwrap(); // level 0 closes at 90: total 90, self 20

    let rows = total_and_self_by_name(&profiler, "fib");
    // Three distinct nodes, one per recursion depth.
    assert_eq!(rows.len(), 3);
    let self_sum: u64 = rows.iter().map(|(_, _, s)| s).sum();
    let root_total: u64 = *rows.iter().map(|(_, t, _)| t).max().unwrap();
    assert_eq!(self_sum, root_total);
    assert_eq!(self_sum, 90);
}

#[test]
fn three_level_recursion_self_times_sum_to_root_total() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    fn recurse(profiler: &mut Profiler<MockClock>, depth: u32) {
        profiler.enter("recurse").unwrap();
        profiler.clock().advance(5);
        if depth > 0 {
            recurse(profiler, depth - 1);
        }
        profiler.clock().advance(5);
        profiler.exit().unwrap();
    }

    recurse(&mut profiler, 3);

    let root_total = profiler.root_total_nanos();
    let self_sum: u64 = profiler
        .nodes()
        .iter()
        .filter(|n| n.name == "recurse")
        .map(|n| n.self_nanos)
        .sum();
    assert_eq!(self_sum, root_total);
    assert_eq!(root_total, 40); // 4 levels * 10ns each
}

#[test]
fn epitaph_picks_the_max_self_time_node() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("cheap").unwrap();
    profiler.clock().advance(5);
    profiler.exit().unwrap();

    profiler.enter("expensive").unwrap();
    profiler.clock().advance(95);
    profiler.exit().unwrap();

    let epitaph = profiler.epitaph().expect("epitaph should exist");
    assert_eq!(epitaph.name, "expensive");
    assert_eq!(epitaph.self_nanos, 95);
    assert!((epitaph.percent_of_total - 95.0).abs() < 0.001);
    assert!(epitaph.message().contains("expensive"));
}

#[test]
fn top_level_percentages_sum_to_one_hundred() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("a").unwrap();
    profiler.clock().advance(30);
    profiler.exit().unwrap();

    profiler.enter("b").unwrap();
    profiler.clock().advance(70);
    profiler.exit().unwrap();

    let root_total = profiler.root_total_nanos();
    let pct_sum: f64 = profiler.nodes()[stopwatch::ROOT]
        .children
        .iter()
        .map(|&idx| profiler.nodes()[idx].total_nanos as f64 / root_total as f64 * 100.0)
        .sum();
    assert!((pct_sum - 100.0).abs() < 0.001);
}

#[test]
fn exceeding_max_depth_returns_typed_error_not_panic() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    for _ in 0..MAX_DEPTH {
        profiler.enter("frame").unwrap();
    }
    let result = profiler.enter("one_too_many");
    assert_eq!(
        result,
        Err(ProfilerError::MaxDepthExceeded { limit: MAX_DEPTH })
    );
}

#[test]
fn exceeding_max_nodes_returns_typed_error_not_panic() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    // MAX_NODES counts the sentinel root, so this creates MAX_NODES - 1
    // distinct children of root, filling the arena exactly.
    for i in 0..(MAX_NODES - 1) {
        let name = format!("n{i}");
        profiler.enter(&name).unwrap();
        profiler.exit().unwrap();
    }

    let result = profiler.enter("one_too_many");
    assert_eq!(
        result,
        Err(ProfilerError::MaxNodesExceeded { limit: MAX_NODES })
    );
}

#[test]
fn json_export_has_the_documented_shape() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);

    profiler.enter("outer").unwrap();
    profiler.clock().advance(10);
    profiler.enter("inner").unwrap();
    profiler.clock().advance(5);
    profiler.exit().unwrap();
    profiler.clock().advance(5);
    profiler.exit().unwrap();

    let json = profiler.to_json();
    assert!(json.contains("\"name\":\"root\""));
    assert!(json.contains("\"name\":\"outer\""));
    assert!(json.contains("\"name\":\"inner\""));
    assert!(json.contains("\"total_nanos\":20"));
    assert!(json.contains("\"children\":["));
}

#[test]
fn exit_without_enter_is_a_typed_error() {
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);
    assert_eq!(profiler.exit(), Err(ProfilerError::ExitWithoutEnter));
}

#[test]
fn flamegraph_folds_stacks_with_self_time() {
    // a spans 0..100 wrapping b at 20..60, so a.self == 60, b.self == 40.
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);
    profiler.enter("a").unwrap();
    profiler.clock().advance(20);
    profiler.enter("b").unwrap();
    profiler.clock().advance(40);
    profiler.exit().unwrap();
    profiler.clock().advance(40);
    profiler.exit().unwrap();

    assert_eq!(profiler.to_flamegraph(), "a 60\na;b 40\n");
}

#[test]
fn deep_tree_report_does_not_overflow_the_host_stack() {
    // Build a tree far deeper than a comfortable recursion limit and
    // confirm the iterative report and exports return without panicking.
    let clock = MockClock::new();
    let mut profiler = Profiler::new(clock);
    let depth = MAX_DEPTH - 1;
    for _ in 0..depth {
        profiler.enter("level").unwrap();
        profiler.clock().advance(1);
    }
    for _ in 0..depth {
        profiler.exit().unwrap();
    }
    assert!(!profiler.text_report().is_empty());
    assert!(profiler.to_json().starts_with('{'));
    assert!(!profiler.to_flamegraph().is_empty());
}
