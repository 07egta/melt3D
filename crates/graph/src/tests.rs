use super::*;

fn text_of(ev: &Evaluation, id: NodeId) -> String {
    match ev.outputs.get(&(id, 0)).and_then(|t| t.branches.values().next()).and_then(|l| l.first()) {
        Some(Value::Text(t)) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn numbers(ev: &Evaluation, id: NodeId) -> Vec<f64> {
    ev.outputs.get(&(id, 0)).map(|t| t.branches.values().flatten().filter_map(|v| v.number().ok()).collect()).unwrap_or_default()
}

#[test]
fn every_component_has_matching_defaults_and_unique_name() {
    let mut names = std::collections::BTreeSet::new();
    for c in COMPONENTS {
        assert_eq!((c.defaults)().len(), c.inputs.len(), "{}", c.name);
        assert!(names.insert(c.name), "duplicate {}", c.name);
    }
}

#[test]
fn numbers_flow_along_wires() {
    let mut g = Graph::new();
    let a = g.add_node("Number", (0.0, 0.0)).unwrap();
    let add = g.add_node("Add", (1.0, 0.0)).unwrap();
    g.set_constant(a, 0, Value::Number(4.0)).unwrap();
    g.connect(a, 0, add, 0).unwrap();
    g.set_constant(add, 1, Value::Number(6.0)).unwrap();
    let ev = g.evaluate().unwrap();
    assert_eq!(numbers(&ev, add), vec![10.0]);
}

#[test]
fn lists_match_item_by_item_and_the_short_one_repeats() {
    let mut g = Graph::new();
    let s = g.add_node("Series", (0.0, 0.0)).unwrap();
    g.set_constant(s, 2, Value::Number(3.0)).unwrap(); // 0, 1, 2
    let add = g.add_node("Add", (1.0, 0.0)).unwrap();
    g.connect(s, 0, add, 0).unwrap();
    g.set_constant(add, 1, Value::Number(10.0)).unwrap();
    assert_eq!(numbers(&g.evaluate().unwrap(), add), vec![10.0, 11.0, 12.0]);
}

#[test]
fn graft_and_flatten_change_the_tree() {
    let t = Tree { branches: BTreeMap::from([(vec![0], vec![Value::Number(1.0), Value::Number(2.0)]), (vec![1], vec![Value::Number(3.0)])]) };
    assert_eq!(t.flatten().branches.len(), 1);
    assert_eq!(t.flatten().item_count(), 3);
    assert_eq!(t.graft().branches.len(), 3);
}

#[test]
fn a_loop_of_wires_is_an_error_not_a_hang() {
    let mut g = Graph::new();
    let a = g.add_node("Add", (0.0, 0.0)).unwrap();
    let b = g.add_node("Add", (1.0, 0.0)).unwrap();
    g.connect(a, 0, b, 0).unwrap();
    g.connect(b, 0, a, 0).unwrap();
    assert_eq!(g.evaluate(), Err(GraphError::Cycle));
}

#[test]
fn a_failing_node_marks_itself_and_what_follows_but_not_the_rest() {
    let mut g = Graph::new();
    let d = g.add_node("Divide", (0.0, 0.0)).unwrap();
    g.set_constant(d, 1, Value::Number(0.0)).unwrap();
    let after = g.add_node("Add", (1.0, 0.0)).unwrap();
    g.connect(d, 0, after, 0).unwrap();
    let fine = g.add_node("Number", (0.0, 1.0)).unwrap();
    let ev = g.evaluate().unwrap();
    assert!(ev.errors.contains_key(&d));
    assert!(ev.errors.contains_key(&after));
    assert!(!ev.errors.contains_key(&fine));
}

#[test]
fn hostile_numbers_are_errors() {
    let mut g = Graph::new();
    let s = g.add_node("Series", (0.0, 0.0)).unwrap();
    g.set_constant(s, 2, Value::Number(1e12)).unwrap();
    let c = g.add_node("Circle", (0.0, 0.0)).unwrap();
    g.set_constant(c, 1, Value::Number(f64::NAN)).unwrap();
    let ev = g.evaluate().unwrap();
    assert!(ev.errors.contains_key(&s));
    assert!(ev.errors.contains_key(&c));
}

#[test]
fn bad_wires_are_refused() {
    let mut g = Graph::new();
    let a = g.add_node("Number", (0.0, 0.0)).unwrap();
    assert_eq!(g.connect(a, 5, a, 0), Err(GraphError::NoSocket));
    assert_eq!(g.connect(NodeId(99), 0, a, 0), Err(GraphError::NoNode));
    assert!(matches!(g.add_node("Nope", (0.0, 0.0)), Err(GraphError::UnknownComponent(_))));
}

#[test]
fn removing_a_node_cuts_its_wires() {
    let mut g = Graph::new();
    let a = g.add_node("Number", (0.0, 0.0)).unwrap();
    let b = g.add_node("Add", (1.0, 0.0)).unwrap();
    g.connect(a, 0, b, 0).unwrap();
    g.remove_node(a);
    assert!(g.nodes.get(&b).is_some_and(|n| n.inputs.first().is_some_and(|s| s.wire.is_none())));
    assert!(g.evaluate().unwrap().errors.is_empty());
}

/// circle -> linear pattern of 3 -> extrude -> merge -> slice -> G-code
#[test]
fn a_pattern_of_cylinders_becomes_g_code() {
    let mut g = Graph::new();
    let circle = g.add_node("Circle", (0.0, 0.0)).unwrap();
    g.set_constant(circle, 0, Value::Number(5.0)).unwrap();
    let pat = g.add_node("Linear Pattern", (1.0, 0.0)).unwrap();
    g.connect(circle, 0, pat, 0).unwrap();
    g.set_constant(pat, 1, Value::Number(3.0)).unwrap();
    g.set_constant(pat, 2, Value::Number(20.0)).unwrap();
    let ext = g.add_node("Extrude", (2.0, 0.0)).unwrap();
    g.connect(pat, 0, ext, 0).unwrap();
    g.set_constant(ext, 1, Value::Number(4.0)).unwrap();
    let merge = g.add_node("Merge Solids", (3.0, 0.0)).unwrap();
    g.connect(ext, 0, merge, 0).unwrap();
    let sl = g.add_node("Slice", (4.0, 0.0)).unwrap();
    g.connect(merge, 0, sl, 0).unwrap();
    let gc = g.add_node("G-code", (5.0, 0.0)).unwrap();
    g.connect(sl, 0, gc, 0).unwrap();

    let ev = g.evaluate().unwrap();
    assert!(ev.errors.is_empty(), "{:?}", ev.errors);
    let layers = match ev.outputs.get(&(sl, 0)).and_then(|t| t.branches.values().next()).and_then(|l| l.first()) {
        Some(Value::Layers(l)) => l.clone(),
        other => panic!("expected layers, got {other:?}"),
    };
    assert_eq!(layers.layers.len(), 20);
    assert!(layers.layers.iter().all(|l| l.loops.len() == 3));
    let text = text_of(&ev, gc);
    assert_eq!(text.matches(";LAYER:").count(), 20);
    assert!(text.contains("G1 X"));
}

#[test]
fn a_circular_pattern_makes_that_many_shapes() {
    let mut g = Graph::new();
    let r = g.add_node("Rectangle", (0.0, 0.0)).unwrap();
    let mv = g.add_node("Move", (1.0, 0.0)).unwrap();
    g.connect(r, 0, mv, 0).unwrap();
    g.set_constant(mv, 1, Value::Number(30.0)).unwrap();
    let pat = g.add_node("Circular Pattern", (2.0, 0.0)).unwrap();
    g.connect(mv, 0, pat, 0).unwrap();
    g.set_constant(pat, 1, Value::Number(5.0)).unwrap();
    let ev = g.evaluate().unwrap();
    assert!(ev.errors.is_empty());
    assert_eq!(ev.outputs.get(&(pat, 0)).map(Tree::item_count), Some(5));
}

#[test]
fn a_graph_can_be_stopped_part_way() {
    let mut g = Graph::new();
    g.add_node("Number", (0.0, 0.0)).unwrap();
    assert_eq!(g.evaluate_until(&|| true), Err(GraphError::Cancelled));
    assert!(g.evaluate_until(&|| false).is_ok());
}
